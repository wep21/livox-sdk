//! Small loopback-only HTTP endpoint for the bundled viewer, not a general server.
use crate::{Error, Source};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::{Duration, Instant},
};

const MAX_REQUEST: usize = 8192;

struct Request {
    method: String,
    path: String,
    host: String,
    origin: Option<String>,
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Request> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    let invalid = || std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid HTTP request");
    loop {
        if bytes.len() >= MAX_REQUEST || Instant::now() >= deadline {
            return Err(invalid());
        }
        let mut b = [0; 1024];
        let n = stream.read(&mut b)?;
        if n == 0 {
            return Err(invalid());
        }
        bytes.extend_from_slice(&b[..n]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            if end + 4 != bytes.len() || bytes.len() > MAX_REQUEST {
                return Err(invalid());
            }
            break;
        }
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next().ok_or_else(invalid)?.split_whitespace();
    let method = first.next().ok_or_else(invalid)?.to_owned();
    let path = first.next().ok_or_else(invalid)?.to_owned();
    if first.next() != Some("HTTP/1.1") || first.next().is_some() {
        return Err(invalid());
    }
    let mut host = None;
    let mut origin = None;
    for line in lines.filter(|l| !l.is_empty()) {
        let (key, value) = line.split_once(':').ok_or_else(invalid)?;
        let value = value.trim();
        match key.to_ascii_lowercase().as_str() {
            "host" if host.is_none() => host = Some(value.to_owned()),
            "origin" if origin.is_none() => origin = Some(value.to_owned()),
            "host" | "origin" | "transfer-encoding" => return Err(invalid()),
            "content-length" if value != "0" => return Err(invalid()),
            _ => {}
        }
    }
    Ok(Request {
        method,
        path,
        host: host.ok_or_else(invalid)?,
        origin,
    })
}

fn respond(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; frame-ancestors 'none'; base-uri 'none'\r\n\r\n",
        bytes.len()
    )?;
    stream.write_all(bytes)
}

pub fn serve(listener: TcpListener, mut source: Source) -> Result<(), Error> {
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "HTTP must bind loopback",
        )
        .into());
    }
    let host = listener.local_addr()?.to_string();
    let origin = format!("http://{host}");
    for stream in listener.incoming() {
        let mut stream = stream?;
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let request = match read_request(&mut stream) {
            Ok(r) => r,
            Err(_) => {
                let _ = respond(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain",
                    b"Invalid request",
                );
                continue;
            }
        };
        if request.host != host || request.origin.as_ref().is_some_and(|o| *o != origin) {
            let _ = respond(
                &mut stream,
                "403 Forbidden",
                "text/plain",
                b"Use the printed loopback URL",
            );
            continue;
        }
        let result = match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/") => respond(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                include_bytes!("../web/index.html"),
            ),
            ("GET", "/viewer.js") => respond(
                &mut stream,
                "200 OK",
                "text/javascript; charset=utf-8",
                include_bytes!("../web/viewer.js"),
            ),
            ("GET", "/style.css") => respond(
                &mut stream,
                "200 OK",
                "text/css; charset=utf-8",
                include_bytes!("../web/style.css"),
            ),
            ("GET", "/api/info") => respond(
                &mut stream,
                "200 OK",
                "application/json",
                &serde_json::to_vec(source.info())?,
            ),
            ("GET", "/api/cloud") => data(&mut stream, source.snapshot()),
            ("POST", "/api/next") => data(&mut stream, source.advance()),
            ("POST", "/api/reset") => data(&mut stream, source.reset()),
            _ => respond(&mut stream, "404 Not Found", "text/plain", b"Not found"),
        };
        // A disconnected browser must not terminate reception/playback.
        if let Err(e) = result {
            eprintln!("viewer HTTP client: {e}");
        }
    }
    Ok(())
}

fn data(stream: &mut TcpStream, result: Result<Vec<u8>, Error>) -> std::io::Result<()> {
    match result {
        Ok(bytes) => respond(stream, "200 OK", "application/octet-stream", &bytes),
        Err(e) => respond(
            stream,
            "500 Internal Server Error",
            "text/plain; charset=utf-8",
            e.to_string().as_bytes(),
        ),
    }
}
