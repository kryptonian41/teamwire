//! An OTLP receiver on `127.0.0.1`: records every request and answers with
//! the status its mode names.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use prost::Message;

#[derive(Clone)]
pub(crate) struct Req {
    pub(crate) path: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

#[derive(Clone, Copy)]
pub(crate) enum Mode {
    Ok,
    Status(u16),
    /// Reads the request, waits, then answers 200.
    Delay(Duration),
}

pub(crate) struct Stub {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<Req>>>,
}

impl Stub {
    pub(crate) fn start(mode: Mode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let seen = Arc::clone(&seen);
                std::thread::spawn(move || serve(stream, mode, &seen));
            }
        });
        Self { addr, requests }
    }

    pub(crate) fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub(crate) fn requests(&self) -> Vec<Req> {
        self.requests.lock().unwrap().clone()
    }

    pub(crate) fn metrics(&self) -> Vec<ExportMetricsServiceRequest> {
        self.requests()
            .iter()
            .filter(|r| r.path == "/v1/metrics")
            .map(|r| ExportMetricsServiceRequest::decode(&r.body[..]).unwrap())
            .collect()
    }

    pub(crate) fn traces(&self) -> Vec<ExportTraceServiceRequest> {
        self.requests()
            .iter()
            .filter(|r| r.path == "/v1/traces")
            .map(|r| ExportTraceServiceRequest::decode(&r.body[..]).unwrap())
            .collect()
    }
}

/// An endpoint nothing listens on.
pub(crate) fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{addr}")
}

fn serve(mut stream: TcpStream, mode: Mode, seen: &Mutex<Vec<Req>>) {
    let Some(req) = read_request(&mut stream) else {
        return;
    };
    seen.lock().unwrap().push(req);
    let status = match mode {
        Mode::Ok => 200,
        Mode::Status(code) => code,
        Mode::Delay(wait) => {
            std::thread::sleep(wait);
            200
        }
    };
    write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .ok();
}

fn read_request(stream: &mut TcpStream) -> Option<Req> {
    let mut raw = Vec::new();
    let mut chunk = [0_u8; 4096];
    let head_end = loop {
        if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let mut lines = head.lines();
    let path = lines.next()?.split(' ').nth(1)?.to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = raw[head_end..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    Some(Req {
        path,
        headers,
        body,
    })
}
