//! Minimal HTTP/1.1 server for hermetic tests: no external network, full
//! control over responses, and fault injection (dropping the connection
//! mid-body) to exercise retry/resume paths.
//!
//! Test-only by design (`publish = false`); not part of the launcher.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// A parsed incoming request (just what the tests need).
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    /// Request body, read using `Content-Length`.
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn body_string(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Parse a `Range: bytes=N-` header into the start offset.
    pub fn range_start(&self) -> Option<u64> {
        self.header("range")?
            .strip_prefix("bytes=")?
            .split('-')
            .next()?
            .parse()
            .ok()
    }
}

/// The response a handler returns.
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// If set, write only this many body bytes (while advertising the full
    /// Content-Length), then drop the connection — simulates a network cut.
    pub drop_after: Option<usize>,
}

impl Response {
    pub fn ok(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body,
            drop_after: None,
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn dropping_after(mut self, bytes: usize) -> Self {
        self.drop_after = Some(bytes);
        self
    }
}

pub type Handler = dyn Fn(&Request) -> Response + Send + Sync;

/// A running test server; shuts down when dropped.
pub struct TestServer {
    addr: SocketAddr,
    handle: tokio::task::JoinHandle<()>,
}

impl TestServer {
    pub async fn start(handler: impl Fn(&Request) -> Response + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let handler: Arc<Handler> = Arc::new(handler);
        let handle = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let handler = Arc::clone(&handler);
                tokio::spawn(async move {
                    let _ = serve_connection(stream, handler).await;
                });
            }
        });
        Self { addr, handle }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn serve_connection(
    mut stream: tokio::net::TcpStream,
    handler: Arc<Handler>,
) -> std::io::Result<()> {
    // Serve sequential requests on one connection (keep-alive).
    loop {
        let Some(request) = read_request(&mut stream).await? else {
            return Ok(()); // client closed
        };
        let response = handler(&request);

        let status_text = match response.status {
            200 => "OK",
            206 => "Partial Content",
            304 => "Not Modified",
            404 => "Not Found",
            416 => "Range Not Satisfiable",
            _ => "Response",
        };
        let mut head = format!("HTTP/1.1 {} {status_text}\r\n", response.status);
        for (name, value) in &response.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str(&format!("Content-Length: {}\r\n\r\n", response.body.len()));
        stream.write_all(head.as_bytes()).await?;

        match response.drop_after {
            Some(n) => {
                stream
                    .write_all(&response.body[..n.min(response.body.len())])
                    .await?;
                stream.flush().await?;
                return Ok(()); // connection cut mid-body
            }
            None => stream.write_all(&response.body).await?,
        }
        stream.flush().await?;
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> std::io::Result<Option<Request>> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte).await? {
            0 => return Ok(None),
            _ => buf.push(byte[0]),
        }
        if buf.len() > 64 * 1024 {
            return Ok(None); // absurd header block; bail
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect();

    // Drain the body. Without this, leftover bytes from a POST would be
    // parsed as the next request line on a keep-alive connection.
    let content_length: usize = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        stream.read_exact(&mut body).await?;
    }

    Ok(Some(Request {
        method,
        path,
        headers,
        body,
    }))
}
