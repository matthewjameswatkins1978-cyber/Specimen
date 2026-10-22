//! Direct artifact verification; this supplements, never substitutes for live GUI evidence.
#[allow(dead_code)]
#[path = "../artwork.rs"]
mod artwork;
#[allow(dead_code)]
#[path = "../document.rs"]
mod document;
use anyhow::{ensure, Context, Result};
use std::path::Path;
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        anyhow::bail!("Usage: verify-artifact PROJECT EXPORTED.png WINDOW.png (guide/selection-free Eye snapshot)")
    }
    let project = Path::new(&args[0]);
    let doc = document::Document::load(project)?;
    let exported = Pixmap::load_png(&args[1]).context("Read exported PNG")?;
    let canonical = artwork::render(&doc)?;
    ensure!(
        exported.width() == doc.width && exported.height() == doc.height,
        "Export dimensions differ"
    );
    ensure!(
        exported.data() == canonical.data(),
        "Export pixels differ from canonical artwork"
    );
    let frame = Pixmap::load_png(&args[2]).context("Read Eye window snapshot")?;
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(format!("{}.json", args[2]))?)?;
    ensure!(
        metadata["document"] == serde_json::to_value(&doc)?,
        "Snapshot document differs from saved project"
    );
    let number = |k: &str| -> Result<f32> {
        Ok(metadata["paper"][k]
            .as_f64()
            .with_context(|| format!("Missing paper.{k}"))? as f32)
    };
    let (x, y, w, h) = (number("x")?, number("y")?, number("w")?, number("h")?);
    let scale = metadata["scale"].as_f64().context("Missing scale")? as f32;
    ensure!(
        (w / doc.width as f32 - scale).abs() < 0.0001
            && (h / doc.height as f32 - scale).abs() < 0.0001,
        "Canvas transform distorted artwork"
    );
    let mut projected =
        Pixmap::new(frame.width(), frame.height()).context("Allocate projection")?;
    projected.draw_pixmap(
        0,
        0,
        exported.as_ref(),
        &PixmapPaint {
            quality: FilterQuality::Bilinear,
            ..Default::default()
        },
        Transform::from_scale(scale, scale).post_translate(x, y),
        None,
    );
    let left = (x.ceil() as u32 + 2).min(frame.width());
    let top = (y.ceil() as u32 + 2).min(frame.height());
    let right = ((x + w).floor() as u32)
        .saturating_sub(2)
        .min(frame.width());
    let bottom = ((y + h).floor() as u32)
        .saturating_sub(2)
        .min(frame.height());
    let mut checked = 0u64;
    let mut mismatches = 0u64;
    for py in top..bottom {
        for px in left..right {
            let i = (py as usize * frame.width() as usize + px as usize) * 4;
            checked += 1;
            if frame.data()[i..i + 4] != projected.data()[i..i + 4] {
                mismatches += 1
            }
        }
    }
    ensure!(checked > 100, "Insufficient visible artwork to verify");
    let reports = artwork::inspect(&doc)?;
    let overflow = reports.iter().filter(|r| r.overflow).count();
    let missing: usize = reports.iter().map(|r| r.missing_glyphs).sum();
    let report = serde_json::json!({
        "mode":"Direct artifact verification after separate-process GUI operation",
        "project":project,"exported_png":args[1],"eye_snapshot":args[2],
        "document_matches_saved_project":true,"export_matches_canonical_pixels":true,
        "export_dimensions":[doc.width,doc.height],
        "screen_artwork_pixels_compared":checked,"screen_artwork_pixel_mismatches":mismatches,
        "canvas_aspect_ratio_correct":true,"text_overflow_count":overflow,"missing_glyph_count":missing,
        "snapshot_app_revision":metadata["app_revision"],"text_reports":reports,
        "renderer":"CPU","gpu_parity":"unverified"
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    ensure!(
        mismatches == 0,
        "Window canvas differs from scaled exported PNG at {mismatches} pixels"
    );
    ensure!(
        overflow == 0 && missing == 0,
        "Typography overflow or missing glyphs requires correction"
    );
    Ok(())
}
