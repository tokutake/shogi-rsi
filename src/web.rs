//! Local browser interface. Positions are reconstructed and validated per request.
use crate::game::state;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

const PAGE: &str = include_str!("../web/index.html");

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &str) -> io::Result<()> {
    respond_bytes(stream, status, kind, body.as_bytes())
}

fn respond_bytes(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> io::Result<()> {
    write!(stream, "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n", status, kind, body.len())?;
    stream.write_all(body)
}

// Accept the interface reached by this connection, never arbitrary Host names.
// This also keeps the Host/origin checks effective when listening on the LAN.
fn valid_host(host: &str, local: SocketAddr) -> bool {
    host == format!("localhost:{}", local.port())
        || host == format!("127.0.0.1:{}", local.port())
        || host.parse::<SocketAddr>().ok() == Some(local)
}

fn handle(mut stream: TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut data = Vec::new();
    let mut buf = [0; 2048];
    let header_end = loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if data.len() > 8192 {
            return respond(
                &mut stream,
                "413 Payload Too Large",
                "text/plain",
                "Request too large",
            );
        }
    };
    let header = String::from_utf8_lossy(&data[..header_end]);
    let headers: Vec<_> = header
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .collect();
    let host = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.trim())
        .unwrap_or("");
    if !valid_host(host, stream.local_addr()?) {
        return respond(&mut stream, "403 Forbidden", "text/plain", "Invalid host");
    }
    if let Some((_, origin)) = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("origin"))
    {
        if origin.trim() != format!("http://{}", host) {
            return respond(
                &mut stream,
                "403 Forbidden",
                "text/plain",
                "Cross-origin request rejected",
            );
        }
    }
    let route = header
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let length = match headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
    {
        Some((_, value)) => match value.trim().parse::<usize>() {
            Ok(length) => length,
            Err(_) => {
                return respond(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain",
                    "Invalid content length",
                )
            }
        },
        None => 0,
    };
    if length > 8192 {
        return respond(
            &mut stream,
            "413 Payload Too Large",
            "text/plain",
            "Request too large",
        );
    }
    match route.as_str() {
        "GET /" | "GET /index.html" => {
            respond(&mut stream, "200 OK", "text/html; charset=utf-8", PAGE)
        }
        "GET /engine-worker.js" => respond(
            &mut stream,
            "200 OK",
            "text/javascript",
            include_str!("../web/engine-worker.js"),
        ),
        "GET /sw.js" => respond(
            &mut stream,
            "200 OK",
            "text/javascript",
            include_str!("../web/sw.js"),
        ),
        "GET /manifest.webmanifest" => respond(
            &mut stream,
            "200 OK",
            "application/manifest+json",
            include_str!("../web/manifest.webmanifest"),
        ),
        "GET /icon.svg" => respond(
            &mut stream,
            "200 OK",
            "image/svg+xml",
            include_str!("../web/icon.svg"),
        ),
        "GET /apple-touch-icon.png" => respond_bytes(
            &mut stream,
            "200 OK",
            "image/png",
            include_bytes!("../web/apple-touch-icon.png"),
        ),
        "GET /shogi_rsi.wasm" => {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dist/shogi_rsi.wasm");
            match std::fs::read(path) {
                Ok(bytes) => respond_bytes(&mut stream, "200 OK", "application/wasm", &bytes),
                Err(_) => respond(
                    &mut stream,
                    "404 Not Found",
                    "text/plain",
                    "Run sh scripts/build-web.sh to enable the browser engine",
                ),
            }
        }
        "POST /api/game" => {
            while data.len() < header_end + length {
                let n = stream.read(&mut buf)?;
                if n == 0 {
                    return respond(
                        &mut stream,
                        "400 Bad Request",
                        "text/plain",
                        "Incomplete request",
                    );
                }
                data.extend_from_slice(&buf[..n]);
            }
            match std::str::from_utf8(&data[header_end..header_end + length])
                .ok()
                .and_then(|body| state(body).ok())
            {
                Some(json) => respond(
                    &mut stream,
                    "200 OK",
                    "application/json; charset=utf-8",
                    &json,
                ),
                None => respond(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain",
                    "Invalid game request",
                ),
            }
        }
        _ => respond(&mut stream, "404 Not Found", "text/plain", "Not found"),
    }
}

pub fn serve(port: u16, lan: bool) -> io::Result<()> {
    let listener = TcpListener::bind((if lan { "0.0.0.0" } else { "127.0.0.1" }, port))?;
    let port = listener.local_addr()?.port();
    println!(
        "ブラウザで http://127.0.0.1:{}/ を開いてください（終了: Ctrl+C）",
        port
    );
    if lan {
        println!(
            "iPhone/iPad: 同じWi-Fiに接続し、Safariで http://<このPCのLAN IP>:{} を開いてください",
            port
        );
        println!("LAN内の端末から接続できます。終了: Ctrl+C");
    }
    // Sequential requests keep concurrent AI searches from exhausting memory/CPU.
    for stream in listener.incoming() {
        if let Err(e) = stream.and_then(handle) {
            eprintln!("Web request: {}", e);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_lan_host() {
        let local = "192.168.1.20:8080".parse().unwrap();
        assert!(valid_host("192.168.1.20:8080", local));
        assert!(valid_host("localhost:8080", local));
        assert!(valid_host("127.0.0.1:8080", local));
        assert!(!valid_host("192.168.1.21:8080", local));
        assert!(!valid_host("192.168.1.20:8081", local));
        assert!(!valid_host("example.com:8080", local));
        assert!(!valid_host("192.168.1.20:8080@example.com", local));
    }

    #[test]
    fn validates_moves_and_replies() {
        assert!(state("0 100 state 7g7f").unwrap().contains("\"side\":1"));
        assert!(state("0 100 play 7g7f").unwrap().contains("\"side\":0"));
        assert!(state("1 100 play").unwrap().contains("\"side\":1"));
        assert!(state("0 100 state 7g7e").is_err());
        assert!(state("0 100 state 7g7fgarbage").is_err());
        assert!(state("0 100 state 7g7f\"quoted").is_err());
        assert!(state("0 999999 play").is_err());
    }

    #[test]
    fn accepts_promotion_capture_and_drop() {
        assert!(state("0 100 state 7g7f 3c3d 8h2b+ 3a2b B*5e").is_ok());
        assert!(state("0 100 state P*5e").is_err());
    }

    #[test]
    fn stops_at_repetition() {
        let cycle = "5i5h 5a5b 5h5i 5b5a ";
        let body = format!("0 100 play {}", cycle.repeat(3));
        assert!(state(&body).unwrap().contains("千日手で引き分け"));
        assert!(state(&(body + "5i5h")).is_err());
    }
}
