//! `#[cfg(test)]` — server HTTP tối giản map path → body, chung cho test
//! install/Mojang runtime (copy semantics của `serve()` trong install tests:
//! 1 request/connection, `Connection: close`, 404 khi path vắng).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

type Routes = Vec<(String, Vec<u8>)>;

/// Serve route cần biết `base_url` TRƯỚC khi build (body chứa URL absolute
/// của chính server — vd manifest nhúng object URL). Route cố định:
/// `serve_dynamic(move |_| routes)`.
pub(crate) fn serve_dynamic(
    build: impl FnOnce(&str) -> Routes + Send + 'static,
) -> (String, std::thread::JoinHandle<()>) {
    serve_inner(build)
}

fn serve_inner(
    build: impl FnOnce(&str) -> Routes + Send + 'static,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let routes = build(&base);
    let handle = std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = handle_connection(stream, &routes);
        }
    });
    (base, handle)
}

fn handle_connection(mut stream: TcpStream, routes: &Routes) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
    }
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let body = routes
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, b)| b.clone())
        .unwrap_or_default();
    let head = if body.is_empty() {
        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
    };
    stream.write_all(head.as_bytes())?;
    if !body.is_empty() {
        stream.write_all(&body)?;
    }
    stream.flush()
}
