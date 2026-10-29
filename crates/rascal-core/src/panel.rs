//! The run page's panel registry (SPEC.md "Diagnostics"). A panel renders one view of one
//! journal line: the envelope and parameters, the artifacts as thumbnails, and from later
//! tasks the rulers' charts, the measures, the filler and pack calls. An op registers its panel
//! in the task that adds the op, so the view is never behind the tool.

use crate::journal::JournalLine;
use crate::workspace::Workspace;

/// What a panel renders for a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// A short heading.
    pub title: String,
    /// An HTML fragment, self-contained (images as `data:` URIs).
    pub html: String,
}

/// One panel.
pub trait Panel: Send + Sync {
    /// The panel's name, as `docs/diagnostics.md` lists it.
    fn name(&self) -> &'static str;
    /// Renders for a line, or `None` when the panel has nothing to say about it.
    fn render(&self, line: &JournalLine, workspace: &Workspace) -> Option<Rendered>;
}

/// The registered panels, in display order.
#[derive(Default)]
pub struct PanelRegistry {
    panels: Vec<Box<dyn Panel>>,
}

impl PanelRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a panel.
    pub fn register(&mut self, panel: Box<dyn Panel>) {
        self.panels.push(panel);
    }

    /// The panel names, in order.
    pub fn names(&self) -> Vec<&'static str> {
        self.panels.iter().map(|p| p.name()).collect()
    }

    /// Every panel's rendering for a line, in order.
    pub fn render(&self, line: &JournalLine, workspace: &Workspace) -> Vec<Rendered> {
        self.panels
            .iter()
            .filter_map(|p| p.render(line, workspace))
            .collect()
    }
}

/// Escapes text for HTML.
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Base64 (standard alphabet, padded), for `data:` URIs; written here to keep the dependency
/// list to the spec's.
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// A PNG `data:` URI.
pub fn png_data_uri(png_bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64(png_bytes))
}
