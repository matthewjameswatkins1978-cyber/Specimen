//! View-construction microbenchmark with warmup; prints median/p95 JSON.
//!
//! Run in release for comparable trials:
//! `cargo run --release --bin view-bench -- --iterations 200 --warmup 50`
//! Three phases cover representative UI states: plain Fit, Fit with a
//! selection, and Fit with a selection plus a focused inspector field
//! (the typing state). Stage means come from the in-process timers.
#[allow(dead_code)]
#[path = "../artwork.rs"]
mod artwork;
#[allow(dead_code)]
#[path = "../document.rs"]
mod document;
#[allow(dead_code)]
#[path = "../editor.rs"]
mod editor;
#[allow(dead_code)]
#[path = "../perf.rs"]
mod perf;

use anyhow::{Context, Result};

fn arg_value(args: &[String], name: &str, fallback: u64) -> u64 {
    args.windows(2)
        .find(|w| w[0] == name)
        .and_then(|w| w[1].parse().ok())
        .unwrap_or(fallback)
}

fn percentile(sorted: &[u64], pct: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (pct / 100.0 * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn phase(
    ed: &mut editor::Editor,
    width: u32,
    height: u32,
    warmup: usize,
    iterations: usize,
) -> serde_json::Value {
    for _ in 0..warmup {
        let _ = ed.view(width, height);
    }
    ed.perf = perf::Perf::default();
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let t = std::time::Instant::now();
        let _ = ed.view(width, height);
        samples.push(perf::ns_since(t));
    }
    samples.sort_unstable();
    let sum: u128 = samples.iter().map(|&v| v as u128).sum();
    let n = iterations as f64;
    let mean = |total: u64| (total as f64 / n).round() as u64;
    serde_json::json!({
        "iterations": iterations,
        "per_view_ns": {
            "min": samples[0],
            "p50": percentile(&samples, 50.0),
            "p95": percentile(&samples, 95.0),
            "max": samples[samples.len() - 1],
            "mean": (sum as f64 / n).round() as u64,
        },
        "stage_mean_ns": {
            "app": mean(ed.perf.view_app_ns),
            "layout": mean(ed.perf.view_layout_ns),
            "ghost": mean(ed.perf.view_ghost_ns),
            "scene": mean(ed.perf.view_scene_ns),
        },
    })
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let iterations = arg_value(&args, "--iterations", 200) as usize;
    let warmup = arg_value(&args, "--warmup", 50) as usize;
    let width = arg_value(&args, "--width", 1440) as u32;
    let height = arg_value(&args, "--height", 980) as u32;
    anyhow::ensure!(iterations > 0, "need at least one iteration");

    let dir = std::env::temp_dir().join(format!("specimen-view-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let mut ed = editor::Editor::new(dir.join("bench.specimen.json"))?;

    let fit = phase(&mut ed, width, height, warmup, iterations);
    ed.action("layer-title")?;
    let selected = phase(&mut ed, width, height, warmup, iterations);
    ed.focus_field("text");
    let typing = phase(&mut ed, width, height, warmup, iterations);

    // Live-session simulation: bump the revision and change view-affecting
    // state between builds, the way real operator actions do. If per-view
    // cost jumps here, the cost depends on revision churn, not on the build.
    ed.perf = perf::Perf::default();
    let mut churn_samples = Vec::with_capacity(iterations);
    for i in 0..iterations {
        if i % 3 == 0 {
            ed.action("zoom")?;
        }
        if i % 5 == 0 {
            ed.action("undo").unwrap_or(());
            ed.action("redo").unwrap_or(());
        }
        let t = std::time::Instant::now();
        let _ = ed.view(width, height);
        churn_samples.push(perf::ns_since(t));
    }
    churn_samples.sort_unstable();
    let churn_sum: u128 = churn_samples.iter().map(|&v| v as u128).sum();
    let n = iterations as f64;
    let churn_mean = |total: u64| (total as f64 / n).round() as u64;
    let churn = serde_json::json!({
        "iterations": iterations,
        "per_view_ns": {
            "min": churn_samples[0],
            "p50": percentile(&churn_samples, 50.0),
            "p95": percentile(&churn_samples, 95.0),
            "max": churn_samples[churn_samples.len() - 1],
            "mean": (churn_sum as f64 / n).round() as u64,
        },
        "stage_mean_ns": {
            "app": churn_mean(ed.perf.view_app_ns),
            "layout": churn_mean(ed.perf.view_layout_ns),
            "ghost": churn_mean(ed.perf.view_ghost_ns),
            "scene": churn_mean(ed.perf.view_scene_ns),
        },
    });

    let out = serde_json::json!({
        "bench": "view",
        "pin": "worktree",
        "width": width,
        "height": height,
        "warmup": warmup,
        "phases": {
            "fit": fit,
            "selected": selected,
            "typing": typing,
            "churn": churn,
        },
    });
    println!(
        "{}",
        serde_json::to_string(&out).context("serialise bench output")?
    );
    Ok(())
}
