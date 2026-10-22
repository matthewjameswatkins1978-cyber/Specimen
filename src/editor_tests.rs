//! Controller/layout regression evidence. These are in-process controller
//! checks, not the separate-process Eye/Hand acceptance demonstration.
use crate::{
    document::{Document, Kind},
    editor::Editor,
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
