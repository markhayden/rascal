//! The fake fillers (SPEC.md "Acceptance", end to end offline): one implementation with five
//! behaviours, reachable as a command (`fake-filler <behaviour> <image> [<mask>] <out>`) and
//! as a local HTTP server, so the real command and HTTP fillers are exercised without a model.
//!
//! - `exact`: a deterministic box blur of the image where the mask is set (everywhere without
//!   a mask); the answer is the request's size.
//! - `mis-sized`: the answer is one pixel wider than the request.
//! - `re-encoding`: every channel of every pixel is changed by one level.
//! - `slow`: sleeps three seconds, then answers as `exact`.
//! - `failing`: exits 2 (HTTP 500) with a message.
//! - `non-png`: answers with text where a PNG was expected (HTTP only).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use image::{DynamicImage, RgbImage};

/// A behaviour by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    Exact,
    MisSized,
    Reencoding,
    Slow,
    Failing,
    NonPng,
}

impl Behaviour {
    pub const ALL: [Behaviour; 6] = [
        Behaviour::Exact,
        Behaviour::MisSized,
        Behaviour::Reencoding,
        Behaviour::Slow,
        Behaviour::Failing,
        Behaviour::NonPng,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Behaviour::Exact => "exact",
            Behaviour::MisSized => "mis-sized",
            Behaviour::Reencoding => "re-encoding",
            Behaviour::Slow => "slow",
            Behaviour::Failing => "failing",
            Behaviour::NonPng => "non-png",
        }
    }

    pub fn parse(name: &str) -> Option<Behaviour> {
        Behaviour::ALL.into_iter().find(|b| b.name() == name)
    }
}

/// How long `slow` sleeps.
pub const SLOW: Duration = Duration::from_secs(3);

/// What a fake answers with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Png(Vec<u8>),
    Text(String),
    Failure(String),
}

/// The answer for a behaviour, given the request's PNG bytes.
pub fn answer(behaviour: Behaviour, image_png: &[u8], mask_png: Option<&[u8]>) -> Answer {
    let decode = |bytes: &[u8]| image::load_from_memory(bytes).map(|i| i.to_rgb8());
    let image = match decode(image_png) {
        Ok(i) => i,
        Err(e) => return Answer::Failure(format!("fake filler: the image is not a PNG: {e}")),
    };
    let mask = match mask_png.map(|m| image::load_from_memory(m).map(|i| i.to_luma8())) {
        Some(Ok(m)) => Some(m),
        Some(Err(e)) => return Answer::Failure(format!("fake filler: the mask is not a PNG: {e}")),
        None => None,
    };
    let encode = |rgb: RgbImage| {
        let mut bytes = Vec::new();
        DynamicImage::ImageRgb8(rgb)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("encoding an in-memory PNG");
        Answer::Png(bytes)
    };
    match behaviour {
        Behaviour::Exact | Behaviour::Slow => {
            if behaviour == Behaviour::Slow {
                std::thread::sleep(SLOW);
            }
            encode(blur_where(&image, mask.as_ref(), 3))
        }
        Behaviour::MisSized => {
            let (w, h) = image.dimensions();
            let mut wider = RgbImage::new(w + 1, h);
            for (x, y, p) in image.enumerate_pixels() {
                wider.put_pixel(x, y, *p);
            }
            encode(wider)
        }
        Behaviour::Reencoding => {
            let mut changed = image;
            for p in changed.pixels_mut() {
                for ch in 0..3 {
                    p[ch] = p[ch].wrapping_add(1);
                }
            }
            encode(changed)
        }
        Behaviour::Failing => Answer::Failure("fake filler: failing as asked".to_string()),
        Behaviour::NonPng => Answer::Text("fake filler: this is not a PNG".to_string()),
    }
}

/// A box blur of radius `r` (edge replication), applied where the mask is nonzero; the
/// image unchanged elsewhere. Without a mask, everywhere.
pub fn blur_where(image: &RgbImage, mask: Option<&image::GrayImage>, r: i64) -> RgbImage {
    let (w, h) = image.dimensions();
    let at = |x: i64, y: i64| {
        image.get_pixel(
            x.clamp(0, i64::from(w) - 1) as u32,
            y.clamp(0, i64::from(h) - 1) as u32,
        )
    };
    RgbImage::from_fn(w, h, |x, y| {
        if mask.is_some_and(|m| m.get_pixel(x, y)[0] == 0) {
            return *image.get_pixel(x, y);
        }
        let mut sum = [0u32; 3];
        let mut n = 0u32;
        for dy in -r..=r {
            for dx in -r..=r {
                let p = at(i64::from(x) + dx, i64::from(y) + dy);
                for ch in 0..3 {
                    sum[ch] += u32::from(p[ch]);
                }
                n += 1;
            }
        }
        image::Rgb([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8])
    })
}

/// The command form: reads the files, writes `out` on success, returns the exit code; a
/// failure's message goes to stderr.
pub fn run_command(
    behaviour: Behaviour,
    image: &std::path::Path,
    mask: Option<&std::path::Path>,
    out: &std::path::Path,
) -> i32 {
    let image_png = match std::fs::read(image) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("fake filler: {}: {e}", image.display());
            return 3;
        }
    };
    let mask_png = match mask.map(std::fs::read) {
        Some(Ok(b)) => Some(b),
        Some(Err(e)) => {
            eprintln!("fake filler: mask: {e}");
            return 3;
        }
        None => None,
    };
    match answer(behaviour, &image_png, mask_png.as_deref()) {
        Answer::Png(bytes) => match std::fs::write(out, bytes) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("fake filler: {}: {e}", out.display());
                3
            }
        },
        Answer::Text(text) => {
            let _ = std::fs::write(out, text);
            0
        }
        Answer::Failure(message) => {
            eprintln!("{message}");
            2
        }
    }
}

/// What the HTTP fake saw in one request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    pub has_image: bool,
    pub has_mask: bool,
    pub prompt: Option<String>,
    pub authorization: Option<String>,
    pub image_bytes: usize,
}

/// Serves the fake on `listener` until the thread is dropped with the process: one request at
/// a time, a multipart POST in, the behaviour's answer out. With `expected_key`, a request
/// without `Authorization: Bearer <key>` is answered 401. Every request is pushed to `seen`.
pub fn serve(
    listener: TcpListener,
    behaviour: Behaviour,
    expected_key: Option<String>,
    seen: Arc<Mutex<Vec<Seen>>>,
) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let Some((headers, body)) = read_request(&mut stream) else {
            continue;
        };
        let header = |name: &str| -> Option<String> {
            headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        };
        let authorization = header("authorization");
        let mut record = Seen {
            authorization: authorization.clone(),
            ..Seen::default()
        };
        let parts = header("content-type")
            .and_then(|ct| boundary(&ct))
            .map(|b| multipart(&body, &b))
            .unwrap_or_default();
        let part = |name: &str| {
            parts
                .iter()
                .find(|p| p.name == name)
                .map(|p| p.body.clone())
        };
        let image = part("image");
        let mask = part("mask");
        record.has_image = image.is_some();
        record.has_mask = mask.is_some();
        record.image_bytes = image.as_ref().map_or(0, Vec::len);
        record.prompt = part("prompt").map(|p| String::from_utf8_lossy(&p).into_owned());
        seen.lock().unwrap_or_else(|e| e.into_inner()).push(record);
        if let Some(key) = &expected_key
            && authorization.as_deref() != Some(&format!("Bearer {key}"))
        {
            respond(
                &mut stream,
                401,
                "text/plain",
                b"missing or wrong bearer key",
            );
            continue;
        }
        let Some(image) = image else {
            respond(&mut stream, 400, "text/plain", b"no image part");
            continue;
        };
        match answer(behaviour, &image, mask.as_deref()) {
            Answer::Png(bytes) => respond(&mut stream, 200, "image/png", &bytes),
            Answer::Text(text) => respond(&mut stream, 200, "text/plain", text.as_bytes()),
            Answer::Failure(message) => respond(&mut stream, 500, "text/plain", message.as_bytes()),
        }
    }
}

/// Binds a loopback port, serves on a thread, and returns the URL and the seen log.
pub fn spawn(
    behaviour: Behaviour,
    expected_key: Option<String>,
) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let url = format!("http://{}/fill", listener.local_addr().expect("local addr"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || serve(listener, behaviour, expected_key, log));
    (url, seen)
}

/// Request headers as `(name, value)` pairs.
type Headers = Vec<(String, String)>;

fn read_request(stream: &mut std::net::TcpStream) -> Option<(Headers, Vec<u8>)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        if buf.len() > 64 * 1024 {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let headers: Headers = head
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[head_end..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Some((headers, body))
}

fn boundary(content_type: &str) -> Option<String> {
    content_type
        .split(';')
        .map(str::trim)
        .find_map(|p| p.strip_prefix("boundary="))
        .map(|b| b.trim_matches('"').to_string())
}

struct Part {
    name: String,
    body: Vec<u8>,
}

fn multipart(body: &[u8], boundary: &str) -> Vec<Part> {
    let delimiter = format!("--{boundary}");
    let mut parts = Vec::new();
    let mut rest = body;
    // Skip the preamble up to the first delimiter.
    let Some(first) = find(rest, delimiter.as_bytes()) else {
        return parts;
    };
    rest = &rest[first + delimiter.len()..];
    loop {
        if rest.starts_with(b"--") {
            break;
        }
        rest = rest.strip_prefix(b"\r\n").unwrap_or(rest);
        let Some(head_end) = find(rest, b"\r\n\r\n") else {
            break;
        };
        let head = String::from_utf8_lossy(&rest[..head_end]).into_owned();
        let name = head
            .split(';')
            .map(str::trim)
            .find_map(|p| p.strip_prefix("name="))
            .map(|n| n.trim_matches('"').to_string())
            .unwrap_or_default();
        let content = &rest[head_end + 4..];
        let Some(next) = find(content, delimiter.as_bytes()) else {
            break;
        };
        // The body ends before the CRLF that precedes the delimiter.
        let end = next.saturating_sub(2);
        parts.push(Part {
            name,
            body: content[..end].to_vec(),
        });
        rest = &content[next + delimiter.len()..];
    }
    parts
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn respond(stream: &mut std::net::TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        500 => "Internal Server Error",
        _ => "",
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
    let _ = stream.shutdown(std::net::Shutdown::Write);
}
