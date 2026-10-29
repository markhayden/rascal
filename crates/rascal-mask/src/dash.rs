//! `rascal dash <run or workspace> [--out page.html | --serve]`: the run page, built from the
//! journal and the operation records and nothing else (SPEC.md "Diagnostics"). It names every
//! journal line (steps, repeated steps, `not_applicable` and failed invocations, notes, filler
//! and pack calls) and nothing else, embeds every artifact the journal names as a `data:` URI,
//! and says when the journal was rebuilt by repair.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

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
    registry
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

/// Builds the page for a run directory or workspace.
pub fn build(root: &Path) -> Result<String> {
    let ws = Workspace::open(root)?;
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
        .replace(
            "{{CALLS}}",
            "<p class=\"empty\" data-calls=\"0\">No filler or pack calls in this journal.</p>",
        )
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

/// Serves the page on loopback only, printing the URL, until the process ends. Each request
/// rebuilds the page, so a running workspace can be watched.
pub fn serve(root: &Path, port: u16) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| Error::io(root, e))?;
    let addr = listener.local_addr().map_err(|e| Error::io(root, e))?;
    println!("http://{addr}/");
    let _ = std::io::stdout().flush();
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
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(body.as_bytes());
        let _ = stream.shutdown(std::net::Shutdown::Write);
    }
    Ok(())
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
