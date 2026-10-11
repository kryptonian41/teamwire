//! Where the export goes: `OTEL_EXPORTER_OTLP_*` in the environment, else the
//! same keys in `$TEAMWIRE_HOME/config.toml` (ADR 0005 point 4a).

use std::path::Path;
use std::time::Duration;

use crate::OtlpConfig;

/// How long one export call may wait for a collector, unless
/// `OTEL_EXPORTER_OTLP_TIMEOUT` (milliseconds) says otherwise.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// The endpoint, headers and timeout, or `None` when no endpoint is set
/// (export is off). `env` looks up one environment variable; an empty value
/// counts as unset. Header values may write `,` as `%2C` and `%` as `%25`.
pub fn load_otlp_config(home: &Path, env: &dyn Fn(&str) -> Option<String>) -> Option<OtlpConfig> {
    let file = std::fs::read_to_string(home.join("config.toml")).unwrap_or_default();
    let from_env = |key: &str| env(key).filter(|v| !v.is_empty());
    let setting = |key: &str| from_env(key).or_else(|| file_setting(&file, key));
    let endpoint_from_env = from_env("OTEL_EXPORTER_OTLP_ENDPOINT");
    let endpoint = endpoint_from_env
        .clone()
        .or_else(|| file_setting(&file, "OTEL_EXPORTER_OTLP_ENDPOINT"))?;
    // Credentials in the file were written for the file's endpoint. An
    // endpoint chosen in the environment must not receive them.
    let headers = if endpoint_from_env.is_some() {
        from_env("OTEL_EXPORTER_OTLP_HEADERS")
    } else {
        setting("OTEL_EXPORTER_OTLP_HEADERS")
    }
    .map(|list| parse_headers(&list))
    .unwrap_or_default();
    let timeout = setting("OTEL_EXPORTER_OTLP_TIMEOUT")
        .and_then(|ms| ms.parse().ok())
        .map_or(DEFAULT_TIMEOUT, Duration::from_millis);
    Some(OtlpConfig {
        endpoint,
        headers,
        timeout,
    })
}

/// `KEY = value` on one line, with the value in double quotes, single quotes
/// or bare. A `#` after the value, outside quotes, starts a comment. The file
/// is small and flat, so this reads only that shape.
fn file_setting(file: &str, key: &str) -> Option<String> {
    file.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| value_of(v.trim())).flatten()
    })
}

fn value_of(text: &str) -> Option<String> {
    let mut chars = text.chars();
    match chars.next()? {
        '"' => {
            let mut out = String::new();
            let mut escaped = false;
            for c in chars {
                match (escaped, c) {
                    (true, c) => {
                        out.push(c);
                        escaped = false;
                    }
                    (false, '\\') => escaped = true,
                    (false, '"') => return Some(out),
                    (false, c) => out.push(c),
                }
            }
            None
        }
        '\'' => text.get(1..)?.split_once('\'').map(|(v, _)| v.to_owned()),
        _ => {
            let bare = text.split_once(" #").map_or(text, |(v, _)| v);
            let bare = bare.trim();
            (!bare.is_empty() && !bare.starts_with('#')).then(|| bare.to_owned())
        }
    }
}

/// `name=value,name2=value2`, with `%2C` and `%25` in values as the
/// exporter specification writes them.
fn parse_headers(list: &str) -> Vec<(String, String)> {
    list.split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| {
            (
                k.trim().to_owned(),
                v.replace("%2C", ",").replace("%25", "%"),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
