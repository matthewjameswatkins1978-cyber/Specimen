# Specimen delivery report

Delivered 22 October 2026 from `D:\Projects\Specimen-app`
(repository `matthewjameswatkins1978-cyber/Specimen`, branch `main`).
This is the delivery report referenced by the README.

## What actually works

- Three-area workspace: design library (three starters with rendered
  thumbnails, New blank canvas, saved-project opening), live artboard with
  fit/scale, and the Type/Layout/Canvas inspector.
- Add text, rectangle and ellipse layers; select by canvas click or layer
  button; drag with 8 px guide snapping; arrow-key nudge; align to
  edges/centre; front/back stacking; duplicate and delete.
- Typography controls (family, weight, size, tracking, leading, alignment),
  colour controls, per-layer geometry; fixed artwork dimensions while the
  window resizes.
- 100-step transactional undo/redo; a drag is one history step; uncommitted
  field drafts are distinct from committed document state.
- Native documented project format (`docs/FORMAT.md`), atomic save, Open/Ctrl+O,
  recovery project on closing a dirty document.
- Atomic, validated full-resolution PNG export (900 x 1200 for the starters).
- System font discovery at startup, platform fallback for missing glyphs,
  unavailable fonts reported through inspection.
- AI operation: token-gated loopback inspector plus the external `hamesh.exe`
  CLI (map / detail / click / key / type / snapshot / wait) over the same
  hit tests and input handlers as physical interaction.

## Acceptance evidence (packet sections 6 and 10)

- `outputs/creative-agent-report.md` and `outputs/creative-interactions.jsonl`:
  separate-process creative run, 137 complete CLI requests/replies, every
  document change through real canvas hit-testing, focused inspector fields
  or ordinary Save/Export buttons. No direct document mutation was used.
  A deliberate 40 px alignment error was introduced, detected from Eye
  geometry, corrected through the inspector and re-verified to a shared edge.
- `outputs/artifact-verification.json`: export matches canonical pixels,
  378,784 screen pixels compared with 0 mismatches, 0 missing glyphs,
  0 text overflows, canvas aspect correct.
- `outputs/after-frequency.png` (exported artwork),
  `outputs/specimen-window.png` (final application frame),
  `outputs/alignment-trial.png` (the deliberate error before correction),
  `outputs/after-frequency.specimen.json` (project file).
- Visual inspection by the development agent on 22 October 2026: the exported
  artwork and the live application frame were both viewed; hierarchy,
  alignment and balance are coherent and read as a designed poster, not
  scattered text boxes. No human visual debugging was requested during the
  creative run.

## Checks run (re-run after the repository move, 22 October 2026)

| Check | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo test` | 14 passed, 0 failed, 1 ignored (documented starter generator) |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo build --release --bins` | pass |
| Portable bundle smoke test | pass (see below) |

Earlier run artefacts from the original location are preserved in
`outputs/format-check.txt`, `outputs/test-results.txt`,
`outputs/clippy-results.txt` and `outputs/release-build.txt`.

## Portable Windows bundle

`scripts/package-portable.ps1` builds `dist/Specimen-portable-win64.zip`
(specimen.exe, hamesh.exe, `Launch Specimen.cmd`, starter project, README).
Smoke test performed from a clean extraction of that zip: the launcher started
the real window, `hamesh map` returned the Eye tree, `hamesh snapshot`
captured the frame, evidence was written beside the program, and the app
exited with code 0 at its smoke deadline. `dist/` is gitignored; the zip is published as GitHub release
[v0.1.0](https://github.com/matthewjameswatkins1978-cyber/Specimen/releases/tag/v0.1.0).

## Hamesh capabilities exercised

Blocks chrome composition, layout/scene with overlays and ghost artwork
nodes, grapheme-aware Composer field editing, Eye map/detail serialization,
Hand click/key/type through real hit testing with revision guards
(click/key reject stale revisions without changing state), diagnostic trace
and snapshot capture, and the CPU scene renderer.

Framework gaps, defects and reproducible cases recorded for Muse are in
`docs/HAMESH-GAPS.md`: missing reusable host API, missing generic command
dispatch, no styled typography/canvas-text API, fixed colour token table,
unspecified equal-z overlay ordering, overlays consuming flex space,
diagnostic-only clipping, missing intrinsic label sizing (observed live in
the first run and worked around with explicit widths), and the upstream type
operation lacking a revision guard.

## Remaining limitations

- CPU renderer only: GPU parity, AccessKit and live IME are unverified and
  are not claimed.
- No native file picker (editable path field), no image editing, vector pen,
  rotation, font installation or embedded AI chat - all deliberate exclusions.
- Evidence directories (`outputs/`, `work/`) remain gitignored and local.

## Human decisions genuinely required

1. Publish the portable zip as a GitHub release (externally visible) -
  **done**: [v0.1.0](https://github.com/matthewjameswatkins1978-cyber/Specimen/releases/tag/v0.1.0),
  published 22 October 2026.
2. Delete the original Codex source folder now that the move is verified.
3. Optionally scope a GPU-parity run if GPU evidence is wanted.
