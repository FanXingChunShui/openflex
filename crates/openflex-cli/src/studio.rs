//! `openflex studio` — a local, self-contained web UI for a model.
//!
//! The server is intentionally tiny: a blocking `std::net::TcpListener` on
//! `127.0.0.1`, hand-written HTTP/1.1, `serde_json` for the API, and a single
//! embedded HTML document with inline CSS/JavaScript and Canvas drawing. There
//! are no external assets, no CDN, no async runtime and no new dependencies.
//!
//! Endpoints:
//!
//! - `GET /` — the studio page
//! - `GET /api/model` — the current model (JSON)
//! - `POST /api/model` — validate and save an edited model (JSON)
//! - `GET /api/objects` — object-kind catalog for the editor
//! - `POST /api/run` — run the current model, returns a `SimReport` JSON
//! - `GET /health` — liveness probe
//!
//! Safety guards: the socket is bound to loopback only, request bodies are
//! capped at [`MAX_BODY`], headers at [`MAX_HEADER`], paths are a fixed
//! whitelist, and model saves are atomic (`write` + `rename`) after
//! `Model::from_json` validation.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

use openflex_model::Model;

use crate::objects;

/// Maximum accepted request body (2 MiB).
pub const MAX_BODY: usize = 2 * 1024 * 1024;
/// Maximum accepted request header block (64 KiB).
pub const MAX_HEADER: usize = 64 * 1024;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Serve the studio until the process is killed.
pub fn serve(model_path: &Path, port: u16, open: bool, quiet: bool) -> Result<(), String> {
    let text = fs::read_to_string(model_path)
        .map_err(|error| format!("could not read model {}: {error}", model_path.display()))?;
    let mut model = Model::from_json(&text)
        .map_err(|error| format!("invalid model {}: {error}", model_path.display()))?;
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("could not bind 127.0.0.1:{port}: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("could not read the listening address: {error}"))?;
    let url = format!("http://{address}");
    eprintln!(
        "openflex studio listening on {url} (model {})",
        model_path.display()
    );
    if !quiet {
        eprintln!("open {url} in a browser; press Ctrl-C to stop");
    }
    if open {
        open_browser(&url);
    }

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                if let Err(error) = handle_connection(stream, &mut model, model_path) {
                    eprintln!("studio request failed: {error}");
                }
            }
            Err(error) => eprintln!("studio connection failed: {error}"),
        }
    }
    Ok(())
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

struct HttpError {
    status: &'static str,
    message: String,
}

impl HttpError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: "400 Bad Request",
            message: message.into(),
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    model: &mut Model,
    model_path: &Path,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let request = match read_request(&stream) {
        Ok(request) => request,
        Err(error) => {
            write_json(
                &mut stream,
                error.status,
                &serde_json::json!({ "ok": false, "error": error.message }).to_string(),
            )?;
            return Ok(());
        }
    };

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/" | "/index.html") => write_response(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            &index_html(model),
        ),
        ("GET", "/api/model") => {
            let text = serde_json::to_string_pretty(&*model).unwrap_or_else(|_| "{}".into());
            write_response(&mut stream, "200 OK", "application/json", &text)
        }
        ("GET", "/api/objects") => write_response(
            &mut stream,
            "200 OK",
            "application/json",
            &objects::objects_json().to_string(),
        ),
        ("GET", "/health") => write_response(
            &mut stream,
            "200 OK",
            "application/json",
            &serde_json::json!({ "status": "ok", "version": VERSION }).to_string(),
        ),
        ("POST", "/api/model") => save_model(&mut stream, model, model_path, &request.body),
        ("POST", "/api/model/objects") => add_object(&mut stream, model, model_path, &request.body),
        ("POST", "/api/run") => run_model_endpoint(&mut stream, model, &request.body),
        (method, path) if method == "DELETE" && path.starts_with("/api/model/objects/") => {
            let name = match percent_decode(path.trim_start_matches("/api/model/objects/")) {
                Ok(name) => name,
                Err(error) => {
                    return write_json(
                        &mut stream,
                        "400 Bad Request",
                        &serde_json::json!({ "ok": false, "error": error }).to_string(),
                    );
                }
            };
            delete_object(&mut stream, model, model_path, &name)
        }
        ("GET", path) if path.starts_with("/api/") || path.starts_with("/assets/") => {
            not_found(&mut stream)
        }
        _ => not_found(&mut stream),
    }
}

fn not_found(stream: &mut TcpStream) -> std::io::Result<()> {
    write_json(
        stream,
        "404 Not Found",
        &serde_json::json!({ "ok": false, "error": "unknown endpoint" }).to_string(),
    )
}

fn save_model(
    stream: &mut TcpStream,
    model: &mut Model,
    model_path: &Path,
    body: &[u8],
) -> std::io::Result<()> {
    let text = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(error) => {
            return write_json(
                stream,
                "400 Bad Request",
                &serde_json::json!({ "ok": false, "error": format!("body is not UTF-8: {error}") })
                    .to_string(),
            );
        }
    };
    let new_model = match Model::from_json(text) {
        Ok(model) => model,
        Err(error) => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("invalid model: {error}") })
                    .to_string(),
            );
        }
    };
    let pretty = match serde_json::to_string_pretty(&new_model) {
        Ok(pretty) => format!("{pretty}\n"),
        Err(error) => {
            return write_json(
                stream,
                "500 Internal Server Error",
                &serde_json::json!({ "ok": false, "error": error.to_string() }).to_string(),
            );
        }
    };
    if let Err(error) = write_atomic(model_path, &pretty) {
        return write_json(
            stream,
            "500 Internal Server Error",
            &serde_json::json!({ "ok": false, "error": error }).to_string(),
        );
    }
    *model = new_model;
    write_json(
        stream,
        "200 OK",
        &serde_json::json!({ "ok": true, "name": model.name, "objects": model.objects.len() })
            .to_string(),
    )
}

#[derive(Debug, Default, serde::Deserialize)]
struct RunOverrides {
    stop_time: Option<f64>,
    seed: Option<u64>,
    replications: Option<u32>,
}

fn run_model_endpoint(stream: &mut TcpStream, model: &Model, body: &[u8]) -> std::io::Result<()> {
    let overrides: RunOverrides = if body.is_empty() || body == b"{}" {
        RunOverrides::default()
    } else {
        match serde_json::from_slice(body) {
            Ok(overrides) => overrides,
            Err(error) => {
                return write_json(
                    stream,
                    "400 Bad Request",
                    &serde_json::json!({ "ok": false, "error": format!("invalid run options: {error}") })
                        .to_string(),
                );
            }
        }
    };
    let mut run_model = model.clone();
    if let Some(stop_time) = overrides.stop_time {
        if stop_time <= 0.0 || stop_time <= run_model.run.warmup {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": "stop_time must be greater than warm-up" })
                    .to_string(),
            );
        }
        run_model.run.stop_time = stop_time;
    }
    if let Some(seed) = overrides.seed {
        run_model.run.seed = seed;
    }
    if let Some(replications) = overrides.replications {
        if replications == 0 {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": "replications must be at least 1" })
                    .to_string(),
            );
        }
        run_model.run.replications = replications;
    }
    if let Err(error) = run_model.validate() {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": format!("invalid model: {error}") })
                .to_string(),
        );
    }
    match openflex_core::run_model(&run_model) {
        Ok(report) => {
            let text = serde_json::to_string(&report).unwrap_or_else(|_| "{}".into());
            write_response(stream, "200 OK", "application/json", &text)
        }
        Err(error) => {
            let status = match error {
                openflex_core::SimError::Config(_) | openflex_core::SimError::Model(_) => {
                    "422 Unprocessable Entity"
                }
                _ => "500 Internal Server Error",
            };
            write_json(
                stream,
                status,
                &serde_json::json!({ "ok": false, "error": error.to_string() }).to_string(),
            )
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct AddObjectRequest {
    kind: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    pos: Option<[f64; 2]>,
}

/// Add an object with kind-specific safe defaults, then validate and save.
fn add_object(
    stream: &mut TcpStream,
    model: &mut Model,
    model_path: &Path,
    body: &[u8],
) -> std::io::Result<()> {
    let request: AddObjectRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return write_json(
                stream,
                "400 Bad Request",
                &serde_json::json!({ "ok": false, "error": format!("invalid add request: {error}") })
                    .to_string(),
            );
        }
    };
    let kind = match openflex_model::ObjectKind::parse(&request.kind.to_ascii_lowercase()) {
        Some(kind) => kind,
        None => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("unknown object kind '{}'", request.kind) })
                    .to_string(),
            );
        }
    };
    let mut value = serde_json::to_value(&*model).unwrap_or_else(|_| serde_json::json!({}));
    let Some(objects) = value
        .get_mut("objects")
        .and_then(|objects| objects.as_array_mut())
    else {
        return write_json(
            stream,
            "500 Internal Server Error",
            &serde_json::json!({ "ok": false, "error": "model has no objects array" }).to_string(),
        );
    };
    let name = match request
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
    {
        Some(name) => name,
        None => unique_object_name(objects, kind.as_str()),
    };
    if objects
        .iter()
        .any(|object| object.get("name").and_then(|name| name.as_str()) == Some(name.as_str()))
    {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": format!("an object named '{name}' already exists") })
                .to_string(),
        );
    }
    let pos = request.pos.unwrap_or_else(|| next_position(objects));
    objects.push(serde_json::json!({
        "name": name,
        "kind": kind.as_str(),
        "pos": pos,
        "params": default_params(kind),
    }));
    let new_model = match Model::from_json(&value.to_string()) {
        Ok(model) => model,
        Err(error) => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("invalid model after add: {error}") })
                    .to_string(),
            );
        }
    };
    persist_model(
        stream,
        model,
        model_path,
        new_model,
        serde_json::json!({ "added": name }),
    )
}

/// Delete one object and clean connections/references to it, then validate.
fn delete_object(
    stream: &mut TcpStream,
    model: &mut Model,
    model_path: &Path,
    name: &str,
) -> std::io::Result<()> {
    let mut value = serde_json::to_value(&*model).unwrap_or_else(|_| serde_json::json!({}));
    let Some(objects) = value
        .get_mut("objects")
        .and_then(|objects| objects.as_array_mut())
    else {
        return write_json(
            stream,
            "500 Internal Server Error",
            &serde_json::json!({ "ok": false, "error": "model has no objects array" }).to_string(),
        );
    };
    let Some(index) = objects
        .iter()
        .position(|object| object.get("name").and_then(|name| name.as_str()) == Some(name))
    else {
        return write_json(
            stream,
            "404 Not Found",
            &serde_json::json!({ "ok": false, "error": format!("unknown object '{name}'") })
                .to_string(),
        );
    };
    objects.remove(index);
    let connections_removed = value
        .get_mut("connections")
        .and_then(|connections| connections.as_array_mut())
        .map(|connections| {
            let before = connections.len();
            connections.retain(|connection| {
                connection.get("from").and_then(|from| from.as_str()) != Some(name)
                    && connection.get("to").and_then(|to| to.as_str()) != Some(name)
            });
            before - connections.len()
        })
        .unwrap_or(0);
    let references_cleaned = value
        .get_mut("objects")
        .and_then(|objects| objects.as_array_mut())
        .map(|objects| {
            objects
                .iter_mut()
                .map(|object| {
                    object
                        .get_mut("params")
                        .map(|params| clean_references(params, name))
                        .unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0);
    let new_model = match Model::from_json(&value.to_string()) {
        Ok(model) => model,
        Err(error) => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("invalid model after delete: {error}") })
                    .to_string(),
            );
        }
    };
    persist_model(
        stream,
        model,
        model_path,
        new_model,
        serde_json::json!({
            "removed": name,
            "connections_removed": connections_removed,
            "references_cleaned": references_cleaned,
        }),
    )
}

/// Remove `home`/`charger`/`dispatcher`-style references to a deleted object.
fn clean_references(value: &mut serde_json::Value, name: &str) -> usize {
    match value {
        serde_json::Value::Object(map) => {
            let mut cleaned = 0;
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                let remove = matches!(
                    key.as_str(),
                    "home"
                        | "charger"
                        | "dispatcher"
                        | "home_floor"
                        | "setup_operator"
                        | "process_operator"
                ) && map.get(&key).and_then(|value| value.as_str()) == Some(name);
                if remove {
                    map.remove(&key);
                    cleaned += 1;
                } else if let Some(child) = map.get_mut(&key) {
                    cleaned += clean_references(child, name);
                }
            }
            cleaned
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .map(|item| clean_references(item, name))
            .sum(),
        _ => 0,
    }
}

fn default_params(kind: openflex_model::ObjectKind) -> serde_json::Value {
    if kind == openflex_model::ObjectKind::Source {
        // The engine default inter-arrival is 0, which would create items at
        // the same instant forever; a studio-added source gets a safe default.
        serde_json::json!({ "interarrival": { "distribution": "constant", "value": 5.0 } })
    } else {
        serde_json::json!({})
    }
}

fn unique_object_name(objects: &[serde_json::Value], kind: &str) -> String {
    let prefix = {
        let mut chars = kind.chars();
        match chars.next() {
            Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
            None => "Object".to_string(),
        }
    };
    for suffix in 1.. {
        let candidate = format!("{prefix}{suffix}");
        if !objects.iter().any(|object| {
            object.get("name").and_then(|name| name.as_str()) == Some(candidate.as_str())
        }) {
            return candidate;
        }
    }
    unreachable!("an unused suffix always exists")
}

fn next_position(objects: &[serde_json::Value]) -> [f64; 2] {
    let max_x = objects
        .iter()
        .filter_map(|object| {
            object
                .get("pos")
                .and_then(|pos| pos.as_array())
                .and_then(|pos| pos.first())
                .and_then(|x| x.as_f64())
        })
        .fold(0.0_f64, f64::max);
    [max_x + 150.0, 200.0]
}

/// Persist a validated model atomically and return it to the UI.
fn persist_model(
    stream: &mut TcpStream,
    model: &mut Model,
    model_path: &Path,
    new_model: Model,
    extra: serde_json::Value,
) -> std::io::Result<()> {
    let pretty = match serde_json::to_string_pretty(&new_model) {
        Ok(pretty) => format!("{pretty}\n"),
        Err(error) => {
            return write_json(
                stream,
                "500 Internal Server Error",
                &serde_json::json!({ "ok": false, "error": error.to_string() }).to_string(),
            );
        }
    };
    if let Err(error) = write_atomic(model_path, &pretty) {
        return write_json(
            stream,
            "500 Internal Server Error",
            &serde_json::json!({ "ok": false, "error": error }).to_string(),
        );
    }
    *model = new_model;
    let mut response = serde_json::json!({ "ok": true, "model": *model });
    if let (Some(target), Some(source)) = (response.as_object_mut(), extra.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    write_json(stream, "200 OK", &response.to_string())
}

fn percent_decode(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Err(format!("invalid percent-encoding in '{input}'"));
                }
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                    .map_err(|_| format!("invalid percent-encoding in '{input}'"))?;
                let byte = u8::from_str_radix(hex, 16)
                    .map_err(|_| format!("invalid percent-encoding in '{input}'"))?;
                decoded.push(byte);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|_| format!("invalid UTF-8 in path '{input}'"))
}

/// Read one HTTP request with strict header/body limits.
fn read_request(stream: &TcpStream) -> Result<Request, HttpError> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|error| HttpError {
        status: "500 Internal Server Error",
        message: error.to_string(),
    })?);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|error| HttpError::bad_request(format!("could not read request line: {error}")))?;
    if request_line.is_empty() {
        return Err(HttpError::bad_request("empty request"));
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default();
    if method.is_empty() || target.is_empty() {
        return Err(HttpError::bad_request("malformed request line"));
    }
    let path = target.split('?').next().unwrap_or_default().to_string();

    let mut header_bytes = request_line.len();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| HttpError::bad_request(format!("could not read headers: {error}")))?;
        if read == 0 {
            break;
        }
        header_bytes += read;
        if header_bytes > MAX_HEADER {
            return Err(HttpError {
                status: "431 Request Header Fields Too Large",
                message: "request headers exceed the 64 KiB limit".into(),
            });
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim();
            if name == "content-length" {
                content_length = value.parse::<usize>().map_err(|error| {
                    HttpError::bad_request(format!("invalid Content-Length: {error}"))
                })?;
            }
            if name == "transfer-encoding" {
                return Err(HttpError::bad_request(
                    "Transfer-Encoding is not supported; send Content-Length",
                ));
            }
        }
    }
    if content_length > MAX_BODY {
        // Reject from the headers alone: do not read an attacker-controlled
        // amount of data just to answer it.
        return Err(HttpError {
            status: "413 Payload Too Large",
            message: format!("request body exceeds the {MAX_BODY} byte limit"),
        });
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader
            .read_exact(&mut body)
            .map_err(|error| HttpError::bad_request(format!("could not read body: {error}")))?;
    }
    Ok(Request { method, path, body })
}

fn write_json(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    write_response(stream, status, "application/json", body)
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let directory = parent.unwrap_or_else(|| Path::new("."));
    let temporary = directory.join(format!(".openflex-studio-{}.tmp", std::process::id()));
    if let Err(error) = fs::write(&temporary, text) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("could not write {}: {error}", temporary.display()));
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("could not replace {}: {error}", path.display()));
    }
    Ok(())
}

fn index_html(model: &Model) -> String {
    include_str!("studio.html")
        .replace("__MODEL_NAME__", &escape_html(&model.name))
        .replace("__MODEL_FILE__", &escape_html(&model.name))
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn open_browser(url: &str) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(error) = result {
        eprintln!("could not open a browser ({error}); open {url} manually");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escapes_the_model_name() {
        let model = Model::from_json(
            r#"{"format":"openflex-model","version":1,"name":"a<b","objects":[{"name":"S","kind":"source"},{"name":"K","kind":"sink"}],"connections":[{"from":"S","to":"K"}]}"#,
        )
        .expect("fixture");
        let html = index_html(&model);
        assert!(html.contains("a&lt;b"));
        assert!(html.contains("OpenFlex Studio"));
    }

    #[test]
    fn request_size_limits_are_documented_values() {
        assert_eq!(MAX_BODY, 2 * 1024 * 1024);
        assert_eq!(MAX_HEADER, 64 * 1024);
    }
}
