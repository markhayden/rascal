//! `rascal dash <run or workspace> [--out page.html | --document | --serve]`: the run page,
//! built from the journal and the operation records and nothing else (SPEC.md "Diagnostics").
//! It names every journal line (steps, repeated steps, `not_applicable` and failed invocations,
//! notes, filler and pack calls) and nothing else, embeds every artifact the journal names as a
//! `data:` URI, and says when the journal was rebuilt by repair.
//!
//! `--document` and `--serve` are the description's two renders (table-manners 3.2): a render
//! opens the run read-only and never changes a byte of it.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use sha2::{Digest, Sha256};

use rascal_core::Error;
use rascal_core::artifact::Artifact;
use rascal_core::error::Result;
use rascal_core::journal::{Event, Journal, JournalLine};
use rascal_core::op::Status;
use rascal_core::outdir::{self, OutKind};
use rascal_core::panel::{self, Panel, PanelRegistry, Rendered};
use rascal_core::workspace::Workspace;

const TEMPLATE: &str = include_str!("../pages/dash.html");

/// The base panels: every op's envelope and parameters, and its artifacts as thumbnails.
pub fn panels() -> PanelRegistry {
    let mut registry = PanelRegistry::new();
    registry.register(Box::new(EnvelopePanel));
    registry.register(Box::new(ArtifactsPanel));
    registry.register(Box::new(ProfilePanel));
    registry.register(Box::new(MeasuresPanel));
    registry
}

/// Every Series a `measure` line produced, as a chart, with the envelope's summaries beside it.
struct MeasuresPanel;

impl Panel for MeasuresPanel {
    fn name(&self) -> &'static str {
        "measures"
    }

    fn render(&self, line: &JournalLine, workspace: &Workspace) -> Option<Rendered> {
        if line.op.as_deref() != Some("measure") {
            return None;
        }
        let mut parts = Vec::new();
        for (port, identity) in &line.outputs {
            let Ok(Artifact::Series(series)) = workspace.load(identity) else {
                continue;
            };
            let values: Vec<f64> = if series.channels == 1 {
                series.values.clone()
            } else {
                series
                    .values
                    .chunks(series.channels as usize)
                    .map(|c| c[0])
                    .collect()
            };
            parts.push(format!(
                "<div class=\"profile\">{}<p class=\"meta\">{}: {} rows ({})</p></div>",
                rascal_ops::inspect::chart_svg(
                    &format!("{port}: {}", series.name),
                    &series.unit,
                    &values
                ),
                panel::escape(port),
                series.rows.len(),
                panel::escape(&series.unit)
            ));
        }
        if let Some(envelope) = &line.envelope {
            let rows: Vec<String> = envelope
                .measurements
                .iter()
                .map(|m| {
                    format!(
                        "<tr><td>{}</td><td>{:.4}</td><td>{}</td></tr>",
                        panel::escape(&m.name),
                        m.value,
                        panel::escape(&m.unit)
                    )
                })
                .collect();
            if !rows.is_empty() {
                parts.push(format!("<table class=\"summaries\"><thead><tr><th>summary</th><th>value</th><th>unit</th></tr></thead><tbody>{}</tbody></table>", rows.join("")));
            }
        }
        (!parts.is_empty()).then(|| Rendered {
            title: "measures".to_string(),
            html: parts.join(""),
        })
    }
}

/// A band profile's half-width per row as a chart, beside the strip of the image it measured.
struct ProfilePanel;

impl Panel for ProfilePanel {
    fn name(&self) -> &'static str {
        "profile"
    }

    fn render(&self, line: &JournalLine, workspace: &Workspace) -> Option<Rendered> {
        let mut parts = Vec::new();
        for (port, identity) in &line.outputs {
            let Ok(Artifact::BandProfile(band)) = workspace.load(identity) else {
                continue;
            };
            let chart = rascal_ops::inspect::chart_svg(
                &format!("{port}: half-width per row"),
                "px",
                &band.half,
            );
            // The strip: the image columns the band can reach, around the centre column.
            let strip = line
                .inputs
                .get("image")
                .and_then(|id| workspace.load(id).ok())
                .and_then(|artifact| match artifact {
                    Artifact::Image(image) => Some(image),
                    _ => None,
                })
                .map(|image| {
                    let reach = band.half.iter().cloned().fold(0.0, f64::max).ceil() as u32 + 4;
                    let (w, h) = (image.pixels.width(), image.pixels.height());
                    let x0 = band.column_px.saturating_sub(reach);
                    let x1 = (band.column_px + reach).min(w);
                    let crop = image.pixels.crop_imm(x0, 0, x1 - x0, h);
                    format!(
                        "<figure class=\"strip\"><img src=\"{}\" alt=\"columns {x0} to {x1} of {}\"><figcaption>columns {x0} to {x1} around {}</figcaption></figure>",
                        panel::png_data_uri(&png_bytes(&crop)),
                        panel::escape(&image.record.frame),
                        band.column_px
                    )
                })
                .unwrap_or_default();
            parts.push(format!(
                "<div class=\"profile\">{strip}{chart}<p class=\"meta\">thin {:.1} px, {} rows, constants {}</p></div>",
                band.thin_px,
                band.half.len(),
                panel::escape(&band.constants)
            ));
        }
        (!parts.is_empty()).then(|| Rendered {
            title: "profile".to_string(),
            html: parts.join(""),
        })
    }
}

struct EnvelopePanel;

impl Panel for EnvelopePanel {
    fn name(&self) -> &'static str {
        "envelope"
    }

    fn render(&self, line: &JournalLine, _: &Workspace) -> Option<Rendered> {
        let envelope = line.envelope.as_ref()?;
        let params = line
            .params
            .as_ref()
            .map(|p| serde_json::to_string_pretty(p).unwrap_or_default())
            .unwrap_or_default();
        let html = format!(
            "<details><summary>envelope: <span class=\"status-{s}\">{s}</span> {summary}</summary><pre>{env}</pre></details><details><summary>resolved parameters{constants}</summary><pre>{params}</pre></details>",
            s = status_name(envelope.status),
            summary = panel::escape(&envelope.summary),
            env = panel::escape(&serde_json::to_string_pretty(envelope).unwrap_or_default()),
            constants = line
                .params
                .as_ref()
                .and_then(|p| p.get("constants"))
                .and_then(|c| c.as_str())
                .map(|c| format!(" (constants {})", panel::escape(c)))
                .unwrap_or_default(),
            params = panel::escape(&params),
        );
        Some(Rendered {
            title: "envelope".to_string(),
            html,
        })
    }
}

struct ArtifactsPanel;

impl Panel for ArtifactsPanel {
    fn name(&self) -> &'static str {
        "artifacts"
    }

    fn render(&self, line: &JournalLine, workspace: &Workspace) -> Option<Rendered> {
        if line.outputs.is_empty() {
            return None;
        }
        let mut figures = Vec::new();
        for (port, identity) in &line.outputs {
            match workspace.load(identity) {
                Ok(Artifact::Image(_)) => figures.push(figure(port, identity, identity)),
                Ok(Artifact::Mask(_)) => {
                    for plane in PLANES {
                        figures.push(figure(
                            &format!("{port}.{plane}"),
                            identity,
                            &format!("{identity}/{plane}"),
                        ));
                    }
                }
                Ok(Artifact::Location { location, .. }) => {
                    figures.push(link(
                        port,
                        identity,
                        &format!("{} location", location.kind()),
                    ));
                }
                Ok(Artifact::Plan(plan)) => figures.push(link(
                    port,
                    identity,
                    &format!("plan, strip {} of {}", plan.strip, plan.width),
                )),
                Ok(Artifact::BandProfile(profile)) => figures.push(link(
                    port,
                    identity,
                    &format!("band profile, {} rows", profile.half.len()),
                )),
                Ok(Artifact::Series(series)) => figures.push(link(
                    port,
                    identity,
                    &format!(
                        "series {} ({} rows, {})",
                        series.name,
                        series.rows.len(),
                        series.unit
                    ),
                )),
                Err(e) => {
                    figures.push(format!("<figure data-identity=\"{}\"><figcaption>{}: not loadable: {}</figcaption></figure>", panel::escape(identity), panel::escape(port), panel::escape(&e.to_string())));
                }
            }
        }
        Some(Rendered {
            title: "artifacts".to_string(),
            html: format!("<div class=\"thumbs\">{}</div>", figures.join("")),
        })
    }
}

const PLANES: [&str; 3] = ["core", "blend", "protect"];

/// The first characters of an identity, for a caption.
fn short(identity: &str) -> String {
    identity.chars().take(19).collect()
}

/// A thumbnail that opens at full size; the bytes come from the page's asset table, keyed.
fn figure(label: &str, identity: &str, key: &str) -> String {
    format!(
        "<figure data-identity=\"{id}\" data-asset=\"{key}\"><a data-asset=\"{key}\" target=\"_blank\"><img data-asset=\"{key}\" alt=\"{label}\"></a><figcaption>{label}<br>{short}</figcaption></figure>",
        id = panel::escape(identity),
        key = panel::escape(key),
        label = panel::escape(label),
        short = panel::escape(&short(identity)),
    )
}

/// A non-image artifact: a link to its JSON in the asset table.
fn link(port: &str, identity: &str, caption: &str) -> String {
    format!(
        "<figure data-identity=\"{id}\" data-asset=\"{id}\"><a data-asset=\"{id}\" target=\"_blank\">{port}: {caption}</a><figcaption>{short}</figcaption></figure>",
        id = panel::escape(identity),
        port = panel::escape(port),
        caption = panel::escape(caption),
        short = panel::escape(&short(identity)),
    )
}

/// Every artifact the journal names, as `data:` URIs by asset key, each embedded once however
/// many lines name it: an image or a JSON artifact under its identity, a mask's planes under
/// `<identity>/<plane>`.
fn assets(lines: &[JournalLine], workspace: &Workspace) -> BTreeMap<String, String> {
    let mut table = BTreeMap::new();
    for identity in lines.iter().flat_map(|l| l.outputs.values()) {
        if table.contains_key(identity) || table.contains_key(&format!("{identity}/core")) {
            continue;
        }
        let json = |value: &dyn erased::Json| -> String {
            format!(
                "data:application/json;base64,{}",
                panel::base64(&value.to_json())
            )
        };
        match workspace.load(identity) {
            Ok(Artifact::Image(image)) => {
                table.insert(
                    identity.clone(),
                    panel::png_data_uri(&png_bytes(&image.pixels)),
                );
            }
            Ok(Artifact::Mask(mask)) => {
                for (plane, pixels) in
                    PLANES
                        .into_iter()
                        .zip([&mask.core, &mask.blend, &mask.protect])
                {
                    table.insert(
                        format!("{identity}/{plane}"),
                        panel::png_data_uri(&png_bytes(&image::DynamicImage::ImageLuma8(
                            pixels.clone(),
                        ))),
                    );
                }
            }
            Ok(Artifact::Location { location, .. }) => {
                table.insert(identity.clone(), json(&location));
            }
            Ok(Artifact::Plan(plan)) => {
                table.insert(identity.clone(), json(&*plan));
            }
            Ok(Artifact::BandProfile(profile)) => {
                table.insert(identity.clone(), json(&*profile));
            }
            Ok(Artifact::Series(series)) => {
                table.insert(identity.clone(), json(&*series));
            }
            Err(_) => {}
        }
    }
    table
}

/// Serialization without naming every type.
mod erased {
    pub trait Json {
        fn to_json(&self) -> Vec<u8>;
    }
    impl<T: serde::Serialize> Json for T {
        fn to_json(&self) -> Vec<u8> {
            serde_json::to_vec(self).unwrap_or_default()
        }
    }
}

fn png_bytes(pixels: &image::DynamicImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    let _ = pixels.write_to(&mut cursor, image::ImageFormat::Png);
    bytes
}

fn status_name(status: Status) -> String {
    serde_json::to_string(&status)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

/// Builds the page for a run directory or workspace, reading it and writing nothing there.
pub fn build(root: &Path) -> Result<String> {
    let ws = Workspace::open_read_only(root)?;
    let lines = match Journal::read(&ws.journal_path()) {
        Ok(lines) => lines,
        Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            Vec::new()
        }
        Err(e) => return Err(e),
    };
    let panels = panels();
    let rebuilt = lines.first().is_some_and(|l| {
        l.event == Event::Note
            && l.note
                .as_deref()
                .is_some_and(|n| n.contains("journal rebuilt by repair"))
    });
    let total_ms: u64 = lines
        .iter()
        .filter_map(|l| l.duration_ms)
        .sum::<u64>()
        .max(1);
    let mut seen_steps: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows = Vec::new();
    let mut steps = Vec::new();
    let mut elapsed = 0u64;
    for line in &lines {
        let step = line.step.clone().unwrap_or_else(|| "note".to_string());
        let repeat = {
            let n = seen_steps.entry(step.clone()).or_insert(0);
            *n += 1;
            *n
        };
        let status = line.envelope.as_ref().map(|e| e.status);
        let (class, status_text) = match (line.event, status) {
            (Event::Note, _) => ("note", "note".to_string()),
            (_, Some(Status::Ok)) => ("ok", "ok".to_string()),
            (_, Some(Status::NotApplicable)) => ("na", "not_applicable".to_string()),
            (_, Some(Status::MeasuredNothing)) => ("na", "measured_nothing".to_string()),
            (_, Some(Status::InvalidArgs)) => ("failed", "failed".to_string()),
            (_, None) => ("note", "no_envelope".to_string()),
        };
        let duration = line.duration_ms.unwrap_or(0);
        let left = elapsed as f64 / total_ms as f64 * 100.0;
        let width = (duration as f64 / total_ms as f64 * 100.0).max(0.5);
        elapsed += duration;
        let repeat_mark = if repeat > 1 {
            format!(" <span class=\"repeat\">(repeat {repeat})</span>")
        } else {
            String::new()
        };
        rows.push(format!(
            "<tr data-seq=\"{seq}\" data-step=\"{step}\" data-status=\"{status_text}\" data-repeat=\"{repeat}\" data-duration-ms=\"{duration}\"><td>{seq}</td><td>{step}{repeat_mark}</td><td>{op}</td><td class=\"status-{status_text}\">{status_text}</td><td>{duration} ms</td><td><div class=\"bar {class}\" style=\"margin-left:{left:.1}%;width:{width:.1}%\"></div></td></tr>",
            seq = line.seq,
            step = panel::escape(&step),
            op = panel::escape(line.op.as_deref().unwrap_or("")),
        ));
        let rendered = panels.render(line, &ws);
        let body = if line.event == Event::Note {
            format!(
                "<p class=\"meta\">{}</p>",
                panel::escape(line.note.as_deref().unwrap_or(""))
            )
        } else {
            rendered
                .iter()
                .map(|r| {
                    format!(
                        "<div class=\"panel\"><strong>{}</strong>{}</div>",
                        panel::escape(&r.title),
                        r.html
                    )
                })
                .collect::<Vec<_>>()
                .join("")
        };
        steps.push(format!(
            "<section data-seq=\"{seq}\" data-step=\"{step}\"><h3>#{seq} {step} <span class=\"meta\">{op} {version}</span>{repeat_mark}</h3>{body}</section>",
            seq = line.seq,
            step = panel::escape(&step),
            op = panel::escape(line.op.as_deref().unwrap_or("")),
            version = panel::escape(line.version.as_deref().unwrap_or("")),
        ));
    }
    let summary = format!(
        "{} journal lines, {} ops, {} ms of op time. Built from the journal and the operation records only.",
        lines.len(),
        lines.iter().filter(|l| l.event == Event::Op).count(),
        total_ms
    );
    Ok(TEMPLATE
        .replace("{{WORKSPACE}}", &panel::escape(&root.display().to_string()))
        .replace(
            "{{REBUILT_BADGE}}",
            if rebuilt {
                "<span class=\"badge rebuilt\">journal rebuilt by repair</span>"
            } else {
                ""
            },
        )
        .replace("{{SUMMARY}}", &panel::escape(&summary))
        .replace("{{WATERFALL_ROWS}}", &rows.join("\n"))
        .replace("{{CALLS}}", &calls_table(&lines))
        .replace("{{STEPS}}", &steps.join("\n"))
        .replace(
            "{{ASSETS}}",
            &serde_json::to_string(&assets(&lines, &ws))
                .unwrap_or_default()
                .replace("</", "<\\/"),
        ))
}

/// Writes the page to a file destination (overwritten) and returns the path.
pub fn write(root: &Path, out: &Path) -> Result<PathBuf> {
    let html = build(root)?;
    let out = outdir::prepare(out, OutKind::File)?;
    std::fs::write(&out, html).map_err(|e| Error::io(&out, e))?;
    Ok(out)
}

/// The document render (3.2): the page as one self-contained file at
/// `$XDG_CACHE_HOME/rascal/dash/<run id>.html` (else `~/.cache/…`), outside the run directory,
/// written whole (a temporary name, then a rename). Returns its absolute path.
pub fn document(root: &Path) -> Result<PathBuf> {
    let html = build(root)?;
    let run = root.canonicalize().map_err(|e| Error::io(root, e))?;
    let dir = cache_root()
        .ok_or_else(|| {
            Error::usage("dash: no cache directory (set XDG_CACHE_HOME or HOME) for --document")
        })?
        .join("rascal")
        .join("dash");
    if resolved(&dir).starts_with(&run) {
        return Err(Error::usage(format!(
            "dash: the document's directory {} is inside the run directory; a render never writes there",
            dir.display()
        )));
    }
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    let id = run_id(&run);
    let path = dir.join(format!("{id}.html"));
    let tmp = dir.join(format!(".{id}.html.{}.tmp", std::process::id()));
    std::fs::write(&tmp, html).map_err(|e| Error::io(&tmp, e))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(&path, e)
    })?;
    Ok(path)
}

/// `$XDG_CACHE_HOME` when it is absolute (the XDG rule: a relative value is ignored), else
/// `$HOME/.cache`; `None` with neither. The dash documents and the artifact previews live under
/// it.
pub fn cache_root() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|home| home.join(".cache"))
        })
}

/// A path with its longest existing prefix resolved through symlinks, so it compares with a
/// canonical path even before the rest of it exists.
fn resolved(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut head = path.to_path_buf();
    loop {
        if let Ok(real) = head.canonicalize() {
            return rest.iter().rev().fold(real, |p, c| p.join(c));
        }
        match (head.file_name().map(|n| n.to_os_string()), head.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                head = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// The document's name: the run's id from its `run.json` (`run.id`) when it is a plain file
/// name, else (a workspace, or a run directory without one) a digest of the directory's path.
fn run_id(run: &Path) -> String {
    let id = std::fs::read_to_string(run.join("run.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.get("run")?.get("id")?.as_str().map(str::to_string))
        .filter(|id| {
            !id.is_empty()
                && !id.starts_with('.')
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        });
    id.unwrap_or_else(|| {
        let digest = Sha256::digest(run.as_os_str().as_encoded_bytes());
        let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
        format!("dir-{hex}")
    })
}

/// Checks a `--frame-ancestor` origin: `http://` or `https://`, a host and an optional port,
/// nothing else (no path, query, user or space), so it is exactly one CSP source.
fn origin(text: &str) -> std::result::Result<String, String> {
    let bad = || {
        format!("{text:?} is not an origin: give http:// or https://, a host and an optional port")
    };
    let authority = text
        .strip_prefix("http://")
        .or_else(|| text.strip_prefix("https://"))
        .ok_or_else(bad)?;
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    let host_ok = !host.is_empty()
        && !host.starts_with(['.', '-'])
        && !host.ends_with(['.', '-'])
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
    let port_ok = port.is_none_or(|p| {
        !p.is_empty()
            && p.chars().all(|c| c.is_ascii_digit())
            && p.parse::<u16>().is_ok_and(|n| n > 0)
    });
    if host_ok && port_ok {
        Ok(text.to_string())
    } else {
        Err(bad())
    }
}

/// The server render (3.2): serves the page on loopback only and prints its URL as the one
/// line of stdout once listening, then serves until stdin reaches its end or SIGTERM arrives,
/// and returns (exit 0). Each request rebuilds the page, so a running workspace can be watched.
/// Every response carries `Content-Security-Policy: frame-ancestors <origin>` for the origin
/// given, else `frame-ancestors 'none'`.
pub fn serve(root: &Path, port: u16, frame_ancestor: Option<&str>) -> Result<()> {
    let policy = match frame_ancestor {
        Some(text) => format!(
            "frame-ancestors {}",
            origin(text).map_err(|e| Error::usage(format!("dash: --frame-ancestor {e}")))?
        ),
        None => "frame-ancestors 'none'".to_string(),
    };
    // A render opens the run read-only; a path that is not one fails here, before listening.
    Workspace::open_read_only(root)?;
    let (stop, stopped) = mpsc::channel::<()>();
    // Registered before the URL is printed, so a host that signals right after reading it is
    // still a clean exit.
    watch_sigterm(stop.clone())?;
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| Error::io(root, e))?;
    let addr = listener.local_addr().map_err(|e| Error::io(root, e))?;
    {
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "http://{addr}/").map_err(|e| Error::io(root, e))?;
        stdout.flush().map_err(|e| Error::io(root, e))?;
    }
    // The host owns the process through its stdin: its end, however it comes, stops the page.
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
        let _ = stop.send(());
    });
    let root = root.to_path_buf();
    std::thread::spawn(move || accept(&listener, &root, &policy));
    let _ = stopped.recv();
    Ok(())
}

/// Sends on `stop` when the process receives SIGTERM.
fn watch_sigterm(stop: mpsc::Sender<()>) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .map_err(|e| Error::op("dash", format!("the signal watcher: {e}")))?;
    let mut term = {
        let _entered = runtime.enter();
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(|e| Error::op("dash", format!("the signal watcher: {e}")))?
    };
    std::thread::spawn(move || {
        runtime.block_on(term.recv());
        let _ = stop.send(());
    });
    Ok(())
}

/// Answers one connection at a time with the page, until the process ends.
fn accept(listener: &TcpListener, root: &Path, policy: &str) {
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        // One idle or slow client must not hold the page for everyone.
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(30)));
        // Read the whole request head before answering: closing with unread bytes in the
        // receive buffer makes the kernel reset the connection instead of ending it.
        if !read_request_head(&mut stream) {
            continue;
        }
        let (status, body) = match build(root) {
            Ok(html) => ("200 OK", html),
            Err(e) => (
                "500 Internal Server Error",
                format!("<pre>{}</pre>", panel::escape(&e.to_string())),
            ),
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: {policy}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(body.as_bytes());
        let _ = stream.shutdown(std::net::Shutdown::Write);
    }
}

/// Reads until the blank line that ends an HTTP request head; false on EOF, an error or a head
/// over 16 KiB. The body of a GET is empty, so nothing is left unread.
fn read_request_head(stream: &mut std::net::TcpStream) -> bool {
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return false,
            Ok(n) => head.extend_from_slice(&buf[..n]),
        }
        if head.windows(4).any(|w| w == b"\r\n\r\n") {
            return true;
        }
        if head.len() > 16 * 1024 {
            return false;
        }
    }
}

/// The filler, pack and checker call rows, from the journal's `filler call:`, `pack call:` and
/// `checker call:` notes.
fn calls_table(lines: &[JournalLine]) -> String {
    let calls: Vec<(u64, &str, serde_json::Value)> = lines
        .iter()
        .filter_map(|l| {
            let note = l.note.as_deref()?;
            let (kind, text) = [
                ("filler", "filler call: "),
                ("pack", "pack call: "),
                ("checker", rascal_mask::checker::call::CALL_PREFIX),
            ]
            .into_iter()
            .find_map(|(kind, prefix)| Some((kind, note.strip_prefix(prefix)?)))?;
            serde_json::from_str::<serde_json::Value>(text)
                .ok()
                .map(|v| (l.seq, kind, v))
        })
        .collect();
    if calls.is_empty() {
        return "<p class=\"empty\" data-calls=\"0\">No filler, pack or checker calls in this journal.</p>"
            .to_string();
    }
    let rows: Vec<String> = calls
        .iter()
        .map(|(seq, kind, c)| {
            let s = |k: &str| panel::escape(c.get(k).and_then(|v| v.as_str()).unwrap_or(""));
            let n = |k: &str| c.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
            let (name, request, status) = match *kind {
                "filler" => (
                    s("filler"),
                    format!("{}x{}", n("request_width"), n("request_height")),
                    n("status").to_string(),
                ),
                "checker" => ("checker".to_string(), String::new(), n("exit").to_string()),
                _ => (
                    format!("{} {}", s("pack"), s("tool")),
                    format!("{} bytes", n("request_bytes")),
                    s("status"),
                ),
            };
            format!(
                "<tr data-call-seq=\"{seq}\" data-call-kind=\"{kind}\"><td>{seq}</td><td>{kind}</td><td>{name}</td><td>{}</td><td>{request}</td><td>{} / {} bytes</td><td>{} ms</td><td>{status}</td><td>{}</td></tr>",
                s("target"),
                n("request_bytes"),
                n("response_bytes"),
                n("duration_ms"),
                s("outcome")
            )
        })
        .collect();
    format!(
        "<table data-calls=\"{}\"><thead><tr><th>#</th><th>kind</th><th>name</th><th>target</th><th>request</th><th>bytes</th><th>duration</th><th>status</th><th>outcome</th></tr></thead><tbody>{}</tbody></table>",
        calls.len(),
        rows.join("")
    )
}
