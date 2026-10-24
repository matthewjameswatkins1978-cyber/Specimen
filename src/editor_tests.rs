//! Controller/layout regression evidence. These are in-process controller
//! checks, not the separate-process Eye/Hand acceptance demonstration.
use crate::{
    document::{Document, Element, Kind},
    editor::{library_capacity, Editor},
};
use hamesh_core::Role;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    directory: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("work/controller-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&directory).unwrap();
        Self { directory }
    }
    fn editor(&self, name: &str) -> Editor {
        Editor::new(self.directory.join(name)).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("work/controller-tests");
        assert!(self.directory.starts_with(root));
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn workspace_controls_are_reachable_and_unclipped_in_all_inspector_states() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("layout.specimen.json");
    let mut failures = Vec::new();
    for (width, height) in [(1100, 820), (1440, 980), (1800, 1100)] {
        for selection in [None, Some("title"), Some("orbit-red")] {
            editor.selected = selection.map(str::to_string);
            for tab in ["type", "layout", "canvas"] {
                editor.tab = tab.into();
                let view = editor.view(width, height);
                let context = format!("{width}x{height} selection={selection:?} tab={tab}");
                let mut incoming = HashMap::<&str, usize>::new();
                for (parent_id, parent) in &view.app.components {
                    let mut children = HashSet::new();
                    for child in &parent.children {
                        assert!(
                            children.insert(child.0.as_str()),
                            "Duplicate child {} under {parent_id}",
                            child.0
                        );
                        let child_node = view.app.get(&child.0).expect("Tree child must exist");
                        assert_eq!(
                            child_node.parent.as_ref().map(|p| p.0.as_str()),
                            Some(parent_id.as_str()),
                            "Parent identity mismatch for {}",
                            child.0
                        );
                        *incoming.entry(child.0.as_str()).or_default() += 1;
                    }
                }
                for (id, component) in &view.app.components {
                    assert_eq!(
                        incoming.get(id.as_str()).copied().unwrap_or(0),
                        usize::from(*id != view.app.root.0),
                        "Component must have exactly one parent: {id}"
                    );
                    if !matches!(component.role, Role::Button | Role::TextInput) {
                        continue;
                    }
                    let node = view
                        .layout
                        .get(id)
                        .expect("Every interactive component needs Eye geometry");
                    let r = &node.rect;
                    let centre = (r.x + r.w / 2., r.y + r.h / 2.);
                    let hit = hamesh_layout::hit_test(&view.app, &view.layout, centre.0, centre.1);
                    if hit.as_deref() != Some(id) {
                        failures.push(format!("{context}: {id} centre {centre:?} hits {hit:?}"));
                    }
                    if !component.enabled {
                        continue;
                    }
                    if !node.effectively_visible
                        || node.clipped_by_parent
                        || r.x < -0.5
                        || r.y < -0.5
                        || r.x + r.w > width as f32 + 0.5
                        || r.y + r.h > height as f32 + 0.5
                    {
                        failures.push(format!(
                            "{context}: {id} invisible/clipped/outside viewport: {r:?}, clipped={}",
                            node.clipped_by_parent
                        ));
                    }
                    let minimum_height = if component.role == Role::TextInput {
                        30.
                    } else {
                        24.
                    };
                    if r.w < 28. || r.h < minimum_height {
                        failures.push(format!("{context}: {id} below usable control size: {r:?}"));
                    }
                }
                assert!(view.paper.w > 0. && view.paper.h > 0.);
                assert!(
                    view.paper.x >= 0.
                        && view.paper.y >= 0.
                        && view.paper.x + view.paper.w <= width as f32
                        && view.paper.y + view.paper.h <= height as f32,
                    "{context}: Artwork must fit the viewport"
                );
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} layout failures:\n{}",
        failures.len(),
        failures.into_iter().take(60).collect::<Vec<_>>().join("\n")
    );
}

/// QBC Phase 1 measurement: report-only viewport survey. Prints panel
/// widths, canvas scale, text overflow and undersized controls for each
/// viewport/selection/tab combination. Fails nothing; the numbers decide
/// what Phase 2 contracts must cover.
#[test]
fn qbc_viewport_measurement() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("qbc-measure.specimen.json");
    for (width, height) in [(1440, 980), (1100, 820), (1920, 1080), (800, 600)] {
        for selection in [None, Some("title"), Some("orbit-red")] {
            editor.selected = selection.map(str::to_string);
            for tab in ["type", "layout", "canvas"] {
                editor.tab = tab.into();
                let view = editor.view(width, height);
                let w = |id: &str| {
                    view.layout
                        .get(id)
                        .map(|n| (n.rect.x, n.rect.w))
                        .unwrap_or((-1.0, -1.0))
                };
                let (lx, lw) = w("library");
                let (ix, iw) = w("inspector");
                let mut overflow = Vec::new();
                let mut tiny = Vec::new();
                for n in &view.scene.nodes {
                    if n.ghost {
                        continue;
                    }
                    if n.advance > 0.0
                        && n.lines == 1
                        && n.origin[0] + n.advance > n.rect.x + n.content[2] + 1.0
                    {
                        overflow.push(format!(
                            "{} adv={:.1} content_w={:.1}",
                            n.id,
                            n.advance,
                            n.content[2] - n.content[0]
                        ));
                    }
                    if n.rect.w < 28.0 && n.rect.h > 0.0 && !n.text.is_empty() {
                        tiny.push(n.id.clone());
                    }
                }
                println!(
                    "[qbc] {width}x{height} sel={selection:?} tab={tab}: \
                     library x={lx:.0} w={lw:.0}, inspector x={ix:.0} w={iw:.0}, \
                     canvas scale={:.3}, paper={:.0}x{:.0} at ({:.0},{:.0}), \
                     overflow={overflow:?}, narrow_text={tiny:?}",
                    view.scale, view.paper.w, view.paper.h, view.paper.x, view.paper.y,
                );
            }
        }
    }
}

fn enter_field(editor: &mut Editor, field: &str, value: &str) -> anyhow::Result<()> {
    editor.focus_field(field);
    editor.insert(value)?;
    let view = editor.view(1440, 980);
    editor.key(&view, "Enter")
}

#[test]
fn property_edits_undo_redo_and_invalid_commits_preserve_the_canonical_document() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("properties.specimen.json");
    editor.selected = Some("title".into());
    let original = editor.history.document.clone();
    enter_field(&mut editor, "text", "NIGHT SIGNAL").unwrap();
    let edited = editor.history.document.clone();
    assert_ne!(original, edited);
    assert!(matches!(&editor.element().unwrap().kind,Kind::Text{text,..} if text=="NIGHT SIGNAL"));
    for (field, value) in [
        ("w", "0"),
        ("x", "NaN"),
        ("color", "#NOPE!!"),
        ("font", "Specimen missing-font regression fixture"),
    ] {
        assert!(
            enter_field(&mut editor, field, value).is_err(),
            "Invalid {field} should be rejected"
        );
        assert_eq!(
            editor.history.document, edited,
            "Invalid commit must be atomic"
        );
        assert!(editor.status.starts_with("Not applied:"));
    }
    editor.action("undo").unwrap();
    assert_eq!(editor.history.document, original);
    assert!(
        !editor.history.can_undo(),
        "Invalid commits must not add undo entries"
    );
    editor.action("redo").unwrap();
    assert_eq!(editor.history.document, edited);
}

#[test]
fn a_drag_routes_through_hamesh_hit_testing_and_creates_one_history_step() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("drag.specimen.json");
    editor.guides = false;
    let original = editor.history.document.clone();
    let title = original.elements.iter().find(|e| e.id == "title").unwrap();
    let origin = (title.x, title.y);
    let mut view = editor.view(1440, 980);
    let start = (
        view.paper.x + (title.x + 100.) * view.scale,
        view.paper.y + (title.y + 50.) * view.scale,
    );
    assert_eq!(
        editor
            .press(&mut view, start.0, start.1)
            .unwrap()
            .as_deref(),
        Some("canvas")
    );
    assert_eq!(editor.selected.as_deref(), Some("title"));
    for (dx, dy) in [(8., 10.), (16., 20.), (24., 32.)] {
        editor.pointer_move(
            start.0 + dx * view.scale,
            start.1 + dy * view.scale,
            view.scale,
        );
    }
    editor.release().unwrap();
    let after = editor.history.document.clone();
    let moved = after.elements.iter().find(|e| e.id == "title").unwrap();
    assert!((moved.x - origin.0 - 24.).abs() < 0.01 && (moved.y - origin.1 - 32.).abs() < 0.01);
    assert_ne!(after, original);
    editor.action("undo").unwrap();
    assert_eq!(editor.history.document, original);
    assert!(
        !editor.history.can_undo(),
        "Pointer moves must coalesce into one undo transaction"
    );
    editor.action("redo").unwrap();
    assert_eq!(editor.history.document, after);
}

#[test]
fn stacking_and_duplication_preserve_stable_ids_and_are_reversible() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("stacking.specimen.json");
    let original = editor.history.document.clone();
    editor.selected = Some("orbit-red".into());
    editor.action("front").unwrap();
    assert_eq!(
        editor.history.document.elements.last().unwrap().id,
        "orbit-red"
    );
    editor.action("undo").unwrap();
    assert_eq!(editor.history.document, original);
    editor.action("redo").unwrap();
    editor.selected = Some("orbit-red".into());
    editor.action("back").unwrap();
    assert_eq!(
        editor.history.document.elements.first().unwrap().id,
        "orbit-red"
    );
    editor.action("duplicate").unwrap();
    let duplicate = editor.selected.clone().unwrap();
    assert_ne!(duplicate, "orbit-red");
    let ids: HashSet<_> = editor
        .history
        .document
        .elements
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(ids.len(), editor.history.document.elements.len());
    editor.history.document.validate().unwrap();
    editor.action("undo").unwrap();
    assert!(!editor
        .history
        .document
        .elements
        .iter()
        .any(|e| e.id == duplicate));
}

#[test]
fn export_never_overwrites_a_project_even_when_its_name_ends_in_png() {
    for filename in ["project.png", "project.specimen.json"] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor(filename);
        editor.action("save").unwrap();
        let project_bytes = std::fs::read(&editor.project).unwrap();
        let export_path = editor.export_path();
        assert_ne!(
            export_path, editor.project,
            "Project and export destinations must differ"
        );
        let expected = if filename == "project.png" {
            "project.png.png"
        } else {
            "project.png"
        };
        assert_eq!(export_path.file_name().unwrap(), expected);
        editor.action("export").unwrap();
        assert_eq!(
            std::fs::read(&editor.project).unwrap(),
            project_bytes,
            "Export must preserve project bytes"
        );
        assert_eq!(
            Document::load(&editor.project).unwrap(),
            editor.history.document
        );
        let png = tiny_skia::Pixmap::load_png(export_path).unwrap();
        assert_eq!(
            (png.width(), png.height()),
            (
                editor.history.document.width,
                editor.history.document.height
            )
        );
    }
}

#[test]
fn export_preserves_other_projects_opaque_files_and_directories() {
    for original in [
        serde_json::to_vec_pretty(&Document::template(1)).unwrap(),
        b"An unrelated opaque file, not disposable artwork.".to_vec(),
    ] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor("project.specimen.json");
        let destination = editor.export_path();
        std::fs::write(&destination, &original).unwrap();
        let error = editor.action("export").unwrap_err().to_string();
        assert!(error.contains("non-PNG"), "{error}");
        assert_eq!(std::fs::read(&destination).unwrap(), original);
        assert_eq!(
            std::fs::read_dir(&fixture.directory).unwrap().count(),
            1,
            "Rejected export must not leave temporary files"
        );
    }
    let fixture = Fixture::new();
    let mut editor = fixture.editor("project.specimen.json");
    let destination = editor.export_path();
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("keep.txt"), b"preserve me").unwrap();
    let error = editor.action("export").unwrap_err().to_string();
    assert!(error.contains("non-file"), "{error}");
    assert_eq!(
        std::fs::read(destination.join("keep.txt")).unwrap(),
        b"preserve me"
    );
}

#[test]
fn existing_png_exports_update_atomically_and_match_canonical_paint() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("replace.specimen.json");
    editor.action("export").unwrap();
    let destination = editor.export_path();
    let before = std::fs::read(&destination).unwrap();
    editor
        .history
        .transact(|d| d.background = "#203040".into())
        .unwrap();
    let expected = crate::artwork::render(&editor.history.document).unwrap();
    editor.action("export").unwrap();
    let updated = std::fs::read(&destination).unwrap();
    assert_ne!(before, updated);
    let decoded = tiny_skia::Pixmap::decode_png(&updated).unwrap();
    assert_eq!(decoded.data(), expected.data());
    assert_eq!(
        std::fs::read_dir(&fixture.directory).unwrap().count(),
        1,
        "Successful replacement must not leave a staged file"
    );
    editor.history.document.width = 0;
    assert!(editor.action("export").is_err());
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        updated,
        "A failed export must preserve the previous complete PNG"
    );
}

#[test]
fn changing_the_project_path_marks_identical_artwork_unsaved() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("original.specimen.json");
    editor.action("save").unwrap();
    assert!(!editor.dirty());
    let old_bytes = std::fs::read(&editor.project).unwrap();
    let old_path = editor.project.clone();
    let next = fixture.directory.join("renamed.specimen.json");
    enter_field(&mut editor, "path", next.to_str().unwrap()).unwrap();
    assert!(
        editor.dirty(),
        "A new destination needs a save even when the design is unchanged"
    );
    assert!(!next.exists());
    editor.action("save").unwrap();
    assert!(!editor.dirty());
    assert_eq!(std::fs::read(next).unwrap(), old_bytes);
    assert_eq!(std::fs::read(old_path).unwrap(), old_bytes);
}

/// Undo/redo keeps a selection that still exists, clears one that is
/// gone, and reports an empty-canvas click honestly.
#[test]
fn undo_redo_retains_live_selection_only() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("undo-select.specimen.json");
    editor.selected = Some("title".into());
    enter_field(&mut editor, "x", "64").unwrap();
    assert!(editor.history.can_undo());
    editor.action("undo").unwrap();
    assert_eq!(editor.selected.as_deref(), Some("title"));
    editor.action("redo").unwrap();
    assert_eq!(editor.selected.as_deref(), Some("title"));
    // A template switch drops the old selection; undo cannot revive it.
    editor.selected = Some("title".into());
    editor.action("template-1").unwrap();
    assert_eq!(editor.selected, None);
    editor.action("undo").unwrap();
    assert_eq!(editor.selected, None);
    // Clicking empty canvas clears the selection and says so.
    editor.selected = Some("title".into());
    let mut view = editor.view(1440, 980);
    let empty = (view.paper.x + view.paper.w + 60.0, view.paper.y + 40.0);
    assert_eq!(
        editor
            .press(&mut view, empty.0, empty.1)
            .unwrap()
            .as_deref(),
        Some("canvas")
    );
    assert_eq!(editor.selected, None);
    assert!(editor.status.contains("Selection cleared"));
}

/// Layers follow: the selected element scrolls into view, the list
/// states its visible window, and manual Up/Down scrolling holds its
/// position until the selection changes again.
#[test]
fn layers_follow_selection_and_state_their_window() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("layers-follow.specimen.json");
    // Template 0 carries 17 elements with `title` third-oldest
    // (reverse-position 14); at 1440x820 the fitted window holds
    // 3 layer rows (capacity contract above), so the window
    // covering the selection reads 13-15.
    editor.action("layer-title").unwrap();
    let view = editor.view(1440, 820);
    let node = view
        .layout
        .get("layer-title")
        .expect("selected layer must scroll into view");
    assert!(!node.clipped_by_parent);
    let text = view
        .app
        .get("layers-position")
        .and_then(|c| c.props.get("text"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(text, "Showing 13-15 of 17");
    // Manual scroll holds: the selection may leave the window, and
    // the indicator keeps stating the visible range honestly.
    editor.action("layers-down").unwrap();
    let view = editor.view(1440, 820);
    assert!(view.layout.get("layer-title").is_none());
    let text = view
        .app
        .get("layers-position")
        .and_then(|c| c.props.get("text"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(text, "Showing 3-5 of 17");
    // Reselecting re-arms follow and brings the layer back.
    editor.action("layer-title").unwrap();
    let view = editor.view(1440, 820);
    assert!(view.layout.get("layer-title").is_some());
}

/// Inspector levels: canvas properties stay off the element tabs,
/// numeric fields state their units, and text alignment shows state.
#[test]
fn inspector_separates_levels_and_states_alignment() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("inspector-levels.specimen.json");
    editor.selected = Some("title".into());
    // Paper is canvas-level: absent with an element on the Type
    // tab, present on the Canvas tab.
    editor.tab = "type".into();
    let view = editor.view(1440, 980);
    assert!(view.layout.get("field-background").is_none());
    assert!(view.layout.get("field-color").is_some());
    editor.tab = "canvas".into();
    let view = editor.view(1440, 980);
    assert!(view.layout.get("field-background").is_some());
    // Numeric fields state their units in the label.
    editor.tab = "type".into();
    let view = editor.view(1440, 980);
    for (id, unit) in [
        ("label-size", "(px)"),
        ("label-tracking", "(px)"),
        ("label-leading", "(x)"),
    ] {
        let text = view
            .app
            .get(id)
            .and_then(|c| c.props.get("text"))
            .cloned()
            .unwrap_or_default();
        assert!(text.contains(unit), "{id} label {text:?} lacks {unit}");
    }
    // The active text alignment reads selected; the others do not.
    let variant = |id: &str| {
        view.app
            .get(id)
            .and_then(|c| c.props.get("variant"))
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(variant("align-left"), "primary");
    assert_eq!(variant("align-center"), "ghost");
    editor.action("align-center").unwrap();
    let view = editor.view(1440, 980);
    let variant = |id: &str| {
        view.app
            .get(id)
            .and_then(|c| c.props.get("variant"))
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(variant("align-center"), "primary");
    assert_eq!(variant("align-left"), "ghost");
}

/// Toolbar groups and live zoom: ghost struts separate the draw,
/// history and view groups, and the canvas heading quantifies
/// the fit zoom it names.
#[test]
fn toolbar_groups_and_zoom_readout() {
    let fixture = Fixture::new();
    let editor = fixture.editor("toolbar-zoom.specimen.json");
    let view = editor.view(1440, 980);
    // The mode label is gone; fixed air separates the groups.
    assert!(view.layout.get("toolbar-name").is_none());
    for id in ["tools-gap", "history-gap"] {
        let node = view.layout.get(id).expect("group strut holds layout room");
        assert!((node.rect.w - 16.0).abs() < 0.5);
        let ghost = view
            .app
            .get(id)
            .and_then(|c| c.props.get("ghost"))
            .cloned()
            .unwrap_or_default();
        assert_eq!(ghost, "true");
    }
    // The heading states artwork size and the live fit zoom.
    let text = view
        .app
        .get("canvas-size")
        .and_then(|c| c.props.get("text"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        text,
        format!("{} x {} px · {}%", 900, 1200, (view.scale * 100.0).round())
    );
}

/// Document palette: swatches show the artwork's own colours and
/// paint the selection through real actions, down to the pixels.
#[test]
fn colour_palette_applies_document_colours() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("palette.specimen.json");
    editor.selected = Some("subtitle".into());
    let palette = editor.palette();
    assert!(!palette.is_empty() && palette.len() <= 6);
    assert!(palette.iter().any(|c| c.eq_ignore_ascii_case("#EEE9D9")));
    let before = editor.element().unwrap().color.clone();
    editor.action("swatch-0").unwrap();
    assert_eq!(editor.element().unwrap().color, palette[0]);
    editor.action("undo").unwrap();
    assert_eq!(editor.element().unwrap().color, before);
    // The chip paint reaches the frame: the swatch centre pixel
    // carries the palette colour.
    let view = editor.view(1440, 980);
    let pix = editor.render(&view).unwrap();
    let node = view.layout.get("swatch-0").expect("first swatch");
    let (cx, cy) = (
        (node.rect.x + node.rect.w / 2.0) as u32,
        (node.rect.y + node.rect.h / 2.0) as u32,
    );
    let px = pix.pixel(cx, cy).expect("swatch centre inside frame");
    let [r, g, b] = crate::document::rgb(&palette[0]).unwrap();
    assert_eq!((px.red(), px.green(), px.blue()), (r, g, b));
}

/// Selection zoom: the element centers in the canvas at the fitted
/// scale, the heading names it, empty selection falls back to Fit,
/// and the zoomed frame still renders clipped to the canvas.
#[test]
fn zoom_focus_centers_selection_and_clips_to_canvas() {
    use crate::editor::selection_scale;
    // Pure scale math, including the tiny-element clamp.
    let tiny = Element::text("t", "T", 0., 0., 2., 2., 16., "#FFFFFF");
    assert_eq!(selection_scale(940.0, 770.0, &tiny), 8.0);
    let big = Element::text("b", "B", 0., 0., 200., 100., 16., "#FFFFFF");
    assert!((selection_scale(940.0, 770.0, &big) - 4.5).abs() < 0.01);
    let fixture = Fixture::new();
    let mut editor = fixture.editor("zoom-focus.specimen.json");
    editor.selected = Some("channel".into());
    editor.action("zoom").unwrap();
    let view = editor.view(1440, 980);
    let area = view.layout.get("canvas").unwrap().rect.clone();
    let e = editor
        .history
        .document
        .elements
        .iter()
        .find(|x| x.id == "channel")
        .unwrap()
        .clone();
    let expected = ((area.w - 40.0) / e.w)
        .min((area.h - 32.0) / e.h)
        .clamp(0.01, 8.0);
    assert!((view.scale - expected).abs() < 0.01);
    let (ecx, ecy) = (e.x + e.w / 2.0, e.y + e.h / 2.0);
    assert!((view.paper.x + ecx * view.scale - (area.x + area.w / 2.0)).abs() < 1.0);
    assert!((view.paper.y + ecy * view.scale - (area.y + area.h / 2.0)).abs() < 1.0);
    let title = view
        .app
        .get("canvas-title")
        .and_then(|c| c.props.get("text"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(title, "SELECTION / FIT");
    // The zoomed frame renders clipped to the canvas size.
    let pix = editor.render(&view).unwrap();
    assert_eq!((pix.width(), pix.height()), (1440, 980));
    // Nothing selected: honest Fit fallback at the same window.
    editor.selected = None;
    let view = editor.view(1440, 980);
    let fit = ((area.w - 40.0) / 900.0)
        .min((area.h - 32.0) / 1200.0)
        .max(0.01);
    assert!((view.scale - fit).abs() < 0.01);
}

/// Actual-size zoom: true scale centered on the canvas, arrow-key
/// panning that can never lose the artwork, and a three-way toggle.
#[test]
fn zoom_actual_centers_true_size_and_pans_with_arrows() {
    use crate::editor::clamp_pan;
    // The canvas center never leaves the paper.
    assert_eq!(clamp_pan((500.0, -700.0), 900.0, 1200.0), (450.0, -600.0));
    assert_eq!(clamp_pan((10.0, 20.0), 900.0, 1200.0), (10.0, 20.0));
    let fixture = Fixture::new();
    let mut editor = fixture.editor("zoom-actual.specimen.json");
    editor.selected = Some("channel".into());
    editor.action("zoom").unwrap();
    editor.action("zoom").unwrap();
    let view = editor.view(1440, 980);
    assert_eq!(view.scale, 1.0);
    let area = view.layout.get("canvas").unwrap().rect.clone();
    let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
    assert!((view.paper.x + 900.0 / 2.0 - cx).abs() < 1.0);
    assert!((view.paper.y + 1200.0 / 2.0 - cy).abs() < 1.0);
    let title = view
        .app
        .get("canvas-title")
        .and_then(|c| c.props.get("text"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(title, "ACTUAL SIZE");
    let zoom = view
        .app
        .get("zoom")
        .and_then(|c| c.props.get("text"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(zoom, "Zoom: Actual");
    // Arrows pan with nothing selected; the artwork stays reachable.
    editor.selected = None;
    let view = editor.view(1440, 980);
    editor.key(&view, "Right").unwrap();
    let moved = editor.view(1440, 980);
    assert!((moved.paper.x - view.paper.x - 20.0).abs() < 0.01);
    for _ in 0..100 {
        let v = editor.view(1440, 980);
        editor.key(&v, "Right").unwrap();
    }
    let view = editor.view(1440, 980);
    assert!((view.paper.x - cx).abs() < 1.0);
    // The zoomed frame still renders at full size, clipped to canvas.
    let pix = editor.render(&view).unwrap();
    assert_eq!((pix.width(), pix.height()), (1440, 980));
    // Toggling on returns to Fit.
    editor.action("zoom").unwrap();
    let view = editor.view(1440, 980);
    let fit = ((area.w - 40.0) / 900.0)
        .min((area.h - 32.0) / 1200.0)
        .max(0.01);
    assert!((view.scale - fit).abs() < 0.01);
}

/// Weight quick-set and heading: common weights are one tap with
/// visible state, and the heading names kind and content.
#[test]
fn weight_quickset_and_heading_state_the_selection() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("weight.specimen.json");
    editor.selected = Some("title".into());
    editor.tab = "type".into();
    let view = editor.view(1440, 980);
    // The heading names kind and content, not the internal id.
    let heading = view
        .app
        .get("inspector-title")
        .and_then(|c| c.props.get("text"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(heading, "Text · AFTER");
    let variant = |id: &str| {
        view.app
            .get(id)
            .and_then(|c| c.props.get("variant"))
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(variant("weight-700"), "primary");
    assert_eq!(variant("weight-400"), "ghost");
    editor.action("weight-400").unwrap();
    assert!(matches!(
        editor.element().map(|e| &e.kind),
        Some(Kind::Text { weight: 400, .. })
    ));
    editor.action("undo").unwrap();
    assert!(matches!(
        editor.element().map(|e| &e.kind),
        Some(Kind::Text { weight: 700, .. })
    ));
    // Shapes get no weight row.
    editor.selected = Some("orbit-lime".into());
    let view = editor.view(1440, 980);
    assert!(view.layout.get("weight-400").is_none());
}

/// Collapsible library: hiding the template cards hands the room
/// to layers, and showing them restores the browser.
#[test]
fn library_collapse_hands_room_to_layers() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("library-collapse.specimen.json");
    // Expanded by default: the browser is present.
    let view = editor.view(1440, 980);
    assert!(view.layout.get("template-0").is_some());
    // Collapse: cards go away, rows grow, controls stay reachable.
    editor.action("library-toggle").unwrap();
    let view = editor.view(1440, 820);
    assert!(view.layout.get("template-0").is_none());
    assert!(view.layout.get("blank").is_some());
    let rows = view
        .app
        .components
        .keys()
        .filter(|id| id.starts_with("layer-"))
        .count();
    assert_eq!(rows, 8);
    let toggle = view
        .app
        .get("library-toggle")
        .and_then(|c| c.props.get("text"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(toggle, "+");
    for (id, component) in &view.app.components {
        if !matches!(component.role, Role::Button | Role::TextInput) {
            continue;
        }
        if !component.enabled {
            continue;
        }
        let node = view.layout.get(id).expect("Eye geometry");
        let r = &node.rect;
        assert!(
            node.effectively_visible
                && !node.clipped_by_parent
                && r.x >= -0.5
                && r.y >= -0.5
                && r.x + r.w <= 1440.0 + 0.5
                && r.y + r.h <= 820.0 + 0.5,
            "{id} clipped/outside: {r:?}"
        );
    }
    // Expand again: the browser returns.
    editor.action("library-toggle").unwrap();
    let view = editor.view(1440, 820);
    assert!(view.layout.get("template-0").is_some());
}

/// Saved state in the header: the document title names unsaved work.
#[test]
fn header_names_unsaved_work() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("header-dirty.specimen.json");
    let title_of = |editor: &Editor| {
        editor
            .view(1440, 980)
            .app
            .get("project-title")
            .and_then(|c| c.props.get("text"))
            .cloned()
            .unwrap_or_default()
    };
    // Fresh, never-saved documents read as unsaved.
    assert!(title_of(&editor).ends_with(" · unsaved"));
    editor.action("save").unwrap();
    assert_eq!(title_of(&editor), editor.history.document.title);
    // Editing dirties it again.
    editor.selected = Some("title".into());
    enter_field(&mut editor, "x", "64").unwrap();
    assert!(title_of(&editor).ends_with(" · unsaved"));
}

/// Library capacity contract: fitted (layer rows, saved rows) for
/// measured panel chrome. Short panels shed saved rows first and
/// always keep the autosave recovery entry visible.
#[test]
fn library_capacity_matches_measured_chrome() {
    assert_eq!(library_capacity(840.0, 2, true, true), (3, 2));
    assert_eq!(library_capacity(820.0, 1, true, true), (3, 1));
    assert_eq!(library_capacity(680.0, 2, true, true), (1, 0));
    assert_eq!(library_capacity(940.0, 2, true, true), (5, 2));
    assert_eq!(library_capacity(680.0, 0, false, true), (3, 0));
    assert_eq!(library_capacity(680.0, 2, true, false), (8, 2));
    assert_eq!(library_capacity(840.0, 0, false, false), (8, 0));
}

/// Seeded-workspace regression for the clipped recovery entry: with
/// three saved projects and two autosave files on disk, every enabled
/// control must stay unclipped inside the viewport at supported sizes,
/// the latest recovery must stay reachable, and the visible library
/// rows must match the fitted capacity.
#[test]
fn library_with_saved_and_recovery_projects_stays_unclipped() {
    let fixture = Fixture::new();
    for name in [
        "alpha.specimen.json",
        "beta.specimen.json",
        "gamma.specimen.json",
        "recovery-1.specimen.json",
        "recovery-2.specimen.json",
    ] {
        std::fs::write(fixture.directory.join(name), "{}").unwrap();
    }
    let mut editor = fixture.editor("library.specimen.json");
    let mut failures = Vec::new();
    for (width, height) in [(1100, 820), (1440, 960), (1440, 980), (1920, 1080)] {
        for selection in [None, Some("title")] {
            editor.selected = selection.map(str::to_string);
            for tab in ["type", "layout", "canvas"] {
                editor.tab = tab.into();
                let view = editor.view(width, height);
                let context = format!("{width}x{height} selection={selection:?} tab={tab}");
                let (rows, saved_shown) = library_capacity(height as f32 - 140.0, 3, true, true);
                for i in 0..saved_shown {
                    let id = format!("saved-{i}");
                    if view.layout.get(&id).is_none() {
                        failures.push(format!("{context}: expected library entry {id}"));
                    }
                }
                if view.layout.get("saved-recovery").is_none() {
                    failures.push(format!("{context}: recovery entry must stay visible"));
                }
                let layers = view
                    .app
                    .components
                    .keys()
                    .filter(|id| id.starts_with("layer-"))
                    .count();
                if layers != rows {
                    failures.push(format!(
                        "{context}: {layers} layer rows, capacity says {rows}"
                    ));
                }
                for (id, component) in &view.app.components {
                    if !matches!(component.role, Role::Button | Role::TextInput) {
                        continue;
                    }
                    if !component.enabled {
                        continue;
                    }
                    let node = view.layout.get(id).expect("Eye geometry");
                    let r = &node.rect;
                    if !node.effectively_visible
                        || node.clipped_by_parent
                        || r.x < -0.5
                        || r.y < -0.5
                        || r.x + r.w > width as f32 + 0.5
                        || r.y + r.h > height as f32 + 0.5
                    {
                        failures.push(format!("{context}: {id} clipped/outside: {r:?}"));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} library failures:\n{}",
        failures.len(),
        failures.into_iter().take(60).collect::<Vec<_>>().join("\n")
    );
}

/// Frame budget: time one full view()+render() cycle (the work a resize
/// or drag frame performs) at the default window size. Report-only; the
/// numbers are the responsiveness baseline for HAMESH 031.
#[test]
fn frame_budget_view_and_render_at_default_size() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor("frame-budget.specimen.json");
    editor.selected = Some("title".into());
    // Warm-up: font discovery and first layout fall outside the frame budget.
    let view = editor.view(1440, 980);
    let _ = editor.render(&view).unwrap();
    // Resize sweep: one full view()+render() cycle per size, i.e. the work
    // a live resize frame performs at that window size.
    for (width, height) in [(1100, 820), (1440, 960), (1440, 980), (1920, 1080)] {
        let start = std::time::Instant::now();
        let view = editor.view(width, height);
        let view_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = std::time::Instant::now();
        let pix = editor.render(&view).unwrap();
        let render_ms = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!((pix.width(), pix.height()), (width, height));
        println!(
            "[frame] {width}x{height}: view={view_ms:.1}ms render={render_ms:.1}ms total={:.1}ms",
            view_ms + render_ms
        );
    }
    // Component split: chrome raster vs artwork raster vs fingerprint.
    let start = std::time::Instant::now();
    let chrome = hamesh_render::render(&view.scene);
    let chrome_ms = start.elapsed().as_secs_f64() * 1000.0;
    let _ = chrome.pixmap;
    let start = std::time::Instant::now();
    let _ = crate::artwork::render(&editor.history.document).unwrap();
    let art_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = std::time::Instant::now();
    let _ = editor.fingerprint();
    let fp_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "[frame] split at 1440x980: chrome={chrome_ms:.1}ms artwork={art_ms:.1}ms fingerprint={fp_ms:.1}ms",
    );
}
