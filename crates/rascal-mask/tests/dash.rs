//! Task 1.10: the run page and tracing. On a workspace holding a repeated step, a
//! `not_applicable` step and a failed step, the page names every journal line and no other,
//! before and after `workspace repair`; every artifact the journal names is embedded as a
//! `data:` URI that decodes; `RASCAL_LOG` prints one span per op and never a pixel; `--serve`
//! binds loopback only.
#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn rascal(args: &[&str]) -> Output {
    rascal_env(args, &[])
}

fn rascal_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rascal"));
    command.args(args).env_remove("RASCAL_LOG");
    for (k, v) in env {
        command.env(k, v);
    }
    command.output().unwrap()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rascal-dash-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn journal(dir: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join("journal.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// One workspace, four runs: `rect` twice (every step repeated), the outside box
/// (`not_applicable` plus a note) and the bad box (a failed invocation).
fn busy_workspace(dir: &Path) -> PathBuf {
    let ws = dir.join("ws");
    let floor = repo_root().join("fixtures/images/floor.png");
    let outside = repo_root().join("fixtures/recipes/outside-box.toml");
    let bad = repo_root().join("fixtures/recipes/bad-box.toml");
    for (name, recipe, bbox, code) in [
        ("a", "rect", Some("0.2,0.2,0.6,0.6"), 0),
        ("b", "rect", Some("0.2,0.2,0.6,0.6"), 0),
        ("c", s(&outside), None, 0),
        ("d", s(&bad), None, 64),
    ] {
        let out = dir.join(name);
        let mut args = vec![
            "run",
            recipe,
            s(&floor),
            "--out",
            s(&out),
            "--workspace",
            s(&ws),
        ];
        if let Some(bbox) = bbox {
            args.extend(["--bbox", bbox]);
        }
        let out = rascal(&args);
        assert_eq!(
            out.status.code(),
            Some(code),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    ws
}

fn dash(ws: &Path, page: &Path) -> String {
    let out = rascal(&["dash", s(ws), "--out", s(page)]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(page).unwrap()
}

/// Every `<tag ... attr="value"` in the page, as values in document order.
fn attr_values(html: &str, tag: &str, attr: &str) -> Vec<String> {
    let open = format!("<{tag} ");
    let mut values = Vec::new();
    for (i, _) in html.match_indices(&open) {
        let end = html[i..].find('>').unwrap() + i;
        let element = &html[i..end];
        let key = format!(" {attr}=\"");
        if let Some(a) = element.find(&key) {
            let rest = &element[a + key.len()..];
            values.push(rest[..rest.find('"').unwrap()].to_string());
        }
    }
    values
}

/// The page's asset table: every artifact once, as a `data:` URI by key (an identity, or
/// `<identity>/<plane>` for a mask).
fn asset_table(html: &str) -> BTreeMap<String, String> {
    let open = "<script id=\"assets\" type=\"application/json\">";
    let at = html.find(open).expect("the asset table") + open.len();
    let end = html[at..].find("</script>").unwrap() + at;
    serde_json::from_str(&html[at..end].replace("<\\/", "</")).unwrap()
}

/// The asset keys of every `<figure data-identity="<identity>">`.
fn figure_keys(html: &str, identity: &str) -> Vec<String> {
    let open = format!("<figure data-identity=\"{identity}\" data-asset=\"");
    html.match_indices(&open)
        .map(|(i, _)| {
            let rest = &html[i + open.len()..];
            rest[..rest.find('"').unwrap()].to_string()
        })
        .collect()
}

fn base64_decode(text: &str) -> Vec<u8> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes().filter(|c| *c != b'=') {
        let v = TABLE.iter().position(|t| *t == c).unwrap() as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// Decodes a `data:<mime>;base64,<payload>` URI to its mime type and bytes.
fn decode_data_uri(uri: &str) -> (String, Vec<u8>) {
    let rest = uri.strip_prefix("data:").unwrap();
    let (mime, payload) = rest.split_once(";base64,").unwrap();
    (mime.to_string(), base64_decode(payload))
}

fn assert_page_matches_journal(html: &str, lines: &[serde_json::Value]) {
    let seqs: BTreeSet<String> = lines.iter().map(|l| l["seq"].to_string()).collect();
    assert_eq!(seqs.len(), lines.len(), "journal seqs are unique");
    for tag in ["tr", "section"] {
        let named = attr_values(html, tag, "data-seq");
        assert_eq!(
            named.len(),
            lines.len(),
            "{tag}: one element per journal line"
        );
        assert_eq!(
            named.into_iter().collect::<BTreeSet<_>>(),
            seqs,
            "{tag}: every journal line and no other"
        );
    }
    let statuses = attr_values(html, "tr", "data-status");
    let count = |status: &str| statuses.iter().filter(|s| *s == status).count();
    let journal_count = |status: &str| {
        lines
            .iter()
            .filter(|l| l["envelope"]["status"] == status)
            .count()
    };
    assert_eq!(count("not_applicable"), journal_count("not_applicable"));
    assert_eq!(count("failed"), journal_count("invalid_args"));
    assert_eq!(count("ok"), journal_count("ok"));
    assert_eq!(
        count("note"),
        lines.iter().filter(|l| l["event"] == "note").count()
    );
    // Repeats: the nth line of a step is marked n.
    let steps = attr_values(html, "tr", "data-step");
    let repeats = attr_values(html, "tr", "data-repeat");
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for (step, repeat) in steps.iter().zip(&repeats) {
        let n = seen.entry(step.as_str()).or_insert(0);
        *n += 1;
        assert_eq!(repeat, &n.to_string(), "step {step}");
    }
}

fn assert_every_artifact_embedded(html: &str, lines: &[serde_json::Value]) {
    let floor = image::open(repo_root().join("fixtures/images/floor.png")).unwrap();
    let table = asset_table(html);
    let mut checked = BTreeSet::new();
    for line in lines {
        let Some(outputs) = line["outputs"].as_object() else {
            continue;
        };
        for (port, identity) in outputs {
            let identity = identity.as_str().unwrap();
            let keys = figure_keys(html, identity);
            assert!(!keys.is_empty(), "{port} {identity} is not on the page");
            for key in keys {
                assert!(
                    key == identity || key.starts_with(&format!("{identity}/")),
                    "{key}"
                );
                let uri = table
                    .get(&key)
                    .unwrap_or_else(|| panic!("{key} is not in the asset table"));
                let (mime, bytes) = decode_data_uri(uri);
                match mime.as_str() {
                    "image/png" => {
                        let decoded = image::load_from_memory(&bytes).unwrap();
                        assert_eq!(
                            (decoded.width(), decoded.height()),
                            (floor.width(), floor.height()),
                            "{port}: an artifact in the raw frame has the image's size"
                        );
                    }
                    "application/json" => {
                        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                        assert!(value.is_object(), "{port}: {value}");
                    }
                    other => panic!("{port}: unexpected media type {other}"),
                }
                checked.insert(key);
            }
        }
    }
    // The image, the location, the mask's three planes and the rendering: distinct assets.
    assert!(
        checked.len() >= 6,
        "decoded {} distinct assets",
        checked.len()
    );
    // Each embedded once: the page holds exactly one PNG data URI per PNG asset, however many
    // journal lines name the artifact (the two rect runs share theirs).
    let png_assets = table
        .values()
        .filter(|u| u.starts_with("data:image/png"))
        .count();
    assert_eq!(html.matches("data:image/png;base64,").count(), png_assets);
}

#[test]
fn the_page_names_every_journal_line_and_embeds_every_artifact_before_and_after_repair() {
    let dir = scratch("page");
    let ws = busy_workspace(&dir);
    let lines = journal(&ws);
    let statuses: Vec<_> = lines
        .iter()
        .map(|l| {
            l["envelope"]["status"]
                .as_str()
                .unwrap_or("note")
                .to_string()
        })
        .collect();
    assert!(
        statuses.contains(&"not_applicable".to_string()),
        "{statuses:?}"
    );
    assert!(
        statuses.contains(&"invalid_args".to_string()),
        "{statuses:?}"
    );
    assert!(statuses.contains(&"note".to_string()), "{statuses:?}");

    let html = dash(&ws, &dir.join("page.html"));
    assert_page_matches_journal(&html, &lines);
    assert_every_artifact_embedded(&html, &lines);
    assert!(!html.contains("journal rebuilt by repair"));
    assert!(
        html.contains("data-calls=\"0\""),
        "the filler and pack rows are present and empty"
    );
    // The page is offline: no external references.
    assert!(
        !html.contains("http://") && !html.contains("https://"),
        "{}",
        &html[..400]
    );

    // After repair the journal is rebuilt from the records: only committed invocations remain,
    // and the page says so.
    let out = rascal(&["workspace", "repair", s(&ws)]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rebuilt = journal(&ws);
    assert_ne!(rebuilt.len(), lines.len());
    assert!(rebuilt[0]["note"].as_str().unwrap().contains("rebuilt"));
    let html = dash(&ws, &dir.join("page.html"));
    assert!(html.contains("journal rebuilt by repair"));
    assert_page_matches_journal(&html, &rebuilt);
    assert_every_artifact_embedded(&html, &rebuilt);

    // --out overwrites a file; neither flag is a usage error.
    let out = rascal(&["dash", s(&ws)]);
    assert_eq!(out.status.code(), Some(64));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rascal_log_prints_one_span_per_op_and_no_pixel_bytes() {
    let dir = scratch("log");
    let floor = repo_root().join("fixtures/images/floor.png");
    let run = dir.join("run");
    let args = [
        "run",
        "rect",
        s(&floor),
        "--bbox",
        "0.2,0.2,0.6,0.6",
        "--out",
        s(&run),
    ];
    let quiet = rascal(&args);
    assert_eq!(quiet.status.code(), Some(0));
    assert!(
        quiet.stderr.is_empty(),
        "off by default: {}",
        String::from_utf8_lossy(&quiet.stderr)
    );

    let run2 = dir.join("run2");
    let mut args = args;
    args[6] = s(&run2);
    let loud = rascal_env(&args, &[("RASCAL_LOG", "trace")]);
    assert_eq!(loud.status.code(), Some(0));
    let err = std::str::from_utf8(&loud.stderr).expect("tracing output is text");
    for op in ["rasterize", "render"] {
        assert!(
            err.contains(&format!("op{{op={op}")),
            "a span per op: {err}"
        );
    }
    assert!(err.contains("op finished"), "{err}");
    assert!(err.contains("sha256:"), "identities are in the span: {err}");
    let png_signature = b"\x89PNG";
    assert!(!loud.stderr.windows(4).any(|w| w == png_signature));
    for line in err.lines() {
        assert!(line.len() < 2048, "a line of pixel bytes: {}", &line[..80]);
        assert!(
            !line.contains("pixels") && !line.contains("base64"),
            "pixels in a span: {line}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serve_binds_loopback_only_and_rebuilds_on_each_request() {
    let dir = scratch("serve");
    let ws = busy_workspace(&dir);
    let mut child = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .args(["dash", s(&ws), "--serve"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut url = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut url)
        .unwrap();
    let url = url.trim();
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    let port: u16 = url
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches('/')
        .parse()
        .unwrap();
    let get = |path: &str| {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    };
    let first = get("/");
    assert!(first.starts_with("HTTP/1.1 200 OK"), "{}", &first[..100]);
    let lines = journal(&ws);
    let body = first.split("\r\n\r\n").nth(1).unwrap();
    assert_page_matches_journal(body, &lines);

    // A second request sees a line appended since the first.
    let floor = repo_root().join("fixtures/images/floor.png");
    let out = rascal(&[
        "run",
        "rect",
        s(&floor),
        "--bbox",
        "0.1,0.1,0.3,0.3",
        "--out",
        s(&dir.join("e")),
        "--workspace",
        s(&ws),
    ]);
    assert_eq!(out.status.code(), Some(0));
    let second = get("/");
    let body = second.split("\r\n\r\n").nth(1).unwrap();
    assert_page_matches_journal(body, &journal(&ws));

    // Only loopback. On Linux the kernel's own table is the proof: every listener on the port
    // is on 127.0.0.1. Everywhere, a connection to the port on a non-loopback local address
    // is refused or times out, both meaning no listener there.
    if let Ok(table) = std::fs::read_to_string("/proc/net/tcp") {
        let listeners: Vec<&str> = table
            .lines()
            .skip(1)
            .filter(|l| {
                let mut f = l.split_whitespace();
                let local = f.nth(1).unwrap_or("");
                let state = f.nth(1).unwrap_or("");
                state == "0A" && local.ends_with(&format!(":{port:04X}"))
            })
            .collect();
        assert!(!listeners.is_empty(), "the listener is in /proc/net/tcp");
        for l in listeners {
            assert!(
                l.split_whitespace()
                    .nth(1)
                    .unwrap()
                    .starts_with("0100007F:"),
                "{l}"
            );
        }
    }
    for addr in local_non_loopback_addrs().into_iter().take(3) {
        let target = std::net::SocketAddr::from((addr, port));
        assert!(
            TcpStream::connect_timeout(&target, std::time::Duration::from_secs(1)).is_err(),
            "the page is reachable on {addr}"
        );
    }
    child.kill().unwrap();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The host's non-loopback IPv4 addresses: the one a routed UDP socket binds to, plus any
/// `ifconfig` or `ip` lists; empty on a host with none.
fn local_non_loopback_addrs() -> Vec<std::net::Ipv4Addr> {
    let mut addrs = BTreeSet::new();
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0")
        && socket.connect("10.255.255.255:1").is_ok()
        && let Ok(std::net::SocketAddr::V4(local)) = socket.local_addr()
    {
        addrs.insert(*local.ip());
    }
    let output = Command::new("ifconfig")
        .output()
        .or_else(|_| Command::new("ip").args(["-4", "addr"]).output());
    if let Ok(output) = output {
        for word in String::from_utf8_lossy(&output.stdout).split_whitespace() {
            if let Ok(a) = word
                .split('/')
                .next()
                .unwrap()
                .parse::<std::net::Ipv4Addr>()
            {
                addrs.insert(a);
            }
        }
    }
    addrs
        .into_iter()
        .filter(|a| !a.is_loopback() && !a.is_unspecified() && !a.is_broadcast())
        .filter(|a| a.octets()[0] != 255 && a.octets()[3] != 255)
        .collect()
}

/// The plan's CLI scenario: the page of a run directory lists the one step with its duration
/// and the mask's three planes as thumbnails.
#[test]
fn the_run_directory_form_lists_the_step_with_its_duration_and_three_planes() {
    let dir = scratch("rundir");
    let floor = repo_root().join("fixtures/images/floor.png");
    let run = dir.join("run");
    let out = rascal(&[
        "run",
        "rect",
        s(&floor),
        "--bbox",
        "0.2,0.2,0.6,0.6",
        "--out",
        s(&run),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let html = dash(&run, &dir.join("page.html"));
    let lines = journal(&run);
    assert_page_matches_journal(&html, &lines);
    let durations = attr_values(&html, "tr", "data-duration-ms");
    assert_eq!(durations.len(), lines.len());
    for (d, line) in durations.iter().zip(&lines) {
        assert_eq!(d, &line["duration_ms"].as_u64().unwrap_or(0).to_string());
    }
    let mask = lines.iter().find(|l| l["step"] == "rasterize").unwrap()["outputs"]["mask"]
        .as_str()
        .unwrap()
        .to_string();
    let table = asset_table(&html);
    for plane in ["core", "blend", "protect"] {
        let key = format!("{mask}/{plane}");
        assert!(
            figure_keys(&html, &mask).contains(&key),
            "{plane} thumbnail"
        );
        let (mime, bytes) = decode_data_uri(&table[&key]);
        assert_eq!(mime, "image/png");
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!(decoded.width(), 320, "{plane}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
