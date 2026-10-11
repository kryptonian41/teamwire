#![expect(
    clippy::disallowed_methods,
    reason = "the test writes a config file into a temporary home"
)]

use std::fs;

use teamwire_fakes::TestHome;

use super::*;

fn none(_: &str) -> Option<String> {
    None
}

#[test]
fn the_environment_wins_over_the_file() {
    let home = TestHome::new().unwrap();
    fs::write(
        home.path().join("config.toml"),
        "OTEL_EXPORTER_OTLP_ENDPOINT = \"http://file:4318\"\n",
    )
    .unwrap();
    let env = |k: &str| (k == "OTEL_EXPORTER_OTLP_ENDPOINT").then(|| "http://env:4318".to_owned());
    assert_eq!(
        load_otlp_config(home.path(), &env).unwrap().endpoint,
        "http://env:4318"
    );
    assert_eq!(
        load_otlp_config(home.path(), &none).unwrap().endpoint,
        "http://file:4318"
    );
}

#[test]
fn no_endpoint_means_export_is_off() {
    let home = TestHome::new().unwrap();
    assert!(load_otlp_config(home.path(), &none).is_none());
    let empty = |_: &str| Some(String::new());
    assert!(load_otlp_config(home.path(), &empty).is_none());
}

#[test]
fn headers_and_timeout_are_read_and_decoded() {
    let home = TestHome::new().unwrap();
    let env = |k: &str| match k {
        "OTEL_EXPORTER_OTLP_ENDPOINT" => Some("http://c:4318".to_owned()),
        "OTEL_EXPORTER_OTLP_HEADERS" => {
            Some("authorization=Bearer a%2Cb%25,x-team=alpha".to_owned())
        }
        "OTEL_EXPORTER_OTLP_TIMEOUT" => Some("2500".to_owned()),
        _ => None,
    };
    let cfg = load_otlp_config(home.path(), &env).unwrap();
    assert_eq!(
        cfg.headers,
        vec![
            ("authorization".to_owned(), "Bearer a,b%".to_owned()),
            ("x-team".to_owned(), "alpha".to_owned()),
        ]
    );
    assert_eq!(cfg.timeout, Duration::from_millis(2500));
    let plain = |k: &str| (k == "OTEL_EXPORTER_OTLP_ENDPOINT").then(|| "http://c".to_owned());
    assert_eq!(
        load_otlp_config(home.path(), &plain).unwrap().timeout,
        DEFAULT_TIMEOUT
    );
}

#[test]
fn file_values_drop_inline_comments_outside_quotes() {
    let file = "\
# a comment line
OTEL_EXPORTER_OTLP_ENDPOINT = \"http://c:4318/#frag\" # trailing
OTEL_EXPORTER_OTLP_TIMEOUT = 3000 # ms
OTEL_EXPORTER_OTLP_HEADERS = 'a=b # not a comment'
";
    assert_eq!(
        file_setting(file, "OTEL_EXPORTER_OTLP_ENDPOINT").as_deref(),
        Some("http://c:4318/#frag")
    );
    assert_eq!(
        file_setting(file, "OTEL_EXPORTER_OTLP_TIMEOUT").as_deref(),
        Some("3000")
    );
    assert_eq!(
        file_setting(file, "OTEL_EXPORTER_OTLP_HEADERS").as_deref(),
        Some("a=b # not a comment")
    );
    assert_eq!(file_setting(file, "MISSING"), None);
}

#[test]
fn headers_from_the_file_never_go_to_an_endpoint_from_the_environment() {
    let home = TestHome::new().unwrap();
    fs::write(
        home.path().join("config.toml"),
        "OTEL_EXPORTER_OTLP_ENDPOINT = \"http://file:4318\"\nOTEL_EXPORTER_OTLP_HEADERS = \"authorization=secret\"\n",
    )
    .unwrap();
    let env = |k: &str| (k == "OTEL_EXPORTER_OTLP_ENDPOINT").then(|| "http://env:4318".to_owned());
    let cfg = load_otlp_config(home.path(), &env).unwrap();
    assert_eq!(cfg.endpoint, "http://env:4318");
    assert!(cfg.headers.is_empty(), "{:?}", cfg.headers);
    // With no endpoint in the environment, the file's headers go with the file's endpoint.
    let cfg = load_otlp_config(home.path(), &none).unwrap();
    assert_eq!(cfg.headers.len(), 1);
}

/// Credentials from the environment were meant for an endpoint in the
/// environment. With the endpoint from the file, they stay home, the file's
/// own headers go with the file's endpoint, and a warning says so.
#[test]
fn headers_from_the_environment_never_go_to_an_endpoint_from_the_file() {
    let home = TestHome::new().unwrap();
    fs::write(
        home.path().join("config.toml"),
        "OTEL_EXPORTER_OTLP_ENDPOINT = \"http://file:4318\"\n",
    )
    .unwrap();
    let env =
        |k: &str| (k == "OTEL_EXPORTER_OTLP_HEADERS").then(|| "authorization=secret".to_owned());
    let (cfg, left_out) = resolve(home.path(), &env);
    let cfg = cfg.unwrap();
    assert_eq!(cfg.endpoint, "http://file:4318");
    assert!(cfg.headers.is_empty(), "{:?}", cfg.headers);
    assert!(
        left_out.unwrap().contains("from the environment"),
        "{left_out:?}"
    );

    fs::write(
        home.path().join("config.toml"),
        "OTEL_EXPORTER_OTLP_ENDPOINT = \"http://file:4318\"\nOTEL_EXPORTER_OTLP_HEADERS = \"x-file=1\"\n",
    )
    .unwrap();
    let (cfg, left_out) = resolve(home.path(), &env);
    assert_eq!(
        cfg.unwrap().headers,
        vec![("x-file".to_owned(), "1".to_owned())]
    );
    assert!(left_out.is_some());
}

/// Each way round, headers left out are reported; headers from the
/// endpoint's own place are not.
#[test]
fn headers_left_out_are_reported_and_headers_sent_are_not() {
    let home = TestHome::new().unwrap();
    fs::write(
        home.path().join("config.toml"),
        "OTEL_EXPORTER_OTLP_ENDPOINT = \"http://file:4318\"\nOTEL_EXPORTER_OTLP_HEADERS = \"authorization=secret\"\n",
    )
    .unwrap();
    let env_endpoint =
        |k: &str| (k == "OTEL_EXPORTER_OTLP_ENDPOINT").then(|| "http://env:4318".to_owned());
    let (cfg, left_out) = resolve(home.path(), &env_endpoint);
    assert!(cfg.unwrap().headers.is_empty());
    assert!(
        left_out.unwrap().contains("from config.toml"),
        "{left_out:?}"
    );
    assert_eq!(resolve(home.path(), &none).1, None);
    let env_both = |k: &str| match k {
        "OTEL_EXPORTER_OTLP_ENDPOINT" => Some("http://env:4318".to_owned()),
        "OTEL_EXPORTER_OTLP_HEADERS" => Some("x-env=1".to_owned()),
        _ => None,
    };
    let (cfg, left_out) = resolve(home.path(), &env_both);
    assert_eq!(
        cfg.unwrap().headers,
        vec![("x-env".to_owned(), "1".to_owned())]
    );
    assert_eq!(left_out, None);
}
