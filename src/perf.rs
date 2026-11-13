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
            "record_calls": self.record_calls,
            "record_ns": self.record_ns,
            "render_ns": self.render_ns,
            "chrome_ns": self.chrome_ns,
            "fingerprint_ns": self.fingerprint_ns,
            "artwork_ns": self.artwork_ns,
            "artwork_hits": self.artwork_hits,
            "artwork_misses": self.artwork_misses,
            "composite_ns": self.composite_ns,
            "mask_ns": self.mask_ns,
            "convert_ns": self.convert_ns,
            "present_ns": self.present_ns,
        })
        .to_string()
    }
}
