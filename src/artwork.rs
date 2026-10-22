//! One artwork renderer for the live canvas and PNG export.
//!
//! Hamesh 9fa37c3's text wrapper exposes only size. This narrow bridge
//! reuses its Parley + Swash stack to shape full document typography;
//! it is artwork paint, not a competing GUI/widget implementation.
use crate::document::{rgb, Alignment, Document, Element, Kind};
use anyhow::{Context, Result};
use parley::style::{FontFamily, FontWeight, LineHeight, StyleProperty};
use parley::{AlignmentOptions, FontContext, Layout, LayoutContext, PositionedLayoutItem};
use serde::Serialize;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Format, Vector};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

thread_local! { static FONT_CONTEXT: RefCell<FontContext> = RefCell::new(FontContext::new()); }

/// Installed families, discovered once per process. No font files are copied
/// or installed. Parley's platform fallback handles missing families/glyphs.
pub fn fonts() -> Vec<String> {
    static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();
    FAMILIES
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            let mut families: Vec<String> = db
                .faces()
                .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
                .collect();
            families.sort_by_key(|name| name.to_lowercase());
            families.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            families
        })
        .clone()
}

fn text_layout(e: &Element) -> Option<Layout<peniko::Brush>> {
    let Kind::Text {
        text,
        font,
        weight,
        size,
        tracking,
        leading,
        align,
    } = &e.kind
    else {
        return None;
    };
    FONT_CONTEXT.with(|fcx| {
        let mut fcx = fcx.borrow_mut();
        let mut lcx: LayoutContext<peniko::Brush> = LayoutContext::new();
        let mut builder = lcx.ranged_builder(&mut fcx, text, 1., false);
        builder.push_default(StyleProperty::FontSize(*size));
        builder.push_default(StyleProperty::FontFamily(FontFamily::from(font.as_str())));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(
            *weight,
        ))));
        builder.push_default(StyleProperty::LetterSpacing(*tracking));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            *leading,
        )));
        let mut layout = builder.build(text);
        layout.break_all_lines(Some(e.w));
        layout.align(
            match align {
                Alignment::Left => parley::Alignment::Left,
                Alignment::Center => parley::Alignment::Center,
                Alignment::Right => parley::Alignment::Right,
            },
            AlignmentOptions::default(),
        );
        Some(layout)
    })
}

#[derive(Debug, Serialize)]
pub struct TextReport {
    pub id: String,
    pub lines: usize,
    pub layout_width: f32,
    pub layout_height: f32,
    pub ink_bounds: Option<[f32; 4]>,
    pub overflow: bool,
    pub missing_glyphs: usize,
    pub requested_font_available: bool,
}

/// Metrics and actual glyph ink bounds, in artwork coordinates. Overflow
/// includes glyph ink overhang, not merely the line box height.
pub fn inspect(document: &Document) -> Result<Vec<TextReport>> {
    document.validate()?;
    let available = fonts();
    let mut reports = Vec::new();
    let mut context = ScaleContext::new();
    for e in &document.elements {
        if let Some(layout) = text_layout(e) {
            let mut ink: Option<[f32; 4]> = None;
            let mut missing = 0;
            raster_glyphs(
                e,
                &layout,
                &mut context,
                |x, y, _| {
                    if let Some(b) = &mut ink {
                        b[0] = b[0].min(x as f32);
                        b[1] = b[1].min(y as f32);
                        b[2] = b[2].max(x as f32 + 1.);
                        b[3] = b[3].max(y as f32 + 1.);
                    } else {
                        ink = Some([x as f32, y as f32, x as f32 + 1., y as f32 + 1.]);
                    }
                },
                &mut missing,
            );
            let Kind::Text { font, .. } = &e.kind else {
                unreachable!()
            };
            let overflow = layout.height() > e.h + 1.
                || ink.is_some_and(|b| {
                    b[0] < e.x - 1.
                        || b[1] < e.y - 1.
                        || b[2] > e.x + e.w + 1.
                        || b[3] > e.y + e.h + 1.
                });
            reports.push(TextReport {
                id: e.id.clone(),
                lines: layout.lines().len(),
                layout_width: layout.width(),
                layout_height: layout.height(),
                ink_bounds: ink,
                overflow,
                missing_glyphs: missing,
                requested_font_available: available.iter().any(|f| f.eq_ignore_ascii_case(font)),
            });
        }
    }
    Ok(reports)
}

/// Rasterize positioned Parley glyphs with the same baseline convention as
/// hamesh-text::glue. Pixel callbacks include unclipped ink for diagnostics.
fn raster_glyphs(
    e: &Element,
    layout: &Layout<peniko::Brush>,
    context: &mut ScaleContext,
    mut pixel: impl FnMut(i32, i32, u8),
    missing: &mut usize,
) {
    let mut render = Render::new(&[Source::Outline]);
    render.format(Format::Alpha);
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(gr) = item else {
                continue;
            };
            let run = gr.run();
            let font = run.font();
            let Some(font_ref) = swash::FontRef::from_index(font.data.data(), font.index as usize)
            else {
                continue;
            };
            let mut scaler = context
                .builder(font_ref)
                .size(run.font_size())
                .hint(true)
                .normalized_coords(run.normalized_coords().iter().map(|c| c.to_bits()))
                .build();
            render.embolden(if run.synthesis().embolden() {
                run.font_size() * 0.025
            } else {
                0.
            });
            for g in gr.positioned_glyphs() {
                if g.id == 0 {
                    *missing += 1;
                }
                render.offset(Vector::new(g.x.fract(), g.y.fract()));
                let Some(image) = render.render(&mut scaler, g.id as u16) else {
                    continue;
                };
                let x0 = (e.x + g.x + image.placement.left as f32).floor() as i32;
                let y0 = (e.y + g.y - image.placement.top as f32).floor() as i32;
                for (i, alpha) in image.data.iter().copied().enumerate() {
                    if alpha == 0 {
                        continue;
                    }
                    let x = x0 + (i as u32 % image.placement.width) as i32;
                    let y = y0 + (i as u32 / image.placement.width) as i32;
                    pixel(x, y, alpha);
                }
            }
        }
    }
}

pub fn render(document: &Document) -> Result<Pixmap> {
    document.validate()?;
    let mut pix = Pixmap::new(document.width, document.height).context("Allocate artwork")?;
    let [r, g, b] = rgb(&document.background)?;
    pix.fill(Color::from_rgba8(r, g, b, 255));
    let mut context = ScaleContext::new();
    for e in &document.elements {
        let [r, g, b] = rgb(&e.color)?;
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, 255);
        match &e.kind {
            Kind::Rectangle => {
                if let Some(rect) = Rect::from_xywh(e.x, e.y, e.w, e.h) {
                    pix.fill_rect(rect, &paint, Transform::identity(), None);
                }
            }
            Kind::Ellipse => {
                if let Some(rect) = Rect::from_xywh(e.x, e.y, e.w, e.h) {
                    if let Some(path) = PathBuilder::from_oval(rect) {
                        pix.fill_path(
                            &path,
                            &paint,
                            FillRule::Winding,
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
            Kind::Text { .. } => {
                let layout = text_layout(e).expect("text layout");
                let (width, height) = (pix.width() as i32, pix.height() as i32);
                let mut missing = 0;
                raster_glyphs(
                    e,
                    &layout,
                    &mut context,
                    |x, y, alpha| {
                        if x < 0
                            || y < 0
                            || x >= width
                            || y >= height
                            || (x as f32) < e.x
                            || (y as f32) < e.y
                            || (x as f32) >= e.x + e.w
                            || (y as f32) >= e.y + e.h
                        {
                            return;
                        }
                        let index = (y as usize * width as usize + x as usize) * 4;
                        let dst = &mut pix.data_mut()[index..index + 4];
                        let a = u32::from(alpha);
                        for (dest, source) in dst[..3].iter_mut().zip([r, g, b]) {
                            *dest = ((u32::from(source) * a + u32::from(*dest) * (255 - a) + 127)
                                / 255) as u8;
                        }
                        dst[3] = 255;
                    },
                    &mut missing,
                );
            }
        }
    }
    Ok(pix)
}

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

fn check_export_destination(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("Inspect PNG destination"),
        Ok(metadata) => anyhow::ensure!(
            metadata.is_file(),
            "Refusing to replace a non-file PNG destination: {}",
            path.display()
        ),
    }
    let mut signature = [0; 8];
    let mut file = std::fs::File::open(path).context("Read existing PNG destination")?;
    anyhow::ensure!(
        file.read_exact(&mut signature).is_ok() && &signature == PNG_SIGNATURE,
        "Refusing to overwrite an existing non-PNG file: {}. Choose another export path.",
        path.display()
    );
    Ok(())
}

/// Replace only an existing PNG image. Stage complete validated PNG bytes in
/// a unique sibling, sync and close it, then atomically rename into place.
/// Recheck the destination immediately before replacement; unrelated files
/// and directories are never treated as disposable image exports.
pub fn export(document: &Document, path: &Path) -> Result<()> {
    check_export_destination(path)?;
    let pix = render(document)?;
    let bytes = pix.encode_png().context("Encode PNG artwork")?;
    let decoded = Pixmap::decode_png(&bytes).context("Validate encoded PNG artwork")?;
    anyhow::ensure!(
        decoded.width() == document.width && decoded.height() == document.height,
        "Encoded PNG dimensions do not match the artwork"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).context("Create PNG export directory")?;
    let filename = path
        .file_name()
        .context("PNG export needs a filename")?
        .to_string_lossy();
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let temporary = parent.join(format!(
        ".{filename}.specimen-export-{}-{}.tmp.png",
        std::process::id(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .context("Create PNG export temporary file")?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)
            .context("Write PNG export temporary file")?;
        file.sync_all().context("Sync PNG export temporary file")?;
        drop(file);
        check_export_destination(path)?;
        std::fs::rename(&temporary, path).context("Atomically replace PNG export")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Generates direct-document visual proofs; never GUI evidence"]
    fn generate_starter_visual_proofs() {
        std::fs::create_dir_all("outputs").unwrap();
        for i in 0..3 {
            let d = Document::template(i);
            export(&d, Path::new(&format!("outputs/starter-{i}.png"))).unwrap();
            let report = inspect(&d).unwrap();
            std::fs::write(
                format!("outputs/starter-{i}-metrics.json"),
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
            println!(
                "starter {i}: {} text layers, {} overflow reports",
                report.len(),
                report.iter().filter(|r| r.overflow).count()
            );
        }
    }
    #[test]
    fn colours_geometry_and_paint_order_are_exact() {
        let mut d = Document::blank();
        d.width = 100;
        d.height = 100;
        d.background = "#112233".into();
        d.elements
            .push(Element::rectangle("r", 10., 10., 50., 40., "#FF0000"));
        d.elements
            .push(Element::ellipse("e", 20., 20., 20., 20., "#00FF00"));
        let p = render(&d).unwrap();
        let at = |x: usize, y: usize| &p.data()[(y * 100 + x) * 4..(y * 100 + x) * 4 + 4];
        assert_eq!(at(0, 0), [17, 34, 51, 255]);
        assert_eq!(at(12, 12), [255, 0, 0, 255]);
        assert_eq!(at(30, 30), [0, 255, 0, 255]);
    }
    #[test]
    fn typography_changes_pixels_and_export_matches_canonical_render() {
        let mut d = Document::blank();
        d.width = 400;
        d.height = 200;
        d.elements.push(Element::text(
            "t", "AFTER", 10., 10., 380., 160., 70., "#111111",
        ));
        let a = render(&d).unwrap();
        if let Kind::Text {
            tracking, weight, ..
        } = &mut d.elements[0].kind
        {
            *tracking = 8.;
            *weight = 400;
        }
        let b = render(&d).unwrap();
        assert_ne!(a.data(), b.data(), "typographic controls must affect paint");
        let path = std::env::temp_dir().join(format!("specimen-export-{}.png", std::process::id()));
        export(&d, &path).unwrap();
        let decoded = Pixmap::load_png(&path).unwrap();
        assert_eq!(b.data(), decoded.data());
        std::fs::remove_file(path).unwrap();
        let report = inspect(&d).unwrap();
        assert!(report[0].ink_bounds.is_some());
        assert_eq!(report[0].missing_glyphs, 0);
    }
}
