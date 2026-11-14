//! In-process stage timers for responsiveness measurement.
//!
//! Counters accumulate in memory while the window runs and are flushed to
//! `perf.json` in the evidence directory only when the host writes its
//! interaction trace, so measuring never adds per-frame file IO. Two
//! snapshots taken around a scripted interaction give that scenario's
//! totals: read the counters, run the interaction, read them again and
//! subtract.
//!
//! Every value is wall-clock nanoseconds spent inside the named stage on
//! the UI thread. They are stage costs, not monitor frame rates, and they
//! exclude the time an external operator spends between Eye/Hand calls.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Nanoseconds elapsed since `t`, saturating at `u64::MAX`.
pub fn ns_since(t: Instant) -> u64 {
    u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Cumulative stage counters for one application run.
#[derive(Debug, Default, Clone)]
pub struct Perf {
    /// `Editor::view` calls: component tree, layout and scene construction.
    pub view_calls: u64,
    pub view_ns: u64,
    /// View builds issued only to read the canvas scale during pointer tracking.
    pub view_scale_calls: u64,
    pub view_scale_ns: u64,
    /// Component-tree construction inside `Editor::view` (Specimen side).
    pub view_app_ns: u64,
    /// Hamesh layout computation inside `Editor::view`.
    pub view_layout_ns: u64,
    /// Specimen ghost artwork-node insertion inside `Editor::view`.
    pub view_ghost_ns: u64,
    /// Hamesh scene construction inside `Editor::view`.
    pub view_scene_ns: u64,
    /// Drag gestures started by a press that selected artwork.
    pub drag_starts: u64,
    /// Input-to-visible-update samples: UI-thread time from a
    /// revision-changing pointer move to the next presented frame.
    pub input_to_present_ns: u64,
    pub input_to_present_samples: u64,
    /// Timestamp of the last revision-changing pointer move not yet
    /// followed by a presented frame. Never serialised.
    pub last_bump: Option<std::time::Instant>,
    /// `Editor::pointer_move` invocations while dragging.
    pub pointer_move_calls: u64,
    pub pointer_move_ns: u64,
    /// Host trace serialisation and file write.
    pub record_calls: u64,
    pub record_ns: u64,
    /// Completed frames presented.
    pub frames: u64,
    /// `Editor::render` total.
    pub render_ns: u64,
    /// Chrome scene raster through the Hamesh CPU renderer.
    pub chrome_ns: u64,
    /// Document fingerprint used as the artwork cache key.
    pub fingerprint_ns: u64,
    /// Full-resolution poster raster (text layout and glyph rasterisation).
    pub artwork_ns: u64,
    pub artwork_hits: u64,
    pub artwork_misses: u64,
    /// Thumbnails, palette chips, artwork blit, guides and selection overlay.
    pub composite_ns: u64,
    /// Library template thumbnail blits.
    pub thumbs_ns: u64,
    /// Colour-swatch chip blits.
    pub palette_ns: u64,
    /// Full artwork blit onto the frame, including its scale to the paper.
    pub art_blit_ns: u64,
    /// Guides and selection overlay strokes.
    pub overlay_ns: u64,
    /// Zoom/clipping mask construction.
    pub mask_ns: u64,
    /// Pixmap to softbuffer pixel conversion.
    pub convert_ns: u64,
    /// softbuffer presentation.
    pub present_ns: u64,
}

impl Perf {
    /// Serialise the cumulative counters as one compact JSON object.
    pub fn snapshot_json(&self) -> String {
        let unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        serde_json::json!({
            "utc_unix": unix,
            "frames": self.frames,
            "view_calls": self.view_calls,
            "view_ns": self.view_ns,
            "view_scale_calls": self.view_scale_calls,
            "view_scale_ns": self.view_scale_ns,
            "view_app_ns": self.view_app_ns,
            "view_layout_ns": self.view_layout_ns,
            "view_ghost_ns": self.view_ghost_ns,
            "view_scene_ns": self.view_scene_ns,
            "drag_starts": self.drag_starts,
            "input_to_present_ns": self.input_to_present_ns,
            "input_to_present_samples": self.input_to_present_samples,
            "pointer_move_calls": self.pointer_move_calls,
            "pointer_move_ns": self.pointer_move_ns,
            "record_calls": self.record_calls,
            "record_ns": self.record_ns,
            "render_ns": self.render_ns,
            "chrome_ns": self.chrome_ns,
            "fingerprint_ns": self.fingerprint_ns,
            "artwork_ns": self.artwork_ns,
            "artwork_hits": self.artwork_hits,
            "artwork_misses": self.artwork_misses,
            "composite_ns": self.composite_ns,
            "thumbs_ns": self.thumbs_ns,
            "palette_ns": self.palette_ns,
            "art_blit_ns": self.art_blit_ns,
            "overlay_ns": self.overlay_ns,
            "mask_ns": self.mask_ns,
            "convert_ns": self.convert_ns,
            "present_ns": self.present_ns,
        })
        .to_string()
    }
}
