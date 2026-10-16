//! A tiny HTTP/1.1 server on 127.0.0.1 for tests (feature `test-util`):
//! canned responses per request path (the raw, percent-encoded target),
//! `Connection: close`, request log. Unknown paths answer 404.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct MockResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl MockResponse {
    pub fn ok(body: impl Into<Vec<u8>>) -> MockResponse {
        MockResponse {
            status: 200,
            headers: Vec::new(),
            body: body.into(),
        }
    }
    pub fn status(code: u16) -> MockResponse {
        MockResponse {
            status: code,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }
    pub fn header(mut self, k: &str, v: &str) -> MockResponse {
        self.headers.push((k.into(), v.into()));
        self
    }
}

#[derive(Clone, Debug)]
pub struct RecordedRequest {
    pub path: String,
    pub headers: Vec<(String, String)>,
}

impl RecordedRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

type Routes = Arc<Mutex<HashMap<String, VecDeque<MockResponse>>>>;

pub struct MockServer {
    addr: SocketAddr,
    routes: Routes,
    log: Arc<Mutex<Vec<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
}

impl MockServer {
    pub fn start() -> MockServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        let routes: Routes = Arc::default();
        let log: Arc<Mutex<Vec<RecordedRequest>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (r, l, s) = (routes.clone(), log.clone(), stop.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if s.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let (r, l) = (r.clone(), l.clone());
                std::thread::spawn(move || {
                    let _ = serve(stream, &r, &l);
                });
            }
        });
        MockServer {
            addr,
            routes,
            log,
            stop,
        }
    }
    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base(), path)
    }
    pub fn mock(&self, path: &str, resp: MockResponse) {
        self.mock_seq(path, vec![resp]);
    }
    pub fn mock_seq(&self, path: &str, seq: Vec<MockResponse>) {
        self.routes
            .lock()
            .unwrap()
            .insert(path.to_string(), seq.into());
    }
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.log.lock().unwrap().clone()
    }
    pub fn hits(&self, path: &str) -> usize {
        self.requests().iter().filter(|r| r.path == path).count()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr); // unblock accept
    }
}

fn serve(
    stream: TcpStream,
    routes: &Routes,
    log: &Arc<Mutex<Vec<RecordedRequest>>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" || headers.len() > 100 {
            break;
        }
        if let Some((k, v)) = h.trim_end().split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    log.lock().unwrap().push(RecordedRequest {
        path: path.clone(),
        headers,
    });
    let resp = {
        let mut map = routes.lock().unwrap();
        match map.get_mut(&path) {
            Some(q) if q.len() > 1 => q.pop_front().unwrap(),
            Some(q) => q
                .front()
                .cloned()
                .unwrap_or_else(|| MockResponse::status(404)),
            None => MockResponse::status(404),
        }
    };
    let mut out = stream;
    write!(
        out,
        "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
        resp.status,
        resp.body.len()
    )?;
    for (k, v) in &resp.headers {
        write!(out, "{k}: {v}\r\n")?;
    }
    out.write_all(b"\r\n")?;
    out.write_all(&resp.body)?;
    out.flush()
}
