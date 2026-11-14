# Responsiveness: what Hamesh 044 changed, and what Specimen fixed

Measured 13 November 2026 in one session on one machine. This report is the
before/after evidence for the Specimen 0.2 responsiveness slice.

## How the numbers were obtained

- **Fixture and sequence.** `scripts/perf-scenario.ps1` launches the real
  native window at a fixed `--window-size=1440x980`, opens the *After
  Frequency* starter (17 elements, 900 x 1200) and runs one identical
  scripted session: layer selection, 20 arrow nudges, 8 type-and-commit
  cycles, 6 typography commits, 20 undo/redo clicks, 8 zoom cycles, 12 layer
  pagers, 4 window resizes, save and export. Every run reported
  `op_failures = 0` except where noted below.
- **Stage timers.** `src/perf.rs` accumulates in-process wall-clock
  nanoseconds per stage and flushes them to `outputs/perf.json` only when the
  host writes its interaction trace, so measurement adds no per-frame file
  IO. Two snapshots are taken around the scenario and differenced.
- **Operator latency** is the Eye/Hand CLI round-trip. It includes one CLI
  process spawn per call, so it bounds application responsiveness rather than
  isolating it.
- **Machine.** Windows 11 build 26320, 2560 x 1440 at 96 DPI, rustc 1.100.0.
  Baseline worktree at old pin `c5b794c35fb97fd067c5cf7be7e5d75b9a19b49f`;
  candidate worktree at accepted pin `53b0b96b7a1897737cb08e2139f62c07815d4817`.
- Raw reports: `perf-report-{old,new,fixed}-{debug,release}.json`.

## Cold start and whole-session wall clock

| | old pin debug | new pin debug | old pin release | new pin release |
|---|---|---|---|---|
| Cold start to first Eye map | 2860 ms | 2417 ms | 552 ms | 633 ms |
| Whole scripted session | 84.2 s | 55.3 s | 7.20 s | 6.76 s |

The debug build is roughly **55x slower** than release on the same work. Any
responsiveness claim that does not name the profile is meaningless.

## Per-frame cost (average over the scenario)

| Stage (per frame) | old debug | new debug | old release | new release |
|---|---|---|---|---|
| Whole frame | 656.83 ms | 445.59 ms | 23.20 ms | **12.47 ms** |
| Chrome paint (Hamesh) | 271.11 ms | 60.41 ms | 15.82 ms | **4.71 ms** |
| Compositing total | 368.90 ms | 354.83 ms | 5.45 ms | 5.51 ms |
| — artwork blit | n/a (not split) | 335.52 ms | n/a | 4.96 ms |
| — thumbnails | n/a | 13.00 ms | n/a | 0.33 ms |
| Pixel conversion | 35.92 ms | 36.81 ms | 0.84 ms | 0.88 ms |
| Artwork raster, per cache miss | 68.01 ms | 65.21 ms | 4.27 ms | 4.30 ms |

**Hamesh 044's effect:** frame time halves in release (23.20 -> 12.47 ms) and
chrome paint drops 3.4x (15.82 -> 4.71 ms); in debug the frame improves 1.47x
and chrome 4.5x. Specimen's own Parley/Swash artwork raster is unchanged, as
expected: 044's caches do not cover that path.

**View construction** (component tree + layout + scene), per build:

| | old debug | new debug | old release | new release |
|---|---|---|---|---|
| Per view build | 99.179 ms | 13.842 ms | 5.177 ms | 7.877 ms |

Debug builds are 7.2x cheaper on 044. In release, view construction measured
*slower* per build (5.177 -> 7.877 ms) while every other stage improved; that
is the one stage where 044 regressed, and it matters because a view is built
several times per operator action.

## Operator latency (Eye/Hand round-trip, ms average)

| Action | old release | new release | old debug | new debug |
|---|---|---|---|---|
| Select layer | 43.65 | 41.72 | 890.40 | 430.82 |
| Arrow nudge | 36.43 | 29.15 | *invalid* | 492.21 |
| Type + commit | 72.58 | 74.30 | *invalid* | 1035.41 |
| Typography commit | 75.57 | 69.42 | 1829.01 | 1059.52 |
| Undo / redo | 41.08 | 40.42 | 975.73 | 567.94 |
| Zoom cycle | 40.42 | 37.97 | 1007.68 | 477.74 |
| Window resize ack | 63.82 | 77.98 | 842.62 | 475.59 |
| Save / export | 54.94 | 45.37 | 1058.13 | 641.61 |

The old-pin **debug** nudge and type rows are marked invalid: that run
recorded 21 nudge failures and 1 type failure (stale-revision rejections
returned fast), so its averages are not comparable. All other cells are from
runs with zero failures.

## Measured but not material

Each of these was measured rather than assumed, and deliberately left alone:

- **Trace serialisation and file write:** 0.36-0.42 ms per call in release
  (about 40-48 ms across a whole session). Not worth batching.
- **Document fingerprint used as the artwork cache key:** 1.62-1.65 ms across
  a whole session. Not worth replacing.
- **Zoom/clipping mask:** 7.24 ms per session. **Palette chips and selection
  overlay:** 3.70 / 9.78 ms per session. **Presentation:** ~70 ms per session.

## Remaining bottlenecks

1. **Debug-profile artwork blit: 335-338 ms per frame.** Scaling the cached
   900 x 1200 poster onto the frame dominates the debug frame (75% of render
   time). It is 4.96-4.98 ms in release, so it is a developer-profile cost,
   not a shipped one; no code was changed for it.
2. **View construction in release rose to 7.877 ms per build** and is built
   several times per action. This is the one measured 044 regression and the
   best remaining candidate for a follow-up slice.
3. **Full-frame rebuild per interaction.** Every revision change re-renders
   chrome, composites the poster and converts the whole pixmap. Release
   absorbs this at 12.47 ms/frame; debug does not.

## What this slice changed, and what it does not prove

1. **No view rebuild per pointer move.** `Host::window_event` used to build
   the whole tree, layout and scene on every `CursorMoved` during a drag just
   to read the canvas scale. The `Drag` now carries the scale captured from
   the real view at press time, and the window-size path re-syncs it. Each
   avoided build is worth the measured per-build cost above (7.877 ms release,
   13.842 ms debug), i.e. 0.65 s and 1.15 s of CPU per second of dragging at
   125 Hz.
2. **No-op pointer moves no longer bump the revision.** Snapping or clamping
   that leaves a layer where it was previously forced a full frame rebuild
   per mouse move (11.94 ms release, 446.91 ms debug). Covered by
   `pointer_move_only_bumps_the_revision_when_the_layer_actually_moves`.

**Update (finishing slice, 14 Nov 2026): both fixes are now proven by real
drags.** `scripts/verify-drag.ps1` drives the actual Windows input path
against the release executable: 5 gestures (text snap, shape snap, rapid
motion, clamp-min, clamp-max) plus a 1 px no-op probe. Every gesture shows
`drag_starts +1` and nonzero `pointer_move_calls`; `view_scale_calls` stays
0 across all of them (no view rebuild per move); the no-op leaves the layer
in place with only press/release bookkeeping revisions. See "Real-drag
evidence" below.

## Correctness evidence alongside the numbers

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` clean;
  **33 tests pass** (3 were added: no-op revision/clamping, captured drag
  scale, artwork-cache invalidation across edit/undo/redo/resize/typography).
- Live native GUI + Eye/Hand: `scripts/acceptance-044.ps1` passes all
  16 phases, 103 guarded operations, exit 0 on the final head.
- Export parity: the old-pin release export and both new-pin exports are
  byte-identical (sha256 `AB75C975...`, 158307 bytes) and their project JSON
  matches structurally. The single divergent export (`old-debug`,
  `D7C27882...`) is explained: that harness run selected a different layer
  before typing, so its *document* differs (the words landed on `channel`
  instead of `lineup`) - a selection difference in the script, not a
  rendering difference.

## Finishing slice: view-build root cause, one more correction, real drags

### The 5.177 -> 7.877 ms "regression" is a scenario-mix artifact

`src/bin/view-bench.rs` builds views in-process with warmup (40) and timed
iterations (120-200), reporting median/p95 per state, on both pins in
release. Two trials each:

| State (per-view p50) | old pin | new pin |
|---|---|---|
| Fit, no selection | 4.13-4.30 ms | 0.85-0.88 ms |
| Fit, selected | 4.50-4.57 ms | 1.03-1.09 ms |
| Typing (focused field) | 4.48-4.69 ms | 1.05-1.14 ms |
| Churn (zoom cycling + undo/redo between builds) | 4.44 ms | 1.04 ms |

Stage means per build (new pin): app construction ~0.33-0.44 ms,
layout ~0.17-0.28 ms, ghost ~0.06-0.08 ms, scene ~0.24-0.36 ms.
Old pin: layout ~2.6-2.8 ms, scene ~1.1-1.5 ms, app/ghost identical
(~0.4/0.07 ms). P95s track p50s within ~15% on both pins.

Conclusion: **there is no per-build view regression.** In matched states
with warmup, 044 builds views 4-5x faster; Specimen-side construction is
unchanged. The scenario averages (5.177 vs 7.877 ms, and 7.82 vs 10.35 ms
across two identical runs of the same binary) conflate UI-state mix,
cold/one-off effects and wall-clock preemption under harness load
(a CLI process is spawned per Eye op). A fourth bench phase that cycles
zoom and undo/redo between builds shows no churn effect (new-pin
p50 1.04 ms), ruling out revision churn as the inflator. The trustworthy
per-build comparison is the bench; the scenario totals measure
end-to-end operator cost, which is what users feel.

### Small correction: reuse the already-built view for ClickId/Type

Each Eye click/type built the chrome twice: once to hit-test, once inside
`press()`. `Host::press_in` reuses the existing view instead. On the
identical scripted session (release): **view calls 282 -> 219 (-22%),
view time 2221 -> 1713 ms (-508 ms)**, per-view unit cost unchanged
(7.877 -> 7.823 ms), zero failures. The remaining per-action builds (one
per key/map/detail/click) each do distinct hit-testing or serialization
work; removing them would need a shared view cache, which this slice
deliberately avoids.

### Real-drag evidence (`scripts/verify-drag.ps1`, release, exit 0)

| Gesture | Result | Counters | Input-to-visible-update |
|---|---|---|---|
| Text snap +53/+37 | landed (80,312) as computed (clamped) | starts +1, moves +8 | 23.3 ms avg, 5 frames |
| Shape snap +61/-43 | landed (192,464) exact | starts +1, moves +8 | 22.1 ms avg, 8 frames |
| Shape rapid +90/+60, 15 ms steps | landed (260,528) exact | starts +1, moves +3 | 22.3 ms avg, 3 frames |
| Text clamp-min (far outside) | clamped (0,0) exact | starts +1, moves +4 | 22.9 ms avg, 4 frames |
| Text clamp-max (far outside) | clamped (80,1084) exact | starts +1, moves +4 | 22.8 ms avg, 4 frames |
| 1 px no-op from snapped grid | unmoved; rev +3 press/release only | starts +1, moves +2 | n/a |

Every gesture also verified selection survival, single-undo-step restore
of the pre-drag position, and redo restore. Save/reopen round-trips;
`verify-artifact` on the final frame: **0 screen mismatches, export
matches canonical pixels** (900 x 1200). `view_scale_calls` stayed 0
throughout: no view rebuild per pointer move on the real path.

Input-to-visible-update is UI-thread time from a revision-changing
pointer move to the next presented frame (~22-23 ms in release, dominated
by the ~12 ms frame). It is not a monitor refresh rate, and the Eye
roundtrip adds its own process-spawn latency on top.

Harness lessons recorded in the script: serialize cursor delivery ahead
of the press with an Eye roundtrip (else the press can observe a stale
cursor and the drag lands ~90 px off); verify the press selected the
intended element (ring centres can belong to a smaller element painted
in front - probe corners first); compute expectations with clamping;
flatten Eye/document values to `[double]` at the call site.

### Rendering correctness on the final head

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` clean;
  **33 tests pass** (no-op revision/clamping, captured drag scale,
  artwork-cache invalidation across edit/undo/redo/resize/typography).
- `scripts/acceptance-044.ps1`: 16 phases, 103 operations, exit 0.
- `scripts/verify-drag.ps1`: exit 0 (above).
- Export parity: drag-run export is structurally identical to the
  template project with only the dragged-layer positions changed;
  `verify-artifact` confirms zero screen mismatches.

## Limits

- One machine; scenario cells are single runs (bench cells are repeated
  trials with warmup, median/p95 over 120-200 iterations x 2).
- Operator latency includes one CLI process spawn per call.
- Stage timers are UI-thread wall clock, not monitor frame rate.
- No GPU measurement: the host uses Hamesh's CPU renderer.
- Input-to-visible-update is measured to the presented frame, not to
  photons; no high-speed camera verification.

## Recommendation: a separate, modest visual-quality slice

Responsiveness is accepted at the measurement level. The next slice
should be visual quality, not performance: audit the poster for
typographic hierarchy, spacing rhythm and the overlapping bottom text
(`THE SIGNAL ROOM / LONDON` vs `FREQUENCY` in the starter), plus the
inspector's draft-until-Enter behaviour Matthew reported ("little
changing" - drafts give no preview). Keep it to presentation and
feedback copy; no host rewrite, no new controls, no framework changes.
