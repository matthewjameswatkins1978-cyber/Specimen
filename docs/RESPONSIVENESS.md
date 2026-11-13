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

**These two are justified by measurement and by construction, not by a
measured drag.** The harness could not drive a physical drag end to end in
this session: `perf.json` shows `pointer_move_calls = 0` and
`view_scale_calls = 0` in every run, even though the window reached the
foreground (`fg_ok = true`) and no operation failed. Drag responsiveness was
therefore **not** directly measured, and no drag-speed claim is made.

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

## Limits

- One machine, one run per cell; no repeated-trial statistics.
- Operator latency includes one CLI process spawn per call.
- Stage timers are UI-thread wall clock, not monitor frame rate.
- No GPU measurement: the host uses Hamesh's CPU renderer.
- The physical-drag path was not exercised (above).
