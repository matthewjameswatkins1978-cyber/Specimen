//! Specimen's application controller and Hamesh workspace.
//! All physical and external inputs converge here; documents never live in widgets.
use crate::{
    artwork,
    document::{Alignment, Document, Element, History, Kind},
};
use anyhow::{bail, Result};
use hamesh_blocks as b;
use hamesh_core::{App, Component, Role};
use hamesh_layout::{LayoutOutput, Rect};
use hamesh_widgets::Composer;
use std::path::PathBuf;
use tiny_skia::{FilterQuality, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

pub struct View {
    pub app: App,
    pub layout: LayoutOutput,
    pub scene: hamesh_scene::Scene,
    pub paper: Rect,
    pub scale: f32,
}
struct Drag {
    before: Document,
    start: (f32, f32),
    origin: (f32, f32),
}
/// Canvas zoom: whole artboard, or the selection fitted to the
/// canvas. Selection zoom reuses the paper transform, so hit-testing,
/// drag and the Eye stay consistent; paper-space paint is clipped
/// to the canvas area whenever it overflows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zoom {
    Fit,
    Selected,
    Actual,
}
pub struct Editor {
    pub history: History,
    pub selected: Option<String>,
    pub revision: u64,
    pub status: String,
    pub project: PathBuf,
    pub focus: Option<String>,
    pub tab: String,
    pub guides: bool,
    pub layer_offset: usize,
    zoom: Zoom,
    /// Screen-pixel pan for Actual-size zoom. Clamped so the
    /// canvas center never leaves the paper.
    pan: (f32, f32),
    /// When true, the layer list scrolls to keep the selection visible.
    /// Manual Up/Down scrolling clears it until the selection changes.
    follow_selection: bool,
    /// Design-library browser open: template cards show. Collapsed,
    /// the room goes to layers and saved projects instead.
    library_open: bool,
    pub composer: Composer,
    pub active: Option<String>,
    select_all: bool,
    saved: String,
    drag: Option<Drag>,
    artwork_cache: Option<(String, Pixmap)>,
    thumbnails: Vec<Pixmap>,
    pub fonts: Vec<String>,
}

fn panel(
    s: &mut b::Scope,
    id: &str,
    width: Option<f32>,
    height: Option<f32>,
    surface: &str,
    kids: impl FnOnce(&mut b::Scope),
) {
    let c = Component::new(id, Role::Panel, id)
        .with_prop("layout", "column")
        .with_prop("pad", "12")
        .with_prop("gap", "8")
        .with_prop("surface", surface);
    let c = if let Some(w) = width {
        c.with_prop("w", &w.to_string()).with_prop("flex_grow", "0")
    } else {
        // Modern Hamesh defaults flex_grow to 0: a panel without an
        // explicit width must opt into free space (the old blanket
        // grow=1 is gone by upstream design).
        c.with_prop("flex_grow", "1")
    };
    let c = if let Some(h) = height {
        c.with_prop("h", &h.to_string())
    } else {
        c
    };
    s.branch(c, kids);
}
/// Fixed air between toolbar groups: a ghost holds the width,
/// paints nothing and never intercepts the Hand.
fn strut(s: &mut b::Scope, id: &str, width: f32) {
    s.leaf(
        Component::new(id, Role::Panel, id)
            .with_prop("ghost", "true")
            .with_prop("w", &width.to_string()),
    );
}
fn button(s: &mut b::Scope, id: &str, label: &str, primary: bool) {
    b::button(
        s,
        id,
        label,
        b::ButtonOpts::new(
            if primary {
                b::ButtonVariant::Primary
            } else {
                b::ButtonVariant::Ghost
            },
            id,
        ),
    );
    // QBC: no manual width — the framework measures button labels
    // intrinsically (measure/render contract, pinned in hamesh-layout).
}
fn label(s: &mut b::Scope, id: &str, text: &str) {
    b::caption(s, id, text);
    // QBC: no manual width — the framework measures caption text
    // intrinsically (measure/render contract, pinned in hamesh-layout).
    if let Some(c) = s.app.get_mut(id) {
        c.props.insert("h".into(), "22".into());
        c.props.insert("pad".into(), "0".into());
        c.props.insert("font_size".into(), "12".into());
    }
}
fn row(s: &mut b::Scope, id: &str, height: f32, kids: impl FnOnce(&mut b::Scope)) {
    b::row(
        s,
        id,
        b::LayoutOpts {
            h: Some(height),
            grow: Some(0.0),
            pad: 0.0,
            ..b::LayoutOpts::default()
        },
        kids,
    );
}
/// Library capacity: the (layer rows, saved rows) that fit the fixed
/// library panel without clipping. All heights are measured chrome
/// (panel pad 12, gap 8). Expanded: heading row 36, three template
/// cards 3 * (84 + 22), blank 36, layers heading 36, position
/// caption 22. Collapsed: heading row 36, blank 36, layers heading
/// 36, position caption 22. Both add layer rows 28, section labels
/// 22, saved/recovery rows 30, plus one spacer kid. Short panels shed
/// saved rows before layer rows (minimum 1); the latest autosave
/// recovery always stays visible. The saved header only renders
/// when saved rows fit.
/// Largest scale that fits the element box in the canvas area,
/// clamped so tiny elements cannot blow navigation up.
pub(crate) fn selection_scale(area_w: f32, area_h: f32, e: &Element) -> f32 {
    ((area_w - 40.0) / e.w.max(0.01))
        .min((area_h - 32.0) / e.h.max(0.01))
        .clamp(0.01, 8.0)
}
/// Pan bounds in screen pixels: the canvas center must stay on
/// the paper, so panning can never lose the artwork.
pub(crate) fn clamp_pan(pan: (f32, f32), paper_w: f32, paper_h: f32) -> (f32, f32) {
    (
        pan.0.clamp(-paper_w / 2.0, paper_w / 2.0),
        pan.1.clamp(-paper_h / 2.0, paper_h / 2.0),
    )
}
pub(crate) fn library_capacity(
    panel_h: f32,
    saved_available: usize,
    has_recovery: bool,
    expanded: bool,
) -> (usize, usize) {
    // Measured fixed chrome per library mode, plus one spacer kid.
    let (fixed, base) = if expanded {
        (448.0, 11.0)
    } else {
        (130.0, 5.0)
    };
    fn fits(
        panel_h: f32,
        fixed: f32,
        base: f32,
        layers: usize,
        saved: usize,
        has_recovery: bool,
    ) -> bool {
        let sections = if saved > 0 {
            22.0 + 30.0 * saved as f32
        } else {
            0.0
        } + if has_recovery { 22.0 + 30.0 } else { 0.0 };
        let kids = base
            + layers as f32
            + if saved > 0 { 1.0 + saved as f32 } else { 0.0 }
            + if has_recovery { 2.0 } else { 0.0 };
        24.0 + fixed + 28.0 * layers as f32 + sections + 8.0 * (kids - 1.0) <= panel_h
    }
    let saved_available = saved_available.min(2);
    for layers in (1..=8).rev() {
        if fits(panel_h, fixed, base, layers, saved_available, has_recovery) {
            return (layers, saved_available);
        }
    }
    for saved in (0..saved_available).rev() {
        for layers in (1..=8).rev() {
            if fits(panel_h, fixed, base, layers, saved, has_recovery) {
                return (layers, saved);
            }
        }
    }
    (1, 0)
}
impl Editor {
    pub fn new(project: PathBuf) -> Result<Self> {
        let doc = Document::template(0);
        let thumbnails = (0..3)
            .map(|i| artwork::render(&Document::template(i)))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            history: History::new(doc),
            selected: None,
            revision: 1,
            status: "Ready. Select a layer or click the artwork.".into(),
            project,
            focus: None,
            tab: "type".into(),
            guides: true,
            layer_offset: 0,
            follow_selection: true,
            library_open: true,
            zoom: Zoom::Fit,
            pan: (0.0, 0.0),
            composer: Composer::new(),
            active: None,
            select_all: false,
            saved: String::new(),
            drag: None,
            artwork_cache: None,
            thumbnails,
            fonts: artwork::fonts(),
        })
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    pub fn fingerprint(&self) -> String {
        serde_json::to_string(&self.history.document).unwrap_or_default()
    }
    pub fn dirty(&self) -> bool {
        self.saved != self.save_identity()
    }
    fn save_identity(&self) -> String {
        format!("{}|{}", self.project.to_string_lossy(), self.fingerprint())
    }
    pub fn export_path(&self) -> PathBuf {
        let name = self
            .project
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let name = if let Some(stem) = name.strip_suffix(".specimen.json") {
            format!("{stem}.png")
        } else {
            format!("{name}.png")
        };
        self.project.with_file_name(name)
    }
    /// Document palette in encounter order: canvas paper first,
    /// then each element colour once, capped so the swatch row
    /// fits the inspector. A swatch paints the selection, keeping
    /// the artwork inside its own palette unless the hex field
    /// says otherwise.
    pub fn palette(&self) -> Vec<String> {
        let mut out = vec![self.history.document.background.clone()];
        for e in &self.history.document.elements {
            if !out.iter().any(|c| c.eq_ignore_ascii_case(&e.color)) {
                out.push(e.color.clone());
            }
        }
        out.truncate(6);
        out
    }
    /// Human-friendly inspector heading: what the selection is and,
    /// for text, its content — never a bare internal identifier.
    fn inspector_title(&self) -> String {
        match self.element() {
            Some(e) => match &e.kind {
                Kind::Text { text, .. } => {
                    let first = text.lines().next().unwrap_or("").trim();
                    let short: String = first.chars().take(24).collect();
                    if short.is_empty() {
                        "Text".into()
                    } else {
                        format!("Text · {short}")
                    }
                }
                _ => e.name.clone(),
            },
            None => "Artwork".into(),
        }
    }
    pub fn element(&self) -> Option<&Element> {
        let id = self.selected.as_ref()?;
        self.history.document.elements.iter().find(|e| &e.id == id)
    }
    fn field_value(&self, field: &str) -> String {
        let d = &self.history.document;
        if field == "path" {
            return self.project.to_string_lossy().into_owned();
        }
        match field {
            "title" => return d.title.clone(),
            "background" => return d.background.clone(),
            "canvas_w" => return d.width.to_string(),
            "canvas_h" => return d.height.to_string(),
            _ => {}
        }
        let Some(e) = self.element() else {
            return String::new();
        };
        match field {
            "x" => e.x.to_string(),
            "y" => e.y.to_string(),
            "w" => e.w.to_string(),
            "h" => e.h.to_string(),
            "color" => e.color.clone(),
            _ => match &e.kind {
                Kind::Text {
                    text,
                    font,
                    weight,
                    size,
                    tracking,
                    leading,
                    ..
                } => match field {
                    "text" => text.clone(),
                    "font" => font.clone(),
                    "weight" => weight.to_string(),
                    "size" => size.to_string(),
                    "tracking" => tracking.to_string(),
                    "leading" => leading.to_string(),
                    _ => String::new(),
                },
                _ => String::new(),
            },
        }
    }
    fn field(&self, s: &mut b::Scope, field: &str, title: &str) {
        row(s, &format!("row-{field}"), 36.0, |s| {
            let c = Component::new(&format!("label-{field}"), Role::Text, title)
                .with_prop("text", title)
                .with_prop("w", "92")
                .with_prop("font_size", "12")
                .with_prop("text_token", "muted")
                .with_prop("h", "36");
            s.leaf(c);
            let id = format!("field-{field}");
            let active = self.active.as_deref() == Some(field);
            let text = if active {
                self.composer.shown_text()
            } else {
                self.field_value(field)
            };
            let mut c = Component::new(&id, Role::TextInput, title)
                .with_prop("text", &text)
                .with_prop("h", "36")
                .with_prop("flex_grow", "1")
                .with_prop("surface", "raised")
                .with_prop("radius", "6")
                .with_prop("border", "1")
                .with_prop("font_size", "16")
                .with_prop("border_token", if active { "focus" } else { "border" })
                .with_prop("field", field)
                .with_prop("commit", "Enter or blur; Escape cancels");
            if active {
                c = c
                    .with_prop("focused", "true")
                    .with_prop("caret_x", &self.composer.caret_x(220.0).to_string());
            }
            s.leaf(c);
        });
    }
    pub fn view(&self, width: u32, height: u32) -> View {
        let mut app = App::new("specimen");
        app.get_mut("specimen").unwrap().props.extend([
            ("pad".into(), "0".into()),
            ("gap".into(), "0".into()),
            ("surface".into(), "bg".into()),
        ]);
        {
            let mut root = b::Scope::root(&mut app, "specimen");
            row(&mut root, "header", 62.0, |s| {
                panel(s, "brand", Some(204.0), Some(62.0), "header", |s| {
                    b::title(s, "brand-title", "Specimen");
                });
                panel(s, "project-header", None, Some(62.0), "header", |s| {
                    // The header names unsaved work, not just the title:
                    // dirty state belongs where the eye rests.
                    let title = if self.dirty() {
                        format!("{} · unsaved", self.history.document.title)
                    } else {
                        self.history.document.title.clone()
                    };
                    b::heading(s, "project-title", &title);
                });
                b::row(
                    s,
                    "file-actions",
                    b::LayoutOpts {
                        w: Some(280.0),
                        h: Some(62.0),
                        pad: 12.0,
                        gap: 8.0,
                        grow: Some(0.0),
                        ..Default::default()
                    },
                    |s| {
                        button(s, "save", "Save", false);
                        button(s, "open", "Open", false);
                        button(s, "export", "Export PNG", true);
                    },
                );
            });
            // Draw, history and view read as groups: 16px ghost
            // struts separate them at twice the standard 8px gap.
            // The file actions already live in the header, so the
            // toolbar needs no mode label.
            row(&mut root, "toolbar", 48.0, |s| {
                b::spacer(s, "toolbar-spacer");
                button(s, "add-text", "+ Text", false);
                button(s, "add-rect", "+ Rectangle", false);
                button(s, "add-ellipse", "+ Ellipse", false);
                strut(s, "tools-gap", 16.0);
                button(s, "undo", "Undo", false);
                button(s, "redo", "Redo", false);
                strut(s, "history-gap", 16.0);
                button(
                    s,
                    "zoom",
                    match self.zoom {
                        Zoom::Fit => "Zoom: Fit",
                        Zoom::Selected => "Zoom: Selection",
                        Zoom::Actual => "Zoom: Actual",
                    },
                    false,
                );
                button(
                    s,
                    "guides",
                    if self.guides {
                        "Guides on"
                    } else {
                        "Guides off"
                    },
                    false,
                );
            });
            b::row(
                &mut root,
                "workspace",
                b::LayoutOpts {
                    pad: 0.0,
                    gap: 1.0,
                    grow: Some(1.0),
                    ..b::LayoutOpts::default()
                },
                |s| {
                    panel(
                        s,
                        "library",
                        Some(212.0),
                        Some(height.saturating_sub(140) as f32),
                        "window",
                        |s| {
                            row(s, "library-heading", 36.0, |s| {
                                label(s, "library-title", "DESIGN LIBRARY");
                                b::spacer(s, "library-head-space");
                                button(
                                    s,
                                    "library-toggle",
                                    if self.library_open { "-" } else { "+" },
                                    false,
                                );
                                if let Some(c) = s.app.get_mut("library-toggle") {
                                    c.name = if self.library_open {
                                        "Collapse design library"
                                    } else {
                                        "Expand design library"
                                    }
                                    .into();
                                }
                            });
                            if self.library_open {
                                for (i, name) in ["After Frequency", "Neue Form", "Soft Signal"]
                                    .iter()
                                    .enumerate()
                                {
                                    let id = format!("template-{i}");
                                    button(s, &id, "", false);
                                    if let Some(c) = s.app.get_mut(&id) {
                                        c.name = format!("Open starter: {name}");
                                        c.props.insert("h".into(), "84".into());
                                        c.props.insert("template".into(), i.to_string());
                                    }
                                    label(s, &format!("template-title-{i}"), name);
                                }
                            }
                            button(s, "blank", "New blank canvas", false);
                            row(s, "layers-heading", 36.0, |s| {
                                label(s, "layers-title", "LAYERS");
                                button(s, "layers-up", "Up", false);
                                button(s, "layers-down", "Down", false);
                            });
                            // Library inventory first: the layer list must
                            // yield space to saved projects and autosave
                            // recovery so nothing spills past the panel.
                            let saved = self.project.parent().unwrap_or(std::path::Path::new("."));
                            let mut projects = std::fs::read_dir(saved)
                                .into_iter()
                                .flatten()
                                .filter_map(|e| e.ok())
                                .map(|e| e.path())
                                .filter(|p| p.to_string_lossy().ends_with(".specimen.json"))
                                .collect::<Vec<_>>();
                            projects.sort();
                            // QBC workspace: saved projects and autosave
                            // recovery are separate regions. Recovery files
                            // present as recovery, not as project names.
                            // Recovery = "recovery.specimen.json" or a
                            // "recovery-*" timestamped autosave. Note
                            // file_stem strips only the last extension, so
                            // match on the full file name here (the display
                            // stem below strips ".specimen" separately). A
                            // user project literally named Recovery would
                            // land here; accepted over showing raw autosave
                            // names as projects.
                            let (recovery, projects): (Vec<_>, Vec<_>) =
                                projects.into_iter().partition(|p| {
                                    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                                        n == "recovery.specimen.json" || n.starts_with("recovery-")
                                    })
                                });
                            // Most recent autosave only, by file
                            // modification time: older recovery files stay
                            // on disk but don't crowd the list.
                            let mut recovery = recovery;
                            recovery.sort_by_key(|p| {
                                std::fs::metadata(p)
                                    .and_then(|m| m.modified())
                                    .unwrap_or(std::time::UNIX_EPOCH)
                            });
                            let recovery_latest = recovery.into_iter().next_back();
                            // Fit measured chrome in the fixed panel: shed
                            // saved rows before layer rows so the recovery
                            // entry is never the clipped casualty.
                            let (rows, saved_shown) = library_capacity(
                                height.saturating_sub(140) as f32,
                                projects.len(),
                                recovery_latest.is_some(),
                                self.library_open,
                            );
                            // Selection follow: keep the selected layer
                            // inside the visible window. Manual Up/Down
                            // scrolling holds its position until the
                            // selection changes again.
                            let total = self.history.document.elements.len();
                            let max_start = total.saturating_sub(rows.min(total.max(1)));
                            let mut start = self.layer_offset.min(max_start);
                            if self.follow_selection {
                                if let Some(sel) = self.selected.as_deref() {
                                    if let Some(pos) = self
                                        .history
                                        .document
                                        .elements
                                        .iter()
                                        .rev()
                                        .position(|e| e.id == sel)
                                    {
                                        if pos < start {
                                            start = pos;
                                        } else if pos >= start + rows {
                                            start = (pos + 1 - rows).min(max_start);
                                        }
                                    }
                                }
                            }
                            let end = (start + rows).min(total);
                            let position = if total == 0 {
                                "No layers".to_string()
                            } else {
                                format!("Showing {}-{} of {}", start + 1, end, total)
                            };
                            label(s, "layers-position", &position);
                            for (n, e) in self
                                .history
                                .document
                                .elements
                                .iter()
                                .enumerate()
                                .rev()
                                .skip(start)
                                .take(rows)
                            {
                                let id = format!("layer-{}", e.id);
                                let name = if self.selected.as_ref() == Some(&e.id) {
                                    format!("> {}", e.name)
                                } else {
                                    e.name.clone()
                                };
                                button(s, &id, &name, self.selected.as_ref() == Some(&e.id));
                                if let Some(c) = s.app.get_mut(&id) {
                                    c.props.insert("h".into(), "28".into());
                                    c.props.insert("element_index".into(), n.to_string());
                                }
                            }
                            b::spacer(s, "library-space");
                            // No empty section header: when no saved
                            // row fits, the header would promise
                            // projects the list cannot show.
                            if saved_shown > 0 {
                                label(s, "saved-section", "SAVED PROJECTS");
                            }
                            for (i, p) in projects.into_iter().take(saved_shown).enumerate() {
                                let id = format!("saved-{i}");
                                // Display the project stem: the full
                                // filename is kept in project_path.
                                let stem = p
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .and_then(|s| s.strip_suffix(".specimen"))
                                    .unwrap_or("project");
                                button(s, &id, stem, false);
                                if let Some(c) = s.app.get_mut(&id) {
                                    c.props.insert(
                                        "project_path".into(),
                                        p.to_string_lossy().into_owned(),
                                    );
                                    c.props.insert("h".into(), "30".into());
                                }
                            }
                            if let Some(p) = recovery_latest {
                                label(s, "recovery-section", "AUTOSAVE RECOVERY");
                                button(s, "saved-recovery", "Recovered work", false);
                                if let Some(c) = s.app.get_mut("saved-recovery") {
                                    c.props.insert(
                                        "project_path".into(),
                                        p.to_string_lossy().into_owned(),
                                    );
                                    c.props.insert("h".into(), "30".into());
                                }
                            }
                        },
                    );
                    panel(
                        s,
                        "stage",
                        None,
                        Some(height.saturating_sub(140) as f32),
                        "bg",
                        |s| {
                            row(s, "canvas-heading", 30.0, |s| {
                                label(s, "canvas-title", "ARTBOARD / FIT");
                                b::spacer(s, "canvas-label-space");
                                label(
                                    s,
                                    "canvas-size",
                                    &format!(
                                        "{} x {} px",
                                        self.history.document.width, self.history.document.height
                                    ),
                                );
                            });
                            s.leaf(
                                Component::new("canvas", Role::Panel, "Live artwork canvas")
                                    .with_prop("surface", "bg")
                                    .with_prop("pad", "0")
                                    .with_prop("flex_grow", "1")
                                    .with_prop("document", &self.fingerprint())
                                    .with_prop(
                                        "interaction",
                                        "click selects; drag moves; arrows nudge",
                                    ),
                            );
                            label(
                                s,
                                "canvas-hint",
                                "Click to select  /  Drag to position  /  Arrow keys to nudge",
                            );
                        },
                    );
                    panel(
                        s,
                        "inspector",
                        Some(286.0),
                        Some(height.saturating_sub(140) as f32),
                        "window",
                        |s| {
                            b::heading(s, "inspector-title", &self.inspector_title());
                            row(s, "inspector-tabs", 36.0, |s| {
                                for (id, name) in
                                    [("type", "Type"), ("layout", "Layout"), ("canvas", "Canvas")]
                                {
                                    button(s, &format!("tab-{id}"), name, self.tab == id);
                                }
                            });
                            if self.tab == "canvas" {
                                label(s, "document-section", "DOCUMENT");
                                for (f, l) in [
                                    ("title", "Title"),
                                    ("canvas_w", "Width (px)"),
                                    ("canvas_h", "Height (px)"),
                                    ("background", "Paper"),
                                    ("path", "Save path"),
                                ] {
                                    self.field(s, f, l);
                                }
                                label(s, "path-hint", "PNG uses the project path with .png.");
                                button(s, "save-project", "Save project", true);
                            } else if let Some(e) = self.element() {
                                if self.tab == "type" {
                                    if matches!(e.kind, Kind::Text { .. }) {
                                        label(s, "type-section", "TYPOGRAPHY");
                                        for (f, l) in [
                                            ("text", "Content"),
                                            ("font", "Family"),
                                            ("weight", "Weight"),
                                        ] {
                                            self.field(s, f, l);
                                        }
                                        // Common weights as one-tap choices with
                                        // visible state; the numeric field stays
                                        // for exact values.
                                        row(s, "weight-quick", 36.0, |s| {
                                            let current = match &e.kind {
                                                Kind::Text { weight, .. } => *weight,
                                                _ => 0,
                                            };
                                            for (id, w) in [
                                                ("weight-400", 400u16),
                                                ("weight-500", 500u16),
                                                ("weight-600", 600u16),
                                                ("weight-700", 700u16),
                                            ] {
                                                button(s, id, &w.to_string(), current == w);
                                            }
                                        });
                                        for (f, l) in [
                                            ("size", "Size (px)"),
                                            ("tracking", "Tracking (px)"),
                                            ("leading", "Leading (x)"),
                                        ] {
                                            self.field(s, f, l);
                                        }
                                        button(s, "next-font", "Next available font", false);
                                        // Stateful text alignment: the active
                                        // option reads selected, so frame-level
                                        // text alignment cannot be mistaken for
                                        // the artboard alignment on the Layout tab.
                                        let text_align = match &e.kind {
                                            Kind::Text { align, .. } => *align,
                                            _ => Alignment::Left,
                                        };
                                        row(s, "alignment", 36.0, |s| {
                                            for (id, l, a) in [
                                                ("align-left", "Left", Alignment::Left),
                                                ("align-center", "Centre", Alignment::Center),
                                                ("align-right", "Right", Alignment::Right),
                                            ] {
                                                button(s, id, l, text_align == a);
                                            }
                                        });
                                    } else {
                                        label(s, "shape-section", "SHAPE FILL");
                                    }
                                    self.field(s, "color", "Colour");
                                    row(s, "palette", 36.0, |s| {
                                        for (i, hex) in self.palette().iter().enumerate() {
                                            let id = format!("swatch-{i}");
                                            button(s, &id, "", false);
                                            if let Some(c) = s.app.get_mut(&id) {
                                                c.name = format!("Paint selection {hex}");
                                                c.props.insert("h".into(), "28".into());
                                                c.props.insert("w".into(), "28".into());
                                            }
                                        }
                                        b::spacer(s, "palette-space");
                                    });
                                } else {
                                    label(s, "geometry-section", "GEOMETRY / DOCUMENT PIXELS");
                                    for (f, l) in [
                                        ("x", "X (px)"),
                                        ("y", "Y (px)"),
                                        ("w", "Width (px)"),
                                        ("h", "Height (px)"),
                                    ] {
                                        self.field(s, f, l);
                                    }
                                    label(s, "align-section", "ALIGN TO ARTBOARD");
                                    row(s, "position-horizontal", 36.0, |s| {
                                        button(s, "position-left", "Left", false);
                                        button(s, "position-center", "Centre", false);
                                        button(s, "position-right", "Right", false);
                                    });
                                    row(s, "position-vertical", 36.0, |s| {
                                        button(s, "position-top", "Top", false);
                                        button(s, "position-middle", "Middle", false);
                                        button(s, "position-bottom", "Bottom", false);
                                    });
                                    row(s, "stacking", 36.0, |s| {
                                        button(s, "back", "To back", false);
                                        button(s, "front", "To front", false);
                                    });
                                }
                                row(s, "element-actions", 36.0, |s| {
                                    button(s, "duplicate", "Duplicate", false);
                                    button(s, "delete", "Delete", false);
                                });
                            } else {
                                // QBC empty state: a genuinely useful
                                // document overview, not just a hint.
                                label(s, "overview-section", "DOCUMENT OVERVIEW");
                                label(s, "overview-title", &self.history.document.title);
                                label(
                                    s,
                                    "overview-stats",
                                    &format!(
                                        "{} layers · {} x {} px{}",
                                        self.history.document.elements.len(),
                                        self.history.document.width,
                                        self.history.document.height,
                                        if self.dirty() { " · unsaved" } else { "" },
                                    ),
                                );
                                label(s, "selection-hint", "Select text or a shape on the canvas.");
                                self.field(s, "background", "Paper");
                                button(s, "document-settings", "Document settings", false);
                            }
                            b::spacer(s, "inspector-space");
                            label(s, "commit-hint", "Enter applies / Esc cancels");
                            label(s, "undo-hint", "Ctrl+Z undo / Ctrl+Shift+Z redo");
                        },
                    );
                },
            );
            row(&mut root, "footer", 30.0, |s| {
                label(
                    s,
                    "status",
                    &format!(
                        "{}  {}",
                        if self.dirty() { "*" } else { "Saved" },
                        self.status
                    ),
                );
                b::spacer(s, "footer-space");
                label(s, "engine-label", "HAMESH / CPU");
            });
        }
        // Hamesh currently has no intrinsic text measurement/min-size policy.
        // Bound chrome to the viewport using the supported size props.
        for c in app.components.values_mut() {
            if c.props.get("ghost").map(String::as_str) == Some("true") {
                c.props.insert("pad".into(), "0".into());
            }
        }
        for (id, w) in [
            ("layers-title", 58.0),
            ("layers-up", 44.0),
            ("layers-down", 62.0),
            ("canvas-title", 130.0),
            ("canvas-size", 170.0),
            ("status", (width as f32 - 200.0).max(100.0)),
            ("engine-label", 160.0),
        ] {
            if let Some(c) = app.get_mut(id) {
                c.props.insert("w".into(), w.to_string());
            }
        }
        // Selection headings run longer (element name + zoom): give
        // the size label room only in that mode, keeping the Fit
        // heading tight.
        if matches!(self.zoom, Zoom::Selected) && self.selected.is_some() {
            if let Some(c) = app.get_mut("canvas-size") {
                c.props.insert("w".into(), "220".into());
            }
        }
        if let Some(c) = app.get_mut("workspace") {
            c.props
                .insert("h".into(), height.saturating_sub(140).to_string());
        }
        // Disable unavailable history actions in the actual semantic tree.
        for (id, enabled) in [
            ("undo", self.history.can_undo()),
            ("redo", self.history.can_redo()),
        ] {
            if let Some(c) = app.get_mut(id) {
                c.enabled = enabled;
            }
        }
        if let Some(id) = self.focus.as_deref() {
            if let Some(c) = app.get_mut(id) {
                c.props.insert("border_token".into(), "focus".into());
            }
        }
        app.revision = self.revision;
        let mut layout = hamesh_layout::compute(&app, width as f32, height as f32);
        let area = layout.get("canvas").unwrap().rect.clone();
        let d = &self.history.document;
        let fit = ((area.w - 40.0) / d.width as f32)
            .min((area.h - 32.0) / d.height as f32)
            .max(0.01);
        // Selection zoom centers the element in the canvas at the
        // largest scale that fits it. Nothing selected: honest Fit.
        let (scale, selected) = match (self.zoom, self.selected.as_deref()) {
            (Zoom::Actual, _) => (1.0, None),
            (Zoom::Selected, Some(id)) => match d.elements.iter().find(|e| e.id == id) {
                Some(e) => (selection_scale(area.w, area.h, e), Some(e)),
                None => (fit, None),
            },
            _ => (fit, None),
        };
        // True size centers the artboard; arrow keys explore it by
        // panning. The clamp keeps the canvas center on the paper.
        let paper = if self.zoom == Zoom::Actual {
            Rect {
                x: area.x + area.w / 2.0 - d.width as f32 / 2.0 + self.pan.0,
                y: area.y + area.h / 2.0 - d.height as f32 / 2.0 + self.pan.1,
                w: d.width as f32,
                h: d.height as f32,
            }
        } else if let Some(e) = selected {
            Rect {
                x: area.x + area.w / 2.0 - (e.x + e.w / 2.0) * scale,
                y: area.y + area.h / 2.0 - (e.y + e.h / 2.0) * scale,
                w: d.width as f32 * scale,
                h: d.height as f32 * scale,
            }
        } else {
            Rect {
                x: area.x + (area.w - d.width as f32 * scale) / 2.0,
                y: area.y + (area.h - d.height as f32 * scale) / 2.0,
                w: d.width as f32 * scale,
                h: d.height as f32 * scale,
            }
        };
        // The heading names the zoom mode and quantifies it: artwork
        // size and live fit percentage, or the focused element and
        // its zoom when the selection fills the canvas.
        if let Some(c) = app.get_mut("canvas-title") {
            c.props.insert(
                "text".into(),
                match (self.zoom, selected) {
                    (Zoom::Actual, _) => "ACTUAL SIZE",
                    (_, Some(_)) => "SELECTION / FIT",
                    _ => "ARTBOARD / FIT",
                }
                .into(),
            );
        }
        if let Some(c) = app.get_mut("canvas-size") {
            let text = match selected {
                Some(e) => {
                    let shown = if e.name.chars().count() > 16 {
                        format!("{}…", e.name.chars().take(15).collect::<String>())
                    } else {
                        e.name.clone()
                    };
                    format!(
                        "{} · {:.0} x {:.0} px · {}%",
                        shown,
                        e.w,
                        e.h,
                        (scale * 100.0).round()
                    )
                }
                None => format!(
                    "{} x {} px · {}%",
                    d.width,
                    d.height,
                    (scale * 100.0).round()
                ),
            };
            c.props.insert("text".into(), text);
        }
        // Read-only document geometry in the Eye. Ghost nodes paint nothing and
        // never intercept Hamesh's pointer dispatch. Canvas stacking stays canonical.
        for (i, e) in d.elements.iter().enumerate() {
            let id = format!("art-{}", e.id);
            let c = Component::new(&id, Role::Other("ArtworkElement".into()), &e.name)
                .with_prop("ghost", "true")
                .with_prop("overlay", "true")
                .with_prop("ox", &(paper.x + e.x * scale).to_string())
                .with_prop("oy", &(paper.y + e.y * scale).to_string())
                .with_prop("w", &(e.w * scale).to_string())
                .with_prop("h", &(e.h * scale).to_string())
                .with_prop("element", &serde_json::to_string(e).unwrap())
                .with_prop("stack_index", &i.to_string());
            app.add("canvas", c);
        }
        app.revision = self.revision;
        // Overlay props provide absolute Eye bounds; fixed canvas size remains the
        // measured chrome layout. Do not recompute flex flow with ghost descendants.
        for e in &d.elements {
            layout.nodes.insert(
                format!("art-{}", e.id),
                hamesh_layout::LayoutNode {
                    rect: Rect {
                        x: paper.x + e.x * scale,
                        y: paper.y + e.y * scale,
                        w: e.w * scale,
                        h: e.h * scale,
                    },
                    effectively_visible: true,
                    clipped_by_parent: e.x < 0.0
                        || e.y < 0.0
                        || e.x + e.w > d.width as f32
                        || e.y + e.h > d.height as f32,
                    z: 100,
                },
            );
        }
        let scene = hamesh_scene::build(&app, &layout, self.revision);
        View {
            app,
            layout,
            scene,
            paper,
            scale,
        }
    }
    pub fn render(&mut self, view: &View) -> Result<Pixmap> {
        let mut frame = hamesh_render::render(&view.scene).pixmap;
        for i in 0..3 {
            if let Some(n) = view.layout.get(&format!("template-{i}")) {
                let p = &self.thumbnails[i];
                let r = &n.rect;
                let scale = ((r.w - 12.0) / p.width() as f32).min((r.h - 10.0) / p.height() as f32);
                Self::blit(
                    &mut frame,
                    p,
                    r.x + (r.w - p.width() as f32 * scale) / 2.0,
                    r.y + 5.0,
                    scale,
                    None,
                );
            }
        }
        // Palette chips paint over their buttons, the same precedent
        // as the template thumbnails: the button stays the hit target.
        for (i, hex) in self.palette().iter().enumerate() {
            if let Some(n) = view.layout.get(&format!("swatch-{i}")) {
                let chip = Self::solid_chip(hex);
                let r = &n.rect;
                let scale =
                    ((r.w - 8.0) / chip.width() as f32).min((r.h - 8.0) / chip.height() as f32);
                Self::blit(
                    &mut frame,
                    &chip,
                    r.x + (r.w - chip.width() as f32 * scale) / 2.0,
                    r.y + (r.h - chip.height() as f32 * scale) / 2.0,
                    scale,
                    None,
                );
            }
        }
        let key = self.fingerprint();
        if self.artwork_cache.as_ref().map(|(k, _)| k) != Some(&key) {
            self.artwork_cache = Some((key, artwork::render(&self.history.document)?));
        }
        // Clip paper-space paint to the canvas area whenever the
        // zoomed paper overflows it. Fit mode never takes this path,
        // so its frame cost is unchanged.
        let canvas = view.layout.get("canvas").map(|n| n.rect.clone());
        let overflow = canvas.as_ref().is_some_and(|a| {
            view.paper.x < a.x - 0.5
                || view.paper.y < a.y - 0.5
                || view.paper.x + view.paper.w > a.x + a.w + 0.5
                || view.paper.y + view.paper.h > a.y + a.h + 0.5
        });
        let mask = if overflow {
            canvas.and_then(|a| {
                let mut m = tiny_skia::Mask::new(frame.width(), frame.height())?;
                let path = PathBuilder::from_rect(tiny_skia::Rect::from_xywh(a.x, a.y, a.w, a.h)?);
                m.fill_path(
                    &path,
                    tiny_skia::FillRule::Winding,
                    true,
                    Transform::identity(),
                );
                Some(m)
            })
        } else {
            None
        };
        Self::blit(
            &mut frame,
            &self.artwork_cache.as_ref().unwrap().1,
            view.paper.x,
            view.paper.y,
            view.scale,
            mask.as_ref(),
        );
        if self.guides {
            let mut paint = Paint::default();
            paint.set_color_rgba8(120, 220, 200, 90);
            let mut path = PathBuilder::new();
            let x = view.paper.x + view.paper.w / 2.0;
            path.move_to(x, view.paper.y);
            path.line_to(x, view.paper.y + view.paper.h);
            if let Some(path) = path.finish() {
                frame.stroke_path(
                    &path,
                    &paint,
                    &Stroke {
                        width: 1.0,
                        ..Default::default()
                    },
                    Transform::identity(),
                    mask.as_ref(),
                );
            }
        }
        if let Some(e) = self.element() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(120, 220, 200, 255);
            let r = tiny_skia::Rect::from_xywh(
                view.paper.x + e.x * view.scale,
                view.paper.y + e.y * view.scale,
                e.w * view.scale,
                e.h * view.scale,
            );
            if let Some(r) = r {
                let path = PathBuilder::from_rect(r);
                frame.stroke_path(
                    &path,
                    &paint,
                    &Stroke {
                        width: 1.5,
                        ..Default::default()
                    },
                    Transform::identity(),
                    mask.as_ref(),
                );
                for (x, y) in [
                    (r.left(), r.top()),
                    (r.right(), r.top()),
                    (r.left(), r.bottom()),
                    (r.right(), r.bottom()),
                ] {
                    if let Some(r) = tiny_skia::Rect::from_xywh(x - 2.5, y - 2.5, 5.0, 5.0) {
                        frame.fill_rect(r, &paint, Transform::identity(), mask.as_ref());
                    }
                }
            }
        }
        Ok(frame)
    }
    fn solid_chip(hex: &str) -> Pixmap {
        let [r, g, b] = crate::document::rgb(hex).unwrap_or([128, 128, 128]);
        let mut chip = Pixmap::new(8, 8).unwrap();
        chip.fill(tiny_skia::Color::from_rgba8(r, g, b, 255));
        chip
    }
    fn blit(
        dst: &mut Pixmap,
        src: &Pixmap,
        x: f32,
        y: f32,
        scale: f32,
        mask: Option<&tiny_skia::Mask>,
    ) {
        dst.draw_pixmap(
            0,
            0,
            src.as_ref(),
            &PixmapPaint {
                quality: FilterQuality::Bilinear,
                ..Default::default()
            },
            Transform::from_scale(scale, scale).post_translate(x, y),
            mask,
        );
    }
    fn update(&mut self, edit: impl FnOnce(&mut Document)) -> Result<()> {
        self.history.transact(edit)?;
        self.revision += 1;
        Ok(())
    }
    fn selected_update(&mut self, edit: impl FnOnce(&mut Element)) -> Result<()> {
        let id = self
            .selected
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Select an element first"))?;
        self.update(|d| {
            if let Some(e) = d.elements.iter_mut().find(|e| e.id == id) {
                edit(e)
            }
        })
    }
    /// Keep the selection across undo/redo when the element still exists;
    /// otherwise clear it so the inspector never points at a removed element.
    /// Assign the selection. Any new selection re-arms list follow so
    /// the layer list scrolls it into view; manual Up/Down scrolling
    /// disarms follow until the next selection change.
    fn select(&mut self, id: Option<String>) {
        self.selected = id;
        self.follow_selection = true;
    }
    fn retain_selection(&mut self) {
        if let Some(id) = self.selected.clone() {
            if !self.history.document.elements.iter().any(|e| e.id == id) {
                self.selected = None;
            } else {
                self.follow_selection = true;
            }
        }
    }
    pub fn commit_field(&mut self) -> Result<()> {
        let Some(field) = self.active.take() else {
            return Ok(());
        };
        let value = self.composer.text.clone();
        let result = self.set_field(&field, &value);
        self.select_all = false;
        self.revision += 1;
        if let Err(e) = &result {
            self.status = format!("Not applied: {e}");
        }
        result
    }
    fn set_field(&mut self, field: &str, value: &str) -> Result<()> {
        if field == "path" {
            if value.trim().is_empty() {
                bail!("A project path is required")
            }
            self.project = PathBuf::from(value);
            return Ok(());
        }
        if field == "title" {
            return self.update(|d| d.title = value.into());
        }
        if field == "background" {
            return self.update(|d| d.background = value.into());
        }
        if field == "canvas_w" {
            let v = value.parse::<u32>()?;
            return self.update(|d| d.width = v);
        }
        if field == "canvas_h" {
            let v = value.parse::<u32>()?;
            return self.update(|d| d.height = v);
        }
        let n = if matches!(
            field,
            "x" | "y" | "w" | "h" | "size" | "tracking" | "leading"
        ) {
            Some(value.parse::<f32>()?)
        } else {
            None
        };
        let weight = if field == "weight" {
            Some(value.parse::<u16>()?)
        } else {
            None
        };
        if field == "font" && !self.fonts.iter().any(|f| f.eq_ignore_ascii_case(value)) {
            bail!("Font is not installed; use Next available font")
        }
        self.selected_update(|e| match field {
            "x" => e.x = n.unwrap(),
            "y" => e.y = n.unwrap(),
            "w" => e.w = n.unwrap(),
            "h" => e.h = n.unwrap(),
            "color" => e.color = value.into(),
            _ => {
                if let Kind::Text {
                    text,
                    font,
                    weight: w,
                    size,
                    tracking,
                    leading,
                    ..
                } = &mut e.kind
                {
                    match field {
                        "text" => *text = value.into(),
                        "font" => *font = value.into(),
                        "weight" => *w = weight.unwrap(),
                        "size" => *size = n.unwrap(),
                        "tracking" => *tracking = n.unwrap(),
                        "leading" => *leading = n.unwrap(),
                        _ => {}
                    }
                }
            }
        })
    }
    pub fn focus_field(&mut self, field: &str) {
        self.active = Some(field.into());
        self.composer = Composer::new();
        self.composer.text = self.field_value(field);
        self.composer.end();
        self.select_all = true;
        self.focus = Some(format!("field-{field}"));
        self.revision += 1;
        self.status = "Editing. Type to replace; arrows move the caret; Enter applies.".into();
    }
    pub fn insert(&mut self, text: &str) -> Result<()> {
        if self.active.is_none() {
            bail!("No editable field has focus")
        }
        if self.select_all {
            self.composer.text.clear();
            self.composer.caret = 0;
            self.select_all = false;
        }
        self.composer.insert(text);
        self.revision += 1;
        Ok(())
    }
    pub fn press(&mut self, view: &mut View, x: f32, y: f32) -> Result<Option<String>> {
        let result = hamesh_automation::click_human(&mut view.app, &view.layout, x, y);
        let Some(hit) = result.hit_component else {
            return Ok(None);
        };
        let c = view.app.get(&hit).unwrap();
        if !c.enabled || !c.visible {
            return Ok(Some(hit));
        }
        if self.active.is_some() {
            self.commit_field()?;
        }
        self.focus = Some(hit.clone());
        self.revision += 1;
        if let Some(field) = hit.strip_prefix("field-") {
            self.focus_field(field);
            return Ok(Some(hit));
        }
        if hit == "canvas" {
            self.active = None;
            let px = (x - view.paper.x) / view.scale;
            let py = (y - view.paper.y) / view.scale;
            self.select(
                self.history
                    .document
                    .elements
                    .iter()
                    .rev()
                    .find(|e| px >= e.x && py >= e.y && px < e.x + e.w && py < e.y + e.h)
                    .map(|e| e.id.clone()),
            );
            if let Some(e) = self.element() {
                self.drag = Some(Drag {
                    before: self.history.document.clone(),
                    start: (x, y),
                    origin: (e.x, e.y),
                });
            }
            self.status = if self.selected.is_some() {
                "Selected artwork. Drag or use the inspector.".into()
            } else {
                "Selection cleared. Click artwork or a layer.".into()
            };
        } else if let Some(path) = c.props.get("project_path") {
            self.open_path(PathBuf::from(path))?;
        } else if matches!(c.role, Role::Button) {
            self.action(&hit)?;
        }
        Ok(Some(hit))
    }
    pub fn pointer_move(&mut self, x: f32, y: f32, scale: f32) {
        let Some(drag) = self.drag.as_ref() else {
            return;
        };
        let Some(id) = self.selected.as_ref() else {
            return;
        };
        let mut nx = drag.origin.0 + (x - drag.start.0) / scale;
        let mut ny = drag.origin.1 + (y - drag.start.1) / scale;
        if self.guides {
            nx = (nx / 8.0).round() * 8.0;
            ny = (ny / 8.0).round() * 8.0;
        }
        if let Some(e) = self
            .history
            .document
            .elements
            .iter_mut()
            .find(|e| &e.id == id)
        {
            e.x = nx.clamp(0.0, (self.history.document.width as f32 - e.w).max(0.0));
            e.y = ny.clamp(0.0, (self.history.document.height as f32 - e.h).max(0.0));
            self.revision += 1;
        }
    }
    pub fn release(&mut self) -> Result<()> {
        if let Some(drag) = self.drag.take() {
            let after = self.history.document.clone();
            self.history.document = drag.before;
            self.update(|d| *d = after)?;
        }
        Ok(())
    }
    pub fn open_path(&mut self, path: PathBuf) -> Result<()> {
        let doc = Document::load(&path)?;
        self.update(|d| *d = doc)?;
        self.project = path;
        self.saved = self.save_identity();
        self.selected = None;
        self.layer_offset = 0;
        self.pan = (0.0, 0.0);
        self.status = "Project opened.".into();
        Ok(())
    }
    pub fn action(&mut self, id: &str) -> Result<()> {
        if let Some(i) = id.strip_prefix("template-") {
            let doc = Document::template(i.parse()?);
            self.update(|d| *d = doc)?;
            self.select(None);
            self.layer_offset = 0;
            self.pan = (0.0, 0.0);
            self.status = "Starter opened. Undo returns to your previous design.".into();
            return Ok(());
        }
        if let Some(id) = id.strip_prefix("layer-") {
            self.select(Some(id.into()));
            self.status = "Layer selected.".into();
            return Ok(());
        }
        if let Some(w) = id.strip_prefix("weight-") {
            let v: u16 = w.parse()?;
            if !(1..=1000).contains(&v) {
                bail!("Weight {v} is outside 1-1000");
            }
            self.selected_update(|e| {
                if let Kind::Text { weight, .. } = &mut e.kind {
                    *weight = v;
                }
            })?;
            self.status = format!("Weight {v}.");
            return Ok(());
        }
        if let Some(i) = id.strip_prefix("swatch-") {
            if let Some(hex) = self.palette().get(i.parse::<usize>()?).cloned() {
                self.selected_update(|e| e.color = hex.clone())?;
                self.status = format!("Painted {hex}.");
            }
            return Ok(());
        }
        if let Some(tab) = id.strip_prefix("tab-") {
            self.tab = tab.into();
            return Ok(());
        }
        match id {
            "document-settings" => self.tab = "canvas".into(),
            "blank" => {
                self.update(|d| *d = Document::blank())?;
                self.select(None);
                self.layer_offset = 0;
                self.pan = (0.0, 0.0);
            }
            "undo" => {
                self.history.undo();
                self.retain_selection();
                self.status = "Undone.".into();
            }
            "redo" => {
                self.history.redo();
                self.retain_selection();
                self.status = "Redone.".into();
            }
            "guides" => self.guides = !self.guides,
            "zoom" => {
                self.zoom = match self.zoom {
                    Zoom::Fit => Zoom::Selected,
                    Zoom::Selected => Zoom::Actual,
                    Zoom::Actual => Zoom::Fit,
                };
                self.pan = (0.0, 0.0);
                self.status = match self.zoom {
                    Zoom::Fit => "Zoom: whole artboard.",
                    Zoom::Selected => "Zoom: selection fills the canvas.",
                    Zoom::Actual => "Zoom: actual size. Arrow keys pan.",
                }
                .into();
            }
            "library-toggle" => {
                self.library_open = !self.library_open;
                self.status = if self.library_open {
                    "Library shown."
                } else {
                    "Library hidden; layers have the room."
                }
                .into();
            }
            "layers-up" => {
                self.layer_offset = self.layer_offset.saturating_sub(2);
                self.follow_selection = false;
            }
            "layers-down" => {
                self.layer_offset = (self.layer_offset + 2)
                    .min(self.history.document.elements.len().saturating_sub(2));
                self.follow_selection = false;
            }
            "save" | "save-project" => {
                self.history.document.save(&self.project)?;
                self.saved = self.save_identity();
                self.status = format!("Saved {}", self.project.display());
            }
            "open" => {
                self.open_path(self.project.clone())?;
            }
            "export" => {
                let p = self.export_path();
                artwork::export(&self.history.document, &p)?;
                self.status = format!(
                    "Exported {} x {} PNG: {}",
                    self.history.document.width,
                    self.history.document.height,
                    p.display()
                );
            }
            "add-text" | "add-rect" | "add-ellipse" => {
                let mut n = self.history.document.elements.len() + 1;
                while self
                    .history
                    .document
                    .elements
                    .iter()
                    .any(|e| e.id == format!("element-{n}"))
                {
                    n += 1
                }
                let eid = format!("element-{n}");
                let copy = eid.clone();
                let kind = match id {
                    "add-text" => Kind::Text {
                        text: "NEW SIGNAL".into(),
                        font: self
                            .fonts
                            .iter()
                            .find(|f| f.as_str() == "Arial")
                            .cloned()
                            .unwrap_or_else(|| "Segoe UI".into()),
                        weight: 700,
                        size: 64.0,
                        tracking: 0.0,
                        leading: 1.1,
                        align: Alignment::Left,
                    },
                    "add-rect" => Kind::Rectangle,
                    _ => Kind::Ellipse,
                };
                let color = if id == "add-text" {
                    "#EFE9D7"
                } else {
                    "#D6F04A"
                };
                self.update(|d| {
                    d.elements.push(Element {
                        id: eid,
                        name: if id == "add-text" {
                            "New text"
                        } else {
                            "New shape"
                        }
                        .into(),
                        x: 60.0,
                        y: 60.0,
                        w: 400.0,
                        h: 120.0,
                        color: color.into(),
                        kind,
                    })
                })?;
                self.select(Some(copy));
                self.tab = "type".into();
                self.layer_offset = 0;
            }
            "delete" => {
                if let Some(id) = self.selected.take() {
                    self.update(|d| d.elements.retain(|e| e.id != id))?;
                }
            }
            "duplicate" => {
                if let Some(e) = self.element() {
                    let mut copy = e.clone();
                    let mut n = self.history.document.elements.len() + 1;
                    while self
                        .history
                        .document
                        .elements
                        .iter()
                        .any(|e| e.id == format!("copy-{n}"))
                    {
                        n += 1
                    }
                    copy.id = format!("copy-{n}");
                    copy.name = format!("{} copy", copy.name);
                    copy.x = (copy.x + 16.0)
                        .min(self.history.document.width as f32 - copy.w)
                        .max(0.0);
                    copy.y = (copy.y + 16.0)
                        .min(self.history.document.height as f32 - copy.h)
                        .max(0.0);
                    let id = copy.id.clone();
                    self.update(|d| d.elements.push(copy))?;
                    self.select(Some(id));
                }
            }
            "front" | "back" => {
                if let Some(selected) = self.selected.clone() {
                    let to_back = id == "back";
                    self.update(|d| {
                        if let Some(i) = d.elements.iter().position(|e| e.id == selected) {
                            let e = d.elements.remove(i);
                            if to_back {
                                d.elements.insert(0, e)
                            } else {
                                d.elements.push(e)
                            }
                        }
                    })?;
                }
            }
            "align-left" | "align-center" | "align-right" => {
                let a = match id {
                    "align-center" => Alignment::Center,
                    "align-right" => Alignment::Right,
                    _ => Alignment::Left,
                };
                self.selected_update(|e| {
                    if let Kind::Text { align, .. } = &mut e.kind {
                        *align = a
                    }
                })?;
            }
            "next-font" => {
                let current = self.field_value("font");
                let i = self.fonts.iter().position(|f| f == &current).unwrap_or(0);
                if !self.fonts.is_empty() {
                    let next = self.fonts[(i + 1) % self.fonts.len()].clone();
                    self.set_field("font", &next)?;
                }
            }
            p if p.starts_with("position-") => {
                let (w, h) = (
                    self.history.document.width as f32,
                    self.history.document.height as f32,
                );
                self.selected_update(|e| match p {
                    "position-left" => e.x = 0.0,
                    "position-center" => e.x = (w - e.w) / 2.0,
                    "position-right" => e.x = w - e.w,
                    "position-top" => e.y = 0.0,
                    "position-middle" => e.y = (h - e.h) / 2.0,
                    "position-bottom" => e.y = h - e.h,
                    _ => {}
                })?;
            }
            _ => bail!("Unknown action: {id}"),
        }
        self.revision += 1;
        Ok(())
    }
    pub fn key(&mut self, view: &View, key: &str) -> Result<()> {
        if key == "Tab" || key == "ShiftTab" {
            if self.active.is_some() {
                self.commit_field()?;
            }
            let mut nodes = view
                .scene
                .nodes
                .iter()
                .filter(|n| n.visible && n.enabled && (n.role == "Button" || n.role == "TextInput"))
                .collect::<Vec<_>>();
            nodes.sort_by(|a, b| {
                a.rect
                    .y
                    .total_cmp(&b.rect.y)
                    .then(a.rect.x.total_cmp(&b.rect.x))
            });
            if !nodes.is_empty() {
                let i = nodes
                    .iter()
                    .position(|n| Some(n.id.as_str()) == self.focus.as_deref());
                let next = if key == "Tab" {
                    i.map(|i| (i + 1) % nodes.len()).unwrap_or(0)
                } else {
                    i.map(|i| (i + nodes.len() - 1) % nodes.len()).unwrap_or(0)
                };
                let id = nodes[next].id.clone();
                self.focus = Some(id.clone());
                if let Some(f) = id.strip_prefix("field-") {
                    self.focus_field(f)
                }
            }
        } else if self.active.is_some() {
            match key {
                "Enter" => {
                    self.commit_field()?;
                    self.status = "Property applied.".into();
                }
                "Escape" => {
                    self.active = None;
                    self.composer.clear_preedit();
                    self.status = "Edit cancelled.".into();
                }
                "SelectAll" => self.select_all = true,
                "Backspace" | "Delete" => {
                    if self.select_all {
                        self.composer.text.clear();
                        self.composer.caret = 0;
                        self.select_all = false;
                    } else if key == "Backspace" {
                        self.composer.backspace()
                    } else {
                        self.composer.delete_forward()
                    }
                }
                "Left" => {
                    self.select_all = false;
                    self.composer.move_left()
                }
                "Right" => {
                    self.select_all = false;
                    self.composer.move_right()
                }
                "Home" => {
                    self.select_all = false;
                    self.composer.home()
                }
                "End" => {
                    self.select_all = false;
                    self.composer.end()
                }
                _ => {
                    if let Some(c) = key.strip_prefix("Char:") {
                        self.insert(c)?;
                    }
                }
            }
        } else {
            match key {
                "Enter" => {
                    if let Some(id) = self.focus.clone() {
                        if let Some(f) = id.strip_prefix("field-") {
                            self.focus_field(f)
                        } else {
                            self.action(&id)?
                        }
                    }
                }
                "Escape" => {
                    self.select(None);
                    self.focus = None;
                }
                "Delete" => self.action("delete")?,
                "Left" | "Right" | "Up" | "Down" if self.selected.is_some() => {
                    let (w, h) = (
                        self.history.document.width as f32,
                        self.history.document.height as f32,
                    );
                    self.selected_update(|e| match key {
                        "Left" => e.x = (e.x - 1.0).max(0.0),
                        "Right" => e.x = (e.x + 1.0).min(w - e.w),
                        "Up" => e.y = (e.y - 1.0).max(0.0),
                        "Down" => e.y = (e.y + 1.0).min(h - e.h),
                        _ => {}
                    })?;
                }
                "Left" | "Right" | "Up" | "Down"
                    if self.selected.is_none() && !matches!(self.zoom, Zoom::Fit) =>
                {
                    // Pan the zoomed canvas with the same arrow
                    // semantics as nudging: Right moves the paper right.
                    let step = 20.0;
                    let (dx, dy) = match key {
                        "Left" => (-step, 0.0),
                        "Right" => (step, 0.0),
                        "Up" => (0.0, -step),
                        _ => (0.0, step),
                    };
                    self.pan = clamp_pan(
                        (self.pan.0 + dx, self.pan.1 + dy),
                        view.paper.w,
                        view.paper.h,
                    );
                }
                _ => {}
            }
        }
        self.revision += 1;
        Ok(())
    }
}
