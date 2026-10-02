//! Phase 3+4 — HTTP engine tối giản (§102) cho download engine §103.
//!
//! Zero dependency: HTTP/1.1 qua TcpStream:
//! - `parse_url` — http(s) + host/port/path/query
//! - `get()` — send request, đọc status line + headers, redirect tối đa 10 lần
//! - `get_stream()` — streaming: callback nhận từng chunk (64KB) — không giữ body
//!   nguyên khối trong RAM (artifact có thể > 1GB)
//!
//! Range: caller thêm header `Range: bytes=N-` qua `extra_headers`; server trả 206 →
//! `status = 206` để pipeline quyết định append (parity `_attempt` legacy). Redirect
//! KHÔNG kế thừa Range/auth header (an toàn).
//!
//! ## TLS (phase 4 — F-01)
//!
//! HTTPS qua rustls + native cert store (Schannel/Security Framework/CA系统).
//! Certificate validation bật theo mặc định — không có insecure fallback;
//! handshake fail → error `NET_UNREACHABLE` có ngữ cảnh TLS.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HttpError {
    #[error("unsupported url scheme: {0}")]
    UnsupportedScheme(String),
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("io: {0}")]
    Io(String),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

impl HttpError {
    pub fn code(&self) -> &'static str {
        match self {
            HttpError::UnsupportedScheme(_) | HttpError::InvalidUrl(_) => "CONFIG_INVALID",
            HttpError::TooManyRedirects => "HTTP_TOO_MANY_REDIRECTS",
            HttpError::Io(_) => "NET_UNREACHABLE",
            HttpError::InvalidResponse(_) => "HTTP_PROTOCOL",
        }
    }
}

/// URL prefix scheme — host, port, path/query.
pub fn parse_url(url: &str) -> Result<(String, u16, String), HttpError> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| HttpError::InvalidUrl(url.to_string()))?;
    match scheme {
        "http" | "https" => {}
        other => return Err(HttpError::UnsupportedScheme(other.to_string())),
    }
    let (authority, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>()
                .map_err(|_| HttpError::InvalidUrl(url.to_string()))?,
        ),
        // Default port theo scheme: https không port → 443 (không phải 80).
        None if scheme == "https" => (authority.to_string(), 443),
        None => (authority.to_string(), 80),
    };
    if host.is_empty() {
        return Err(HttpError::InvalidUrl(url.to_string()));
    }
    Ok((host, port, path.to_string()))
}

/// Transport: plain TCP hoặc TLS (rustls) — Read/Write uniform với engine.
enum Connection {
    Plain(TcpStream),
    Tls(StreamOwned<ClientConnection, TcpStream>),
}

impl Read for Connection {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Connection::Plain(s) => s.read(buf),
            Connection::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Connection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Connection::Plain(s) => s.write(buf),
            Connection::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Connection::Plain(s) => s.flush(),
            Connection::Tls(s) => s.flush(),
        }
    }
}

/// Client config với root certs cho sẵn (test inject self-signed cert).
fn build_client_config(roots: RootCertStore) -> Arc<ClientConfig> {
    let mut config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    // Engine chỉ speak HTTP/1.1 — không chào h2.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}

/// Config production: native cert store của OS (Schannel/Security Framework/CA bundle).
fn client_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = RootCertStore::empty();
            let loaded = rustls_native_certs::load_native_certs();
            for cert in &loaded.certs {
                let _ = roots.add(cert.clone());
            }
            if !loaded.errors.is_empty() {
                log::warn!(
                    "native cert store: {} lỗi load ({:?})",
                    loaded.errors.len(),
                    loaded.errors.first().map(|e| e.to_string())
                );
            }
            build_client_config(roots)
        })
        .clone()
}

/// Response thu gọn: status + headers (lowercase keys) + body bytes.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Metadata một response stream (trước khi đọc body).
#[derive(Debug, Clone)]
pub struct StreamHead {
    pub status: u16,
    pub headers: Vec<(String, String)>,
}

impl StreamHead {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }
}

const MAX_REDIRECTS: usize = 10;
const MAX_BODY_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2GB cap (artifact lớn nhất)
const STREAM_CHUNK: usize = 64 * 1024;

/// GET một URL với redirect. `extra_headers` (vd Range) chỉ dùng cho request đầu —
/// redirect KHÔNG kế thừa header (an toàn Range/auth). Body đọc nguyên khối.
pub fn get(
    url: &str,
    extra_headers: &[(String, String)],
    timeout: Duration,
) -> Result<HttpResponse, HttpError> {
    get_with_config(url, extra_headers, timeout, client_config())
}

/// `get` với client config chỉ định — test inject root cert riêng.
fn get_with_config(
    url: &str,
    extra_headers: &[(String, String)],
    timeout: Duration,
    config: Arc<ClientConfig>,
) -> Result<HttpResponse, HttpError> {
    let (head, body) = get_following_redirects(url, extra_headers, timeout, config, |reader, head| {
        read_full_body(reader, head)
    })
    .map_err(|err| match err {
        FollowError::Http(err) => err,
        // get() không có callback user — E = Infallible, nhánh này unreachable.
        FollowError::User(e) => match e {},
    })?;
    Ok(HttpResponse {
        status: head.status,
        headers: head.headers,
        body,
    })
}

/// GET streaming — callback nhận từng chunk. Redirect tự theo dõi; body chunk cuối
/// được deliver về callback với status/headers của response cuối (206 giữ nguyên).
/// Callback trả `Err(E)` → stream dừng + error bubble lên (cancel giữa stream).
pub fn get_stream<E>(
    url: &str,
    extra_headers: &[(String, String)],
    timeout: Duration,
    mut on_chunk: impl FnMut(&[u8]) -> Result<(), E>,
) -> Result<StreamHead, HttpOrUserError<E>> {
    get_following_redirects(
        url,
        extra_headers,
        timeout,
        client_config(),
        |reader, head| stream_body(reader, head, &mut on_chunk),
    )
    .map_err(|err| match err {
        FollowError::Http(err) => HttpOrUserError::Http(err),
        FollowError::User(err) => HttpOrUserError::User(err),
    })
    .map(|(head, ())| head)
}

/// Lỗi từ callback user (cancel giữa stream) tách khỏi lỗi HTTP.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HttpOrUserError<E> {
    #[error("{0}")]
    Http(HttpError),
    #[error("{0}")]
    User(E),
}

enum FollowError<E> {
    Http(HttpError),
    User(E),
}

/// Vòng lặp redirect dùng chung cho get/get_stream. `read_body` quyết định ăn cả
/// (buffer) hay ăn từng miếng (stream).
fn get_following_redirects<T, E>(
    url: &str,
    extra_headers: &[(String, String)],
    timeout: Duration,
    config: Arc<ClientConfig>,
    mut read_body: impl FnMut(&mut BufReader<Connection>, &StreamHead) -> Result<T, FollowError<E>>,
) -> Result<(StreamHead, T), FollowError<E>> {
    let mut current = url.to_string();
    let mut redirects = 0;
    loop {
        // Scheme theo từng hop — redirect có thể đổi http ↔ https (bảo toàn).
        let tls = current.starts_with("https://");
        let (host, port, path) = parse_url(&current).map_err(FollowError::Http)?;
        let headers: &[(String, String)] = if redirects == 0 { extra_headers } else { &[] };
        let (mut reader, head) = send_request(&host, port, &path, tls, headers, timeout, &config)
            .map_err(FollowError::Http)?;
        match head.status {
            301 | 302 | 303 | 307 | 308 => {
                redirects += 1;
                if redirects > MAX_REDIRECTS {
                    return Err(FollowError::Http(HttpError::TooManyRedirects));
                }
                let location = head
                    .header("location")
                    .ok_or_else(|| {
                        FollowError::Http(HttpError::InvalidResponse(
                            "redirect without location".into(),
                        ))
                    })?
                    .to_string();
                // relative → absolute cùng origin + scheme hiện tại
                current = if location.starts_with("http://") || location.starts_with("https://") {
                    location
                } else {
                    let scheme = if tls { "https" } else { "http" };
                    if location.starts_with('/') {
                        format!("{scheme}://{host}:{port}{location}")
                    } else {
                        format!("{scheme}://{host}:{port}/{location}")
                    }
                };
            }
            _ => {
                let body = read_body(&mut reader, &head)?;
                return Ok((head, body));
            }
        }
    }
}

/// Gửi request, trả reader + head (chưa đọc body). `tls=true` → handshake rustls
/// sau khi connect (cert validation theo root store của config).
fn send_request(
    host: &str,
    port: u16,
    path: &str,
    tls: bool,
    extra_headers: &[(String, String)],
    timeout: Duration,
    config: &Arc<ClientConfig>,
) -> Result<(BufReader<Connection>, StreamHead), HttpError> {
    let mut tcp =
        TcpStream::connect((host, port)).map_err(|err| HttpError::Io(err.to_string()))?;
    tcp.set_read_timeout(Some(timeout))
        .map_err(|err| HttpError::Io(err.to_string()))?;
    tcp.set_write_timeout(Some(timeout))
        .map_err(|err| HttpError::Io(err.to_string()))?;

    let mut stream = if tls {
        // ServerName::try_from validates host (DNS name hoặc IP literal).
        let name = ServerName::try_from(host.to_string())
            .map_err(|err| HttpError::InvalidUrl(format!("{host}: {err}")))?;
        let mut conn =
            ClientConnection::new(Arc::clone(config), name).map_err(|err| {
                HttpError::Io(format!("TLS init failed: {err}"))
            })?;
        // Handshake đồng bộ — read/write timeout trên tcp đã set ở trên.
        conn.complete_io(&mut tcp)
            .map_err(|err| HttpError::Io(format!("TLS handshake failed: {err}")))?;
        Connection::Tls(StreamOwned::new(conn, tcp))
    } else {
        Connection::Plain(tcp)
    };

    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: antares-launcher\r\nAccept: */*\r\nConnection: close\r\n"
    );
    for (name, value) in extra_headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|err| HttpError::Io(err.to_string()))?;
    stream.flush().map_err(|err| HttpError::Io(err.to_string()))?;

    let mut reader = BufReader::new(stream);

    // Status line: "HTTP/1.1 200 OK"
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|err| HttpError::Io(err.to_string()))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| HttpError::InvalidResponse(status_line.trim().to_string()))?;

    // Headers
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|err| HttpError::Io(err.to_string()))?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_lowercase(), value.trim().to_string()));
        }
    }
    Ok((reader, StreamHead { status, headers }))
}

fn body_len(head: &StreamHead) -> Option<u64> {
    head.header("content-length")
        .and_then(|v| v.parse::<u64>().ok())
}

fn is_chunked(head: &StreamHead) -> bool {
    head.headers
        .iter()
        .any(|(k, v)| k == "transfer-encoding" && v.to_lowercase().contains("chunked"))
}

/// Buffer toàn bộ body (get) — cap MAX_BODY_BYTES.
fn read_full_body(
    reader: &mut BufReader<Connection>,
    head: &StreamHead,
) -> Result<Vec<u8>, FollowError<core::convert::Infallible>> {
    let body = if is_chunked(head) {
        read_chunked(reader)
    } else if let Some(len) = body_len(head) {
        if len > MAX_BODY_BYTES {
            return Err(FollowError::Http(HttpError::InvalidResponse(format!(
                "body {len} quá lớn"
            ))));
        }
        let mut body = vec![0u8; len as usize];
        reader
            .read_exact(&mut body)
            .map_err(|err| FollowError::Http(HttpError::Io(err.to_string())))?;
        Ok(body)
    } else {
        // Không length, không chunked → đọc tới khi đóng (Connection: close)
        let mut body = Vec::new();
        reader
            .take(MAX_BODY_BYTES)
            .read_to_end(&mut body)
            .map_err(|err| FollowError::Http(HttpError::Io(err.to_string())))?;
        Ok(body)
    };
    body.map_err(FollowError::Http)
}

/// Streaming body qua callback (get_stream) — chunked hoặc content-length hoặc
/// till-close. Callback user trả Err → dừng ngay và bubble `FollowError::User`
/// (cancel giữa stream không bị nuốt thành lỗi HTTP).
fn stream_body<E>(
    reader: &mut BufReader<Connection>,
    head: &StreamHead,
    on_chunk: &mut impl FnMut(&[u8]) -> Result<(), E>,
) -> Result<(), FollowError<E>> {
    let mut deliver = |bytes: &[u8]| -> Result<(), FollowError<E>> {
        on_chunk(bytes).map_err(FollowError::User)
    };
    if is_chunked(head) {
        loop {
            let mut size_line = String::new();
            reader
                .read_line(&mut size_line)
                .map_err(|err| FollowError::Http(HttpError::Io(err.to_string())))?;
            let size_str = size_line.trim().split(';').next().unwrap_or("").trim();
            let size = usize::from_str_radix(size_str, 16).map_err(|_| {
                FollowError::Http(HttpError::InvalidResponse(format!(
                    "bad chunk size {size_str:?}"
                )))
            })?;
            if size == 0 {
                let mut crlf = String::new();
                let _ = reader.read_line(&mut crlf);
                return Ok(());
            }
            let mut chunk = vec![0u8; size];
            reader
                .read_exact(&mut chunk)
                .map_err(|err| FollowError::Http(HttpError::Io(err.to_string())))?;
            deliver(&chunk)?;
            let mut crlf = String::new();
            let _ = reader.read_line(&mut crlf);
        }
    }
    if let Some(len) = body_len(head) {
        let mut remaining = len;
        let mut buf = vec![0u8; STREAM_CHUNK.min(len.max(1) as usize)];
        while remaining > 0 {
            let take = remaining.min(buf.len() as u64) as usize;
            reader
                .read_exact(&mut buf[..take])
                .map_err(|err| FollowError::Http(HttpError::Io(err.to_string())))?;
            deliver(&buf[..take])?;
            remaining -= take as u64;
        }
        return Ok(());
    }
    // Till-close
    let mut buf = vec![0u8; STREAM_CHUNK];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|err| FollowError::Http(HttpError::Io(err.to_string())))?;
        if n == 0 {
            return Ok(());
        }
        deliver(&buf[..n])?;
    }
}

/// Đọc chunked body buffer-mode: `size hex\r\n<size bytes>\r\n` … `0\r\n\r\n`.
fn read_chunked(reader: &mut BufReader<Connection>) -> Result<Vec<u8>, HttpError> {
    let mut body = Vec::new();
    loop {
        let mut size_line = String::new();
        reader
            .read_line(&mut size_line)
            .map_err(|err| HttpError::Io(err.to_string()))?;
        let size_str = size_line.trim().split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16)
            .map_err(|_| HttpError::InvalidResponse(format!("bad chunk size {size_str:?}")))?;
        if size == 0 {
            // trailing \r\n sau chunk cuối
            let mut crlf = String::new();
            let _ = reader.read_line(&mut crlf);
            return Ok(body);
        }
        if body.len() as u64 + size as u64 > MAX_BODY_BYTES {
            return Err(HttpError::InvalidResponse("chunked body quá lớn".into()));
        }
        let mut chunk = vec![0u8; size];
        reader
            .read_exact(&mut chunk)
            .map_err(|err| HttpError::Io(err.to_string()))?;
        body.extend_from_slice(&chunk);
        let mut crlf = String::new();
        let _ = reader.read_line(&mut crlf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use std::net::TcpListener;

    /// Server mini: accept đúng `accept` connection (mỗi connection 1 request),
    /// handler nhận (path, headers-lowercase). Thread kết thúc sau accept connections.
    fn spawn_server(
        accept: usize,
        handler: impl Fn(&str, &[(String, String)]) -> Vec<u8> + Send + 'static,
    ) -> (u16, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            for _ in 0..accept {
                let Ok((stream, _)) = listener.accept() else {
                    break;
                };
                let mut stream = stream;
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    break;
                }
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                let mut headers: Vec<(String, String)> = Vec::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.trim().split_once(':') {
                        headers.push((name.trim().to_lowercase(), value.trim().to_string()));
                    }
                }
                let response = handler(&path, &headers);
                let _ = stream.write_all(&response);
                let _ = stream.flush();
            }
        });
        (port, handle)
    }

    #[test]
    fn parse_url_cases() {
        assert_eq!(
            parse_url("http://example.com/a/b.jar?x=1").unwrap(),
            ("example.com".into(), 80, "/a/b.jar?x=1".into())
        );
        assert_eq!(
            parse_url("http://example.com").unwrap(),
            ("example.com".into(), 80, "/".into())
        );
        assert_eq!(
            parse_url("http://example.com:8080/f.jar").unwrap(),
            ("example.com".into(), 8080, "/f.jar".into())
        );
        assert_eq!(
            parse_url("https://example.com:443/x").unwrap(),
            ("example.com".into(), 443, "/x".into())
        );
        assert_eq!(
            parse_url("ftp://example.com/x").unwrap_err().code(),
            "CONFIG_INVALID"
        );
        assert!(matches!(
            parse_url("example.com/x").unwrap_err(),
            HttpError::InvalidUrl(_)
        ));
    }

    #[test]
    fn parse_url_https_defaults_to_443() {
        assert_eq!(
            parse_url("https://example.com/x").unwrap(),
            ("example.com".into(), 443, "/x".into())
        );
        assert_eq!(
            parse_url("https://example.com:8443/x").unwrap(),
            ("example.com".into(), 8443, "/x".into())
        );
    }

    #[test]
    fn https_connect_refused_is_unreachable() {
        // Không còn seam 127.0.0.1:1 — https đi thẳng TLS; cổng chết → refused.
        let err = get("https://127.0.0.1:1/x", &[], Duration::from_secs(1)).unwrap_err();
        assert_eq!(err.code(), "NET_UNREACHABLE");
    }

    /// TLS server 1-kết nối: nhận cert self-signed cho trước, trả `response`.
    fn spawn_tls_server(
        cert: rustls::pki_types::CertificateDer<'static>,
        key: rustls::pki_types::PrivatePkcs8KeyDer<'static>,
        response: Vec<u8>,
    ) -> (u16, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let Ok((tcp, _)) = listener.accept() else { return };
            let cfg = rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert], key.into())
                .unwrap();
            let conn = rustls::ServerConnection::new(Arc::new(cfg)).unwrap();
            let mut stream = rustls::StreamOwned::new(conn, tcp);
            // Đọc tới hết request headers — handshake tự chạy trong read/write.
            let mut seen = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        seen.extend_from_slice(&buf[..n]);
                        if seen.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => return,
                }
            }
            let _ = stream.write_all(&response);
            let _ = stream.flush();
        });
        (port, handle)
    }

    fn self_signed_localhost() -> (
        rustls::pki_types::CertificateDer<'static>,
        rustls::pki_types::PrivatePkcs8KeyDer<'static>,
    ) {
        let certified =
            rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        (
            certified.cert.der().clone(),
            rustls::pki_types::PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der()),
        )
    }

    fn tls_response(body: &[u8]) -> Vec<u8> {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes()
        .into_iter()
        .chain(body.iter().copied())
        .collect()
    }

    #[test]
    fn https_roundtrip_with_trusted_self_signed_cert() {
        let (cert, key) = self_signed_localhost();
        let (port, server) =
            spawn_tls_server(cert.clone(), key, tls_response(b"hello-tls"));

        // Client tin đúng cert này (inject root) — không dùng native store.
        let mut roots = RootCertStore::empty();
        roots.add(cert).unwrap();
        let config = build_client_config(roots);

        let resp = get_with_config(
            &format!("https://localhost:{port}/x"),
            &[],
            Duration::from_secs(5),
            config,
        )
        .unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"hello-tls");
        server.join().unwrap();
    }

    #[test]
    fn https_rejects_untrusted_certificate() {
        // Cert self-signed không nằm trong root store → handshake bị từ chối
        // (validation BẬT mặc định — không có insecure fallback).
        let (cert, key) = self_signed_localhost();
        let (port, server) =
            spawn_tls_server(cert, key, tls_response(b"should-not-see"));
        let config = build_client_config(RootCertStore::empty());

        let err = get_with_config(
            &format!("https://localhost:{port}/x"),
            &[],
            Duration::from_secs(5),
            config,
        )
        .unwrap_err();
        assert_eq!(err.code(), "NET_UNREACHABLE");
        assert!(
            err.to_string().contains("TLS handshake"),
            "lỗi phải có ngữ cảnh TLS: {err}"
        );
        let _ = server.join();
    }

    #[test]
    fn get_content_length_body_and_range_header_passthrough() {
        let (port, server) = spawn_server(2, |path, headers| {
            let body = b"hello-antares";
            if path.contains("range") {
                let range_seen = headers
                    .iter()
                    .any(|(k, v)| k == "range" && v == "bytes=5-");
                format!(
                    "HTTP/1.1 206 Partial Content\r\nX-Range-Seen: {}\r\nContent-Length: {}\r\n\r\n",
                    range_seen,
                    body.len()
                )
                .into_bytes()
            } else {
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes()
            }
            .into_iter()
            .chain(body.iter().copied())
            .collect()
        });

        // Plain 200 + content-length
        let response =
            get(&format!("http://127.0.0.1:{port}/plain"), &[], Duration::from_secs(5)).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"hello-antares");

        // Range header truyền qua + server thấy (206 path)
        let response = get(
            &format!("http://127.0.0.1:{port}/range"),
            &[("Range".into(), "bytes=5-".into())],
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(response.status, 206);
        assert_eq!(response.header("x-range-seen"), Some("true"));
        assert_eq!(response.body, b"hello-antares");

        server.join().unwrap();
    }

    #[test]
    fn get_chunked_body() {
        let (port, server) = spawn_server(1, |_, _| {
            let body = b"chunked-data";
            let head = format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n",
                body.len()
            );
            let mut out = head.into_bytes();
            out.extend_from_slice(body);
            out.extend_from_slice(b"\r\n0\r\n\r\n");
            out
        });
        let response =
            get(&format!("http://127.0.0.1:{port}/chunked"), &[], Duration::from_secs(5)).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"chunked-data");
        server.join().unwrap();
    }

    #[test]
    fn get_follows_redirect_without_inheriting_range() {
        // Redirect /r → /final; /final trả 400 nếu nhận Range (chứng tỏ header
        // KHÔNG được kế thừa qua redirect). 2 connection: /r rồi /final.
        let (port, server) = spawn_server(2, |path, headers| {
            if path.starts_with("/r") {
                b"HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\n\r\n".to_vec()
            } else {
                let has_range = headers.iter().any(|(k, _)| k == "range");
                if has_range {
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".to_vec()
                } else {
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec()
                }
            }
        });
        let response = get(
            &format!("http://127.0.0.1:{port}/r"),
            &[("Range".into(), "bytes=0-".into())],
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"ok");
        server.join().unwrap();
    }

    #[test]
    fn get_connection_refused_maps_code() {
        // Port gần như chắc chắn không mở: bind 1 port rồi drop listener.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let err =
            get(&format!("http://127.0.0.1:{port}/"), &[], Duration::from_secs(2)).unwrap_err();
        assert_eq!(err.code(), "NET_UNREACHABLE");
    }

    #[test]
    fn stream_content_length_receives_all_chunks_in_order() {
        let (port, server) = spawn_server(1, |_, _| {
            let mut out =
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", PAYLOAD.len()).into_bytes();
            out.extend_from_slice(PAYLOAD);
            out
        });
        let mut received: Vec<Vec<u8>> = Vec::new();
        let head = get_stream(
            &format!("http://127.0.0.1:{port}/s"),
            &[],
            Duration::from_secs(5),
            |chunk| {
                received.push(chunk.to_vec());
                Ok::<(), Infallible>(())
            },
        )
        .unwrap();
        assert_eq!(head.status, 200);
        let joined: Vec<u8> = received.concat();
        assert_eq!(joined, PAYLOAD);
        // chunk nhỏ hơn payload → nhiều chunk (STREAM_CHUNK 64KB > payload → 1 chunk
        // là đủ; kiểm tra ít nhất 1 chunk nhận đủ)
        assert!(!received.is_empty());
        server.join().unwrap();
    }

    #[test]
    fn stream_chunked_body() {
        let (port, server) = spawn_server(1, |_, _| {
            let body = b"stream-chunked-body";
            let head = format!(
                "HTTP/1.1 206 Partial Content\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n",
                body.len()
            );
            let mut out = head.into_bytes();
            out.extend_from_slice(body);
            out.extend_from_slice(b"\r\n0\r\n\r\n");
            out
        });
        let mut total = Vec::new();
        let head = get_stream(
            &format!("http://127.0.0.1:{port}/c"),
            &[],
            Duration::from_secs(5),
            |chunk| {
                total.extend_from_slice(chunk);
                Ok::<(), Infallible>(())
            },
        )
        .unwrap();
        assert_eq!(head.status, 206);
        assert_eq!(total, b"stream-chunked-body");
        server.join().unwrap();
    }

    #[test]
    fn stream_till_close_body() {
        let (port, server) = spawn_server(1, |_, _| {
            // Không Content-Length, không chunked → close sau body
            let mut out = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
            out.extend_from_slice(PAYLOAD);
            out
        });
        let mut total = Vec::new();
        let head = get_stream(
            &format!("http://127.0.0.1:{port}/tc"),
            &[],
            Duration::from_secs(5),
            |chunk| {
                total.extend_from_slice(chunk);
                Ok::<(), Infallible>(())
            },
        )
        .unwrap();
        assert_eq!(head.status, 200);
        assert_eq!(total, PAYLOAD);
        server.join().unwrap();
    }

    #[test]
    fn stream_callback_error_stops_and_bubbles() {
        let (port, server) = spawn_server(1, |_, _| {
            let mut out =
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", PAYLOAD.len()).into_bytes();
            out.extend_from_slice(PAYLOAD);
            out
        });
        let mut calls = 0;
        let err = get_stream(
            &format!("http://127.0.0.1:{port}/x"),
            &[],
            Duration::from_secs(5),
            |chunk| {
                calls += 1;
                let _ = chunk;
                Err::<(), _>("stop".to_string())
            },
        )
        .unwrap_err();
        assert!(matches!(err, HttpOrUserError::User(msg) if msg == "stop"));
        assert_eq!(calls, 1);
        server.join().unwrap();
    }

    #[test]
    fn stream_follows_redirect() {
        // 2 connection: /r (302) rồi /final.
        let (port, server) = spawn_server(2, |path, _| {
            if path.starts_with("/r") {
                b"HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\n\r\n".to_vec()
            } else {
                let mut out = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n".to_vec();
                out.extend_from_slice(b"done");
                out
            }
        });
        let mut total = Vec::new();
        let head = get_stream(
            &format!("http://127.0.0.1:{port}/r"),
            &[],
            Duration::from_secs(5),
            |chunk| {
                total.extend_from_slice(chunk);
                Ok::<(), Infallible>(())
            },
        )
        .unwrap();
        assert_eq!(head.status, 200);
        assert_eq!(total, b"done");
        server.join().unwrap();
    }

    const PAYLOAD: &[u8] = b"antares-stream-payload-0123456789abcdefghijklmnopqrstuvwxyz";
}
