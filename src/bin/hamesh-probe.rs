//! Reproducible diagnostic probes against Specimen's pinned Hamesh baseline.
//! Success means diagnostics were collected, not that the framework passed.
use anyhow::{Context, Result};
use hamesh_blocks as b;
use hamesh_core::{App, Component, Role};
use serde_json::{json, Value};
use std::path::Path;

fn main() -> Result<()> {
    std::fs::create_dir_all("outputs")?;
    let mut overlap_runs = Vec::new();
    let mut mismatch_count = 0;
    let mut paint_not_last_child = 0;
    let mut hit_not_last_child = 0;
    let mut captured_mismatch = false;
    for iteration in 0..64 {
        let mut app = App::new("overlap");
        app.get_mut("overlap")
            .unwrap()
            .props
            .extend([("pad".into(), "0".into()), ("gap".into(), "0".into())]);
        for (id, text, surface) in [
            ("overlay-a", "A / inserted first", "accent"),
            ("overlay-b", "B / inserted last", "warning"),
        ] {
            app.add(
                "overlap",
                Component::new(id, Role::Button, text)
                    .with_prop("overlay", "true")
                    .with_prop("ox", "40")
                    .with_prop("oy", "40")
                    .with_prop("w", "220")
                    .with_prop("h", "100")
                    .with_prop("surface", surface)
                    .with_prop("text", text),
            );
        }
        let layout = hamesh_layout::compute(&app, 320., 220.);
        let scene = hamesh_scene::build(&app, &layout, iteration + 1);
        let paint_order: Vec<_> = scene
            .nodes
            .iter()
            .filter(|n| n.id.starts_with("overlay-"))
            .map(|n| n.id.as_str())
            .collect();
        let painted_last = paint_order
            .last()
            .copied()
            .context("Overlay missing from scene")?;
        let hit = hamesh_layout::hit_test(&app, &layout, 100., 90.);
        let mismatch = hit.as_deref() != Some(painted_last);
        mismatch_count += usize::from(mismatch);
        paint_not_last_child += usize::from(painted_last != "overlay-b");
        hit_not_last_child += usize::from(hit.as_deref() != Some("overlay-b"));
        if iteration == 0 || mismatch && !captured_mismatch {
            hamesh_render::render(&scene)
                .pixmap
                .save_png("outputs/hamesh-overlap.png")?;
            captured_mismatch = mismatch;
        }
        overlap_runs.push(json!({
            "iteration":iteration,"point":[100,90],"paint_order":paint_order,
            "painted_last":painted_last,"hit":hit,"paint_hit_disagree":mismatch,
            "overlay_z":[layout.get("overlay-a").unwrap().z,layout.get("overlay-b").unwrap().z],
            "declared_children":app.get("overlap").unwrap().children
        }));
    }

    let mut intrinsic = App::new("intrinsic");
    intrinsic
        .get_mut("intrinsic")
        .unwrap()
        .props
        .extend([("pad".into(), "0".into()), ("gap".into(), "0".into())]);
    {
        let mut scope = b::Scope::root(&mut intrinsic, "intrinsic");
        b::row(
            &mut scope,
            "row",
            b::LayoutOpts {
                pad: 0.,
                gap: 8.,
                h: Some(70.),
                ..b::LayoutOpts::default()
            },
            |s| {
                b::button(
                    s,
                    "auto-button",
                    "Export finished artwork",
                    b::ButtonOpts::new(b::ButtonVariant::Primary, "export"),
                );
                b::caption(s, "auto-label", "Document properties");
            },
        );
    }
    let intrinsic_layout = hamesh_layout::compute(&intrinsic, 600., 100.);
    let button_rect = &intrinsic_layout.get("auto-button").unwrap().rect;
    let label_rect = &intrinsic_layout.get("auto-label").unwrap().rect;
    let intrinsic_diagnosis = json!({
        "button":{"text":intrinsic.get("auto-button").unwrap().props.get("text"),"rect":button_rect,
            "declared_padding":8,"padding_only_width":16,"observed_padding_only":button_rect.w<=16.1},
        "label":{"text":intrinsic.get("auto-label").unwrap().props.get("text"),"rect":label_rect,
            "default_padding":12,"padding_only_width":24,"observed_padding_only":label_rect.w<=24.1},
        "diagnosis":"Nonempty auto-width row labels/buttons receive no intrinsic text measurement on this baseline."
    });

    let mut clipping = App::new("clipping");
    clipping.get_mut("clipping").unwrap().props.extend([
        ("pad".into(), "0".into()),
        ("gap".into(), "0".into()),
        ("surface".into(), "bg".into()),
    ]);
    clipping.add(
        "clipping",
        Component::new("parent", Role::Panel, "Fixed parent")
            .with_prop("w", "100")
            .with_prop("h", "100")
            .with_prop("flex_grow", "0")
            .with_prop("pad", "0")
            .with_prop("surface", "surface"),
    );
    clipping.add(
        "parent",
        Component::new("overflow-child", Role::Button, "Overflow child")
            .with_prop("overlay", "true")
            .with_prop("ox", "80")
            .with_prop("oy", "30")
            .with_prop("w", "100")
            .with_prop("h", "50")
            .with_prop("surface", "warning"),
    );
    let clipping_layout = hamesh_layout::compute(&clipping, 260., 180.);
    let clipping_scene = hamesh_scene::build(&clipping, &clipping_layout, 1);
    let clipping_frame = hamesh_render::render(&clipping_scene);
    let point = (150_u32, 50_u32);
    let offset = ((point.1 * clipping_frame.pixmap.width() + point.0) * 4) as usize;
    let pixel = &clipping_frame.pixmap.data()[offset..offset + 4];
    let child_scene = clipping_scene
        .nodes
        .iter()
        .find(|n| n.id == "overflow-child")
        .unwrap();
    let parent_node = clipping_layout.get("parent").unwrap();
    let outside_parent = !parent_node.rect.contains(point.0 as f32, point.1 as f32);
    let overflow_hit =
        hamesh_layout::hit_test(&clipping, &clipping_layout, point.0 as f32, point.1 as f32);
    let clipping_diagnosis = json!({
        "parent":parent_node,"child":clipping_layout.get("overflow-child").unwrap(),
        "sample_point":[point.0,point.1],"sample_outside_parent":outside_parent,
        "cpu_pixel_rgba":pixel,"child_fill_rgba":child_scene.color,
        "child_painted_outside_parent":outside_parent && pixel==child_scene.color,
        "hit_outside_parent":overflow_hit,
        "diagnosis":"clipped_by_parent is a geometry diagnostic; this baseline does not enforce an ancestor clip in CPU paint or hit testing."
    });

    let mut flow = App::new("flow");
    flow.get_mut("flow")
        .unwrap()
        .props
        .extend([("pad".into(), "0".into()), ("gap".into(), "0".into())]);
    flow.add(
        "flow",
        Component::new("canvas", Role::Panel, "Growing canvas")
            .with_prop("pad", "0")
            .with_prop("gap", "0")
            .with_prop("flex_grow", "1"),
    );
    let before = hamesh_layout::compute(&flow, 600., 400.);
    flow.add(
        "canvas",
        Component::new("canvas-fill", Role::Panel, "Flexible canvas content")
            .with_prop("pad", "0")
            .with_prop("flex_grow", "1"),
    );
    let content_before = hamesh_layout::compute(&flow, 600., 400.);
    flow.add(
        "flow",
        Component::new("floating-child", Role::Button, "Floating child")
            .with_prop("overlay", "true")
            .with_prop("ox", "30")
            .with_prop("oy", "30")
            .with_prop("w", "120")
            .with_prop("h", "80"),
    );
    let after = hamesh_layout::compute(&flow, 600., 400.);
    let flow_delta = content_before.get("canvas-fill").unwrap().rect.h
        - after.get("canvas-fill").unwrap().rect.h;
    let flow_diagnosis = json!({
        "canvas_empty":before.get("canvas").unwrap().rect,
        "canvas_before_overlay":content_before.get("canvas").unwrap().rect,
        "canvas_after_overlay":after.get("canvas").unwrap().rect,
        "growing_content_before":content_before.get("canvas-fill").unwrap().rect,
        "growing_content_after":after.get("canvas-fill").unwrap().rect,
        "content_height_lost":flow_delta,
        "overlay_consumes_flow":flow_delta.abs()>0.5,
        "diagnosis":"Overlay geometry is replaced after Taffy flow; its style still participates in sizing its siblings."
    });

    let defects_observed = mismatch_count > 0
        || paint_not_last_child > 0
        || hit_not_last_child > 0
        || button_rect.w <= 16.1
        || label_rect.w <= 24.1
        || clipping_diagnosis["child_painted_outside_parent"]
            .as_bool()
            .unwrap_or(false)
        || clipping_diagnosis["hit_outside_parent"].as_str() == Some("overflow-child")
        || flow_delta.abs() > 0.5;
    let diagnostics: Value = json!({
        "schema_version":1,
        "framework":{"repository":"https://github.com/matthewjameswatkins1978-cyber/hameshgui",
            "revision":"9fa37c33a92620151e0fbfbd6e22c7ae32bccbf8"},
        "scope":"Headless runtime diagnostics against exact pinned dependencies; not GUI, GPU or framework acceptance.",
        "collection_status":"SUCCESS",
        "verdict":if defects_observed {"DEFECTS_OBSERVED"} else {"NO_DEFECTS_OBSERVED"},
        "overlap":{"iterations":64,"paint_hit_mismatches":mismatch_count,
            "paint_departures_from_last_child_b":paint_not_last_child,
            "hit_departures_from_last_child_b":hit_not_last_child,
            "mismatch_snapshot_captured":captured_mismatch,
            "diagnosis":"Equal-z ordering traverses independently randomized HashMaps rather than declared child order; counts vary per execution.",
            "runs":overlap_runs},
        "intrinsic_sizing":intrinsic_diagnosis,"ancestor_clipping":clipping_diagnosis,"overlay_flow":flow_diagnosis
    });
    std::fs::write(
        Path::new("outputs/hamesh-probe.json"),
        serde_json::to_vec_pretty(&diagnostics)?,
    )?;
    println!("Diagnostics collected at pinned Hamesh 9fa37c3: overlap paint/hit mismatches {mismatch_count}/64; paint != last child {paint_not_last_child}/64; hit != last child {hit_not_last_child}/64; auto button width {}, label width {}; overlay flow height loss {flow_delta}.",button_rect.w,label_rect.w);
    println!("Evidence: outputs/hamesh-probe.json and outputs/hamesh-overlap.png. Observed defects are diagnostic findings, not passing acceptance.");
    Ok(())
}
