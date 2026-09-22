//! A small HTTP server for the engine's integration tests.
//!
//! The engine's behaviour is defined by what real servers do — ranged
//! requests, validators, servers that ignore `Range`, servers that hang up
//! mid-body — so the tests drive it against a socket rather than a mock of the
//! engine's own types.

use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

/// How the server should answer the next request.
#[derive(Debug, Clone)]
pub struct ServerBehaviour {
    pub body: Vec<u8>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub filename: Option<String>,
    /// When false the server answers every request with the whole body and a
    /// `200`, exactly as a server without range support does.
    pub supports_range: bool,
    /// Status to answer with instead of serving the body.
    pub status: Option<(u16, &'static str)>,
    /// Pause between body chunks, used to keep a transfer running long enough
    /// to pause or cancel it.
    pub chunk_delay: Option<Duration>,
    pub chunk_size: usize,
    /// Stop after this many body bytes and close the connection, simulating a
    /// transfer that is cut off.
    pub truncate_after: Option<usize>,
}

impl Default for ServerBehaviour {
    fn default() -> Self {
        Self {
            body: DEFAULT_BODY.to_vec(),
            etag: Some("\"v1\"".to_owned()),
            last_modified: None,
            filename: Some("payload.bin".to_owned()),
            supports_range: true,
            status: None,
            chunk_delay: None,
            chunk_size: 8,
            truncate_after: None,
        }
    }
}

/// The body every test server serves unless a test says otherwise.
pub const DEFAULT_BODY: &[u8] = b"the quick brown fox jumps over the lazy dog";

/// Counters shared with every connection handler, so tests can assert on
/// what the engine actually asked for and how many transfers really overlapped.
#[derive(Debug, Default)]
pub struct ServerStats {
    pub requests: AtomicUsize,
    pub ranged_requests: AtomicUsize,
    pub active_bodies: AtomicUsize,
    pub peak_concurrent_bodies: AtomicUsize,
}

#[derive(Debug)]
pub struct TestServer {
    address: SocketAddr,
    behaviour: Arc<Mutex<ServerBehaviour>>,
    stats: Arc<ServerStats>,
    handle: JoinHandle<()>,
}

impl TestServer {
    pub async fn start(behaviour: ServerBehaviour) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let behaviour = Arc::new(Mutex::new(behaviour));
        let stats = Arc::new(ServerStats::default());

        let handle = {
            let behaviour = Arc::clone(&behaviour);
            let stats = Arc::clone(&stats);

            tokio::spawn(async move {
                loop {
                    let Ok((socket, _)) = listener.accept().await else {
                        return;
                    };

                    let behaviour = Arc::clone(&behaviour);
                    let stats = Arc::clone(&stats);

                    tokio::spawn(async move {
                        let _ = serve(socket, behaviour, stats).await;
                    });
                }
            })
        };

        Self {
            address,
            behaviour,
            stats,
            handle,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}/{}", self.address, path.trim_start_matches('/'))
    }

    pub fn update(&self, change: impl FnOnce(&mut ServerBehaviour)) {
        change(&mut self.behaviour.lock().unwrap());
    }

    pub fn request_count(&self) -> usize {
        self.stats.requests.load(Ordering::SeqCst)
    }

    pub fn ranged_request_count(&self) -> usize {
        self.stats.ranged_requests.load(Ordering::SeqCst)
    }

    /// The largest number of body transfers that were in flight at once.
    pub fn peak_concurrent_bodies(&self) -> usize {
        self.stats.peak_concurrent_bodies.load(Ordering::SeqCst)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn serve(
    mut socket: TcpStream,
    behaviour: Arc<Mutex<ServerBehaviour>>,
    stats: Arc<ServerStats>,
) -> std::io::Result<()> {
    let mut buffer = vec![0_u8; 4096];
    let read = socket.read(&mut buffer).await?;

    if read == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
    let is_head = request.starts_with("HEAD ");
    let range_start = parse_range_start(&request);

    stats.requests.fetch_add(1, Ordering::SeqCst);

    if range_start.is_some() {
        stats.ranged_requests.fetch_add(1, Ordering::SeqCst);
    }

    let behaviour = behaviour.lock().unwrap().clone();

    if let Some((code, reason)) = behaviour.status {
        let response =
            format!("HTTP/1.1 {code} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        socket.write_all(response.as_bytes()).await?;
        return socket.shutdown().await;
    }

    let total = behaviour.body.len();
    let start = range_start
        .filter(|_| behaviour.supports_range)
        .unwrap_or(0);

    if start >= total {
        let response = format!(
            "HTTP/1.1 416 Range Not Satisfiable\r\n\
             Content-Range: bytes */{total}\r\n\
             Content-Length: 0\r\n\
             Connection: close\r\n\r\n"
        );
        socket.write_all(response.as_bytes()).await?;
        return socket.shutdown().await;
    }

    // A single-byte probe asks for `bytes=0-0`; anything else asks for the
    // rest of the file. A server without range support ignores all of that
    // and answers with the whole body.
    let end = if behaviour.supports_range {
        probe_end(&request).unwrap_or(total - 1).min(total - 1)
    } else {
        total - 1
    };
    let slice = &behaviour.body[start..=end];

    let mut headers = String::new();

    if range_start.is_some() && behaviour.supports_range {
        headers.push_str("HTTP/1.1 206 Partial Content\r\n");
        headers.push_str(&format!("Content-Range: bytes {start}-{end}/{total}\r\n"));
    } else {
        headers.push_str("HTTP/1.1 200 OK\r\n");
    }

    headers.push_str(&format!("Content-Length: {}\r\n", slice.len()));
    headers.push_str("Content-Type: application/octet-stream\r\n");

    if behaviour.supports_range {
        headers.push_str("Accept-Ranges: bytes\r\n");
    }

    if let Some(etag) = &behaviour.etag {
        headers.push_str(&format!("ETag: {etag}\r\n"));
    }

    if let Some(last_modified) = &behaviour.last_modified {
        headers.push_str(&format!("Last-Modified: {last_modified}\r\n"));
    }

    if let Some(filename) = &behaviour.filename {
        headers.push_str(&format!(
            "Content-Disposition: attachment; filename=\"{filename}\"\r\n"
        ));
    }

    headers.push_str("Connection: close\r\n\r\n");
    socket.write_all(headers.as_bytes()).await?;

    if is_head {
        return socket.shutdown().await;
    }

    let limit = behaviour
        .truncate_after
        .unwrap_or(slice.len())
        .min(slice.len());

    // Only a real body transfer counts towards concurrency; the single-byte
    // capability probe is not a download.
    let counts_as_body = limit > 1 || slice.len() > 1;

    if counts_as_body {
        let active = stats.active_bodies.fetch_add(1, Ordering::SeqCst) + 1;
        stats
            .peak_concurrent_bodies
            .fetch_max(active, Ordering::SeqCst);
    }

    // The delay goes before each chunk but the first, so a finished body is
    // never still counted as active while the next transfer starts.
    for (index, chunk) in slice[..limit]
        .chunks(behaviour.chunk_size.max(1))
        .enumerate()
    {
        if index > 0
            && let Some(delay) = behaviour.chunk_delay
        {
            tokio::time::sleep(delay).await;
        }

        socket.write_all(chunk).await?;
        socket.flush().await?;
    }

    if counts_as_body {
        stats.active_bodies.fetch_sub(1, Ordering::SeqCst);
    }

    socket.shutdown().await
}

fn parse_range_start(request: &str) -> Option<usize> {
    let line = request
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("range:"))?;

    let value = line.split(':').nth(1)?.trim();
    let range = value.strip_prefix("bytes=")?;

    range.split('-').next()?.trim().parse().ok()
}

/// The end of an explicit `bytes=a-b` range, used by the capability probe.
fn probe_end(request: &str) -> Option<usize> {
    let line = request
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("range:"))?;

    let value = line.split(':').nth(1)?.trim().strip_prefix("bytes=")?;
    let end = value.split('-').nth(1)?.trim();

    if end.is_empty() {
        None
    } else {
        end.parse().ok()
    }
}
