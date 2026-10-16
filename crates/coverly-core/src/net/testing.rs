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

impl MockServer {
    /// Serves `text` at the libretro-database path `repo_path`
    /// (percent-encoded like `DatStore` requests it).
    pub fn serve_dat(&self, repo_path: &str, text: &str) {
        self.mock(
            &format!("/{}", crate::identify::dat::encode_path(repo_path)),
            MockResponse::ok(text.as_bytes().to_vec()),
        );
    }
}

/// An Apache-style autoindex page with the given raw hrefs.
pub fn apache_listing(title: &str, hrefs: &[String]) -> String {
    let mut html = format!(
        "<html><head><title>Index of {title}</title></head><body><h1>Index of {title}</h1><table>\n"
    );
    for h in hrefs {
        html.push_str(&format!("<tr><td><a href=\"{h}\">{h}</a></td></tr>\n"));
    }
    html.push_str("</table></body></html>\n");
    html
}

/// A solid-colour RGBA PNG.
pub fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .expect("encode test png");
    out.into_inner()
}

/// Apache's own href style: spaces as %20, commas and parentheses literal.
fn listing_href(name: &str) -> String {
    name.replace('%', "%25")
        .replace(' ', "%20")
        .replace('"', "%22")
}

impl MockServer {
    /// The thumbnails.libretro.com root listing with `systems` as folders.
    pub fn serve_libretro_root(&self, systems: &[&str]) {
        let mut hrefs = vec!["?C=N;O=D".to_string(), "stats/".to_string()];
        hrefs.extend(systems.iter().map(|s| format!("{}/", listing_href(s))));
        self.mock("/", MockResponse::ok(apache_listing("/", &hrefs)));
    }

    /// The `<system>/<media_dir>/` listing (HTML only), with `ETag: "listing-v1"`.
    pub fn serve_libretro_index(&self, system: &str, media_dir: &str, files: &[&str]) {
        let sys = crate::sources::libretro::encode_segment(system);
        let mut hrefs = vec!["?C=N;O=D".to_string(), format!("/{sys}/")];
        hrefs.extend(files.iter().map(|f| listing_href(f)));
        self.mock(
            &format!("/{sys}/{media_dir}/"),
            MockResponse::ok(apache_listing(&format!("/{system}/{media_dir}"), &hrefs))
                .header("ETag", "\"listing-v1\""),
        );
    }

    /// One image at its thumbnails.libretro.com path.
    pub fn serve_libretro_image(&self, system: &str, media_dir: &str, file: &str, body: Vec<u8>) {
        use crate::sources::libretro::encode_segment;
        self.mock(
            &format!(
                "/{}/{media_dir}/{}",
                encode_segment(system),
                encode_segment(file)
            ),
            MockResponse::ok(body).header("Content-Type", "image/png"),
        );
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

/// A server on 127.0.0.1 that accepts connections and never answers: every
/// request through it times out. Counts the connections it accepted.
pub struct HangingServer {
    addr: SocketAddr,
    accepted: Arc<Mutex<Vec<TcpStream>>>,
    stop: Arc<AtomicBool>,
}

impl HangingServer {
    pub fn start() -> HangingServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind hanging server");
        let addr = listener.local_addr().unwrap();
        let accepted: Arc<Mutex<Vec<TcpStream>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (a, s) = (accepted.clone(), stop.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if s.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(stream) = stream {
                    // Kept open and never written to.
                    a.lock().unwrap().push(stream);
                }
            }
        });
        HangingServer {
            addr,
            accepted,
            stop,
        }
    }
    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }
    /// Connections accepted so far.
    pub fn connections(&self) -> usize {
        self.accepted.lock().unwrap().len()
    }
}

impl Drop for HangingServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr); // unblock accept
    }
}
