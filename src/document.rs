//! Canonical Specimen v1 design, persistence and bounded snapshot history.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Kind {
    Text {
        text: String,
        font: String,
        weight: u16,
        size: f32,
        tracking: f32,
        leading: f32,
        align: Alignment,
    },
    Rectangle,
    Ellipse,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub id: String,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: String,
    pub kind: Kind,
}

impl Element {
    pub fn rectangle(id: &str, x: f32, y: f32, w: f32, h: f32, color: &str) -> Self {
        Self {
            id: id.into(),
            name: id.into(),
            x,
            y,
            w,
            h,
            color: color.into(),
            kind: Kind::Rectangle,
        }
    }
    pub fn ellipse(id: &str, x: f32, y: f32, w: f32, h: f32, color: &str) -> Self {
        Self {
            kind: Kind::Ellipse,
            ..Self::rectangle(id, x, y, w, h, color)
        }
    }
    #[allow(clippy::too_many_arguments)] // Explicit artwork geometry plus typography.
    pub fn text(
        id: &str,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        size: f32,
        color: &str,
    ) -> Self {
        Self {
            kind: Kind::Text {
                text: text.into(),
                font: "Arial".into(),
                weight: 700,
                size,
                tracking: 0.0,
                leading: 1.0,
                align: Alignment::Left,
            },
            ..Self::rectangle(id, x, y, w, h, color)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub version: u32,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub background: String,
    /// Paint order, back to front. IDs remain stable across edits and history.
    pub elements: Vec<Element>,
}

pub fn rgb(hex: &str) -> Result<[u8; 3]> {
    ensure!(
        hex.len() == 7 && hex.starts_with('#') && hex[1..].bytes().all(|b| b.is_ascii_hexdigit()),
        "Colour must use #RRGGBB"
    );
    Ok([
        u8::from_str_radix(&hex[1..3], 16)?,
        u8::from_str_radix(&hex[3..5], 16)?,
        u8::from_str_radix(&hex[5..7], 16)?,
    ])
}

impl Document {
    pub fn blank() -> Self {
        Self {
            version: 1,
            title: "Untitled composition".into(),
            width: 900,
            height: 1200,
            background: "#F1EBDD".into(),
            elements: Vec::new(),
        }
    }

    pub fn template(index: usize) -> Self {
        match index {
            1 => {
                let mut d = Self {
                    title: "Forms / in motion".into(),
                    background: "#F0EBDD".into(),
                    ..Self::blank()
                };
                d.elements = vec![
                    Element::rectangle("red-field", 450., 0., 450., 1200., "#ED442A"),
                    Element::text(
                        "edition",
                        "SPECIMEN STUDIES     /     002",
                        48.,
                        40.,
                        804.,
                        40.,
                        18.,
                        "#171B20",
                    ),
                    Element::text(
                        "title",
                        "FORMS\nIN\nMOTION",
                        42.,
                        130.,
                        800.,
                        450.,
                        146.,
                        "#171B20",
                    ),
                    Element::ellipse("disc", 110., 630., 630., 360., "#171B20"),
                    Element::ellipse("disc-core", 290., 740., 265., 155., "#F0EBDD"),
                    Element::rectangle("rule", 48., 1060., 804., 3., "#171B20"),
                    Element::text(
                        "footer",
                        "A STUDY IN RHYTHM, TYPE AND SPACE",
                        48.,
                        1090.,
                        800.,
                        36.,
                        20.,
                        "#171B20",
                    ),
                    Element::text("number", "01 / 03", 48., 1140., 800., 32., 16., "#171B20"),
                ];
                d
            }
            2 => {
                let mut d = Self {
                    title: "Quiet signal / EP".into(),
                    width: 1000,
                    height: 1000,
                    background: "#E2E5DA".into(),
                    ..Self::blank()
                };
                d.elements = vec![
                    Element::rectangle("blue-field", 40., 40., 920., 920., "#142CDA"),
                    Element::ellipse("orbit-outer", 170., 230., 660., 610., "#D1E49B"),
                    Element::ellipse("orbit-inner", 255., 315., 490., 440., "#142CDA"),
                    Element::rectangle("cut", 40., 520., 920., 30., "#142CDA"),
                    Element::text(
                        "title",
                        "QUIET SIGNAL",
                        80.,
                        80.,
                        840.,
                        112.,
                        87.,
                        "#E2E5DA",
                    ),
                    Element::text(
                        "artist",
                        "MIRA / LOW FREQUENCY WORKS",
                        80.,
                        870.,
                        800.,
                        40.,
                        23.,
                        "#E2E5DA",
                    ),
                    Element::text(
                        "edition",
                        "SIDE A  /  33 RPM  /  2026",
                        80.,
                        915.,
                        800.,
                        28.,
                        15.,
                        "#D1E49B",
                    ),
                ];
                d
            }
            _ => {
                let mut d = Self {
                    title: "After Frequency / live electronics".into(),
                    background: "#181D20".into(),
                    ..Self::blank()
                };
                d.elements = vec![
                    Element::text(
                        "edition",
                        "SPECIMEN SOUND SYSTEM      /      SESSION 026",
                        48.,
                        38.,
                        804.,
                        30.,
                        17.,
                        "#DDE4BD",
                    ),
                    Element::rectangle("top-rule", 48., 88., 804., 2., "#DDE4BD"),
                    Element::text("title", "AFTER", 40., 118., 820., 152., 142., "#EEE9D9"),
                    Element::text(
                        "subtitle",
                        "FREQUENCY",
                        42.,
                        276.,
                        820.,
                        116.,
                        95.,
                        "#EEE9D9",
                    ),
                    Element::text(
                        "genre",
                        "EXPERIMENTAL ELECTRONICS / ALL NIGHT TRANSMISSION",
                        48.,
                        418.,
                        804.,
                        32.,
                        17.,
                        "#DDE4BD",
                    ),
                    Element::ellipse("orbit-lime", 130., 510., 640., 450., "#DDE4BD"),
                    Element::ellipse("orbit-dark", 198., 554., 504., 362., "#181D20"),
                    Element::ellipse("orbit-red", 310., 620., 280., 230., "#F05835"),
                    Element::ellipse("orbit-cut", 356., 658., 188., 154., "#181D20"),
                    Element::rectangle("beam", 48., 724., 804., 9., "#EEE9D9"),
                    Element::rectangle("beam-red", 48., 745., 804., 3., "#F05835"),
                    Element::text(
                        "coordinates",
                        "51.5072 N\n00.1276 W",
                        49.,
                        520.,
                        160.,
                        66.,
                        16.,
                        "#EEE9D9",
                    ),
                    Element::text("channel", "FM / 026", 675., 920., 176., 32., 18., "#DDE4BD"),
                    Element::rectangle("bottom-rule", 48., 1010., 804., 2., "#DDE4BD"),
                    Element::text(
                        "date",
                        "FRIDAY 23 OCTOBER",
                        48.,
                        1040.,
                        804.,
                        49.,
                        34.,
                        "#EEE9D9",
                    ),
                    Element::text(
                        "venue",
                        "THE SIGNAL ROOM  /  LONDON",
                        48.,
                        1100.,
                        804.,
                        34.,
                        21.,
                        "#DDE4BD",
                    ),
                    Element::text(
                        "lineup",
                        "MIRA  +  PHASE ARRAY  +  NOCTILUCENT",
                        48.,
                        1150.,
                        804.,
                        26.,
                        16.,
                        "#EEE9D9",
                    ),
                ];
                d
            }
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1,
            "Unsupported Specimen document version {}",
            self.version
        );
        ensure!(
            (64..=8192).contains(&self.width)
                && (64..=8192).contains(&self.height)
                && u64::from(self.width) * u64::from(self.height) <= 32_000_000,
            "Canvas must be 64..8192 px and at most 32 megapixels"
        );
        ensure!(self.title.len() <= 1024, "Title exceeds 1024 bytes");
        rgb(&self.background)?;
        ensure!(
            self.elements.len() <= 128,
            "At most 128 elements are supported"
        );
        let mut ids = HashSet::new();
        for e in &self.elements {
            ensure!(
                !e.id.is_empty() && e.id.len() <= 128 && ids.insert(&e.id),
                "Element IDs must be nonempty and unique"
            );
            ensure!(e.name.len() <= 1024, "Element name too long");
            ensure!(
                [e.x, e.y, e.w, e.h].into_iter().all(f32::is_finite)
                    && e.x.abs() <= 16384.
                    && e.y.abs() <= 16384.
                    && e.w > 0.
                    && e.h > 0.
                    && e.w <= 16384.
                    && e.h <= 16384.,
                "Invalid geometry for {}",
                e.id
            );
            rgb(&e.color).with_context(|| format!("Element {} colour", e.id))?;
            if let Kind::Text {
                text,
                font,
                weight,
                size,
                tracking,
                leading,
                ..
            } = &e.kind
            {
                ensure!(
                    text.len() <= 20_000 && font.len() <= 256 && !font.trim().is_empty(),
                    "Invalid text or font for {}",
                    e.id
                );
                ensure!(
                    (1..=1000).contains(weight)
                        && size.is_finite()
                        && (1.0..=1000.).contains(size)
                        && tracking.is_finite()
                        && (-50.0..=200.).contains(tracking)
                        && leading.is_finite()
                        && (0.5..=5.0).contains(leading),
                    "Invalid typography for {}",
                    e.id
                );
            }
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let metadata = std::fs::metadata(path).context("Read project metadata")?;
        ensure!(
            metadata.len() <= 4_000_000,
            "Project exceeds the 4 MB input limit"
        );
        let text = std::fs::read_to_string(path).context("Read project")?;
        let d: Self = serde_json::from_str(&text).context("Parse Specimen v1 JSON")?;
        d.validate()?;
        Ok(d)
    }

    /// Write a unique same-directory temporary file, sync, then atomically
    /// rename over the destination (Rust uses replace-existing on Windows).
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).context("Create project directory")?;
        let filename = path
            .file_name()
            .context("Project path needs a filename")?
            .to_string_lossy();
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let temp = parent.join(format!(
            ".{filename}.{}.{}.tmp",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let write_result = (|| -> Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp)
                .context("Create project temporary file")?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            std::fs::rename(&temp, path).context("Replace project")?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        write_result
    }
}

pub struct History {
    pub document: Document,
    undo: VecDeque<Document>,
    redo: Vec<Document>,
}
impl History {
    pub fn new(document: Document) -> Self {
        Self {
            document,
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }
    pub fn transact(&mut self, change: impl FnOnce(&mut Document)) -> Result<bool> {
        let mut next = self.document.clone();
        change(&mut next);
        next.validate()?;
        if next == self.document {
            return Ok(false);
        }
        self.undo
            .push_back(std::mem::replace(&mut self.document, next));
        if self.undo.len() > 100 {
            self.undo.pop_front();
        }
        self.redo.clear();
        Ok(true)
    }
    pub fn undo(&mut self) -> bool {
        if let Some(previous) = self.undo.pop_back() {
            self.redo
                .push(std::mem::replace(&mut self.document, previous));
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo
                .push_back(std::mem::replace(&mut self.document, next));
            true
        } else {
            false
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn templates_valid_and_ids_unique() {
        for i in 0..3 {
            let d = Document::template(i);
            d.validate().unwrap();
            assert!(d
                .elements
                .iter()
                .any(|e| matches!(e.kind, Kind::Text { .. })));
            assert!(d.elements.iter().any(|e| matches!(e.kind, Kind::Ellipse)));
        }
    }
    #[test]
    fn history_restores_and_rejects_invalid_atomically() {
        let d = Document::template(0);
        let mut h = History::new(d.clone());
        assert!(h.transact(|d| d.elements[0].x += 24.).unwrap());
        let moved = h.document.clone();
        assert!(h.transact(|d| d.elements[0].w = f32::NAN).is_err());
        assert_eq!(h.document, moved);
        assert!(h.undo());
        assert_eq!(h.document, d);
        assert!(h.redo());
        assert_eq!(h.document, moved);
        h.undo();
        h.transact(|d| d.title = "Other".into()).unwrap();
        assert!(!h.can_redo());
    }
    #[test]
    fn rejects_future_version_duplicate_ids_and_bad_colour() {
        let mut d = Document::template(0);
        d.version = 2;
        assert!(d.validate().is_err());
        d.version = 1;
        d.elements.push(d.elements[0].clone());
        assert!(d.validate().is_err());
        d.elements.pop();
        d.background = "#XYZ123".into();
        assert!(d.validate().is_err());
        assert!(serde_json::from_str::<Document>(r#"{"version":1,"unknown":true}"#).is_err());
    }
    #[test]
    fn save_replaces_existing_project_and_roundtrips() {
        let path =
            std::env::temp_dir().join(format!("specimen-model-{}.specimen", std::process::id()));
        let mut d = Document::template(0);
        d.save(&path).unwrap();
        d.title = "A new saved title".into();
        d.save(&path).unwrap();
        assert_eq!(d, Document::load(&path).unwrap());
        std::fs::remove_file(path).unwrap();
    }
}
