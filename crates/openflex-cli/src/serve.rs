//! Minimal HTTP service for `openflex serve`.
//!
//! Two read-only endpoints are enough to make the CLI useful from scripts and
//! dashboards:
//!
//! - `GET /health` — `{"status":"ok","version":"…"}`
//! - `GET /objects` — the object-kind catalog `openflex objects list` prints
//!
//! This is deliberately not a production server: it binds `127.0.0.1` only,
//! serves one request per connection, has no authentication and runs the
//! requests sequentially. `/run` is not exposed yet because it would need
//! request limits, cancellation and a job model; see `docs/BACKLOG.md` (C4).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use serde_json::{Value, json};

use crate::objects;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bind `127.0.0.1:port` (port `0` picks a free port) and serve until killed.
pub fn serve(port: u16, quiet: bool) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("could not bind 127.0.0.1:{port}: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("could not read the listening address: {error}"))?;
    if !quiet {
        eprintln!("listening on http://{address}");
    }
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                if let Err(error) = handle(stream) {
                    eprintln!("request failed: {error}");
                }
            }
            Err(error) => eprintln!("connection failed: {error}"),
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let mut request_line = String::new();
    {
        let mut reader = BufReader::new(&stream);
        reader.read_line(&mut request_line)?;
        // Drain the headers so the client is not left waiting on a reset.
        let mut header = String::new();
        loop {
            header.clear();
            if reader.read_line(&mut header)? == 0 || header == "\r\n" || header == "\n" {
                break;
            }
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let path = target.split('?').next().unwrap_or_default();

    match (method, path) {
        ("GET", "/health" | "/health/") => {
            respond(&mut stream, "200 OK", &health_json().to_string())
        }
        ("GET", "/objects" | "/objects/") => {
            respond(&mut stream, "200 OK", &objects::objects_json().to_string())
        }
        ("GET", "/" | "") => respond(&mut stream, "200 OK", &index_json().to_string()),
        ("GET", _) => respond(
            &mut stream,
            "404 Not Found",
            &json!({"error": {"code": "not_found", "message": "unknown endpoint"}}).to_string(),
        ),
        _ => respond(
            &mut stream,
            "405 Method Not Allowed",
            &json!({"error": {"code": "method_not_allowed", "message": "only GET is supported"}})
                .to_string(),
        ),
    }
}

fn health_json() -> Value {
    json!({ "status": "ok", "version": VERSION })
}

fn index_json() -> Value {
    json!({
        "service": "openflex",
        "version": VERSION,
        "endpoints": [
            { "method": "GET", "path": "/health", "description": "liveness and version" },
            { "method": "GET", "path": "/objects", "description": "object kind catalog" }
        ]
    })
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_reports_the_package_version() {
        let health = health_json();
        assert_eq!(health["status"], "ok");
        assert_eq!(health["version"], VERSION);
    }

    #[test]
    fn objects_payload_lists_every_kind() {
        let payload = objects::objects_json();
        let count = payload["count"].as_u64().expect("count");
        assert_eq!(
            count as usize,
            objects::objects_json()["objects"]
                .as_array()
                .expect("objects")
                .len()
        );
        assert!(count >= 15, "all object kinds are advertised");
        assert!(
            payload["objects"]
                .as_array()
                .expect("objects")
                .iter()
                .any(|object| object["kind"] == "processor")
        );
    }
}
