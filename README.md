# Specimen

A Windows typography and graphic composition studio built with Hamesh.
Three editable starter compositions, text and geometric layers, typography
and colour controls, canvas selection/dragging, alignment, stacking,
100-step undo/redo, native JSON projects and full-resolution PNG export.

## Run

Build the portable bundle with `scripts/package-portable.ps1`, unzip it and double-click **Launch Specimen.cmd**.
It opens the native application and keeps projects/evidence beside the program.
No Rust toolchain is needed for the portable build. Source development:

```powershell
cargo run --bin specimen
cargo run --bin specimen -- --project=outputs/after-frequency.specimen.json
```

Rust and the Windows MSVC build tools are required to build from source.
Cargo fetches the exact Hamesh revision pinned in Cargo.toml. This app does not
depend on Muse's mutable working checkout.

## Editing

Select a starter in the design library, or choose New blank canvas.
Add text, rectangles or ellipses with the toolbar. Select artwork by clicking
the canvas or a layer button. Drag to move; guides enable 8 px snapping.
Arrow keys nudge the selected layer by one document pixel.

The inspector has Type, Layout and Canvas tabs. Click a field, type a replacement
and press Enter to apply. Escape cancels a draft. Fields also support caret
navigation and Backspace/Delete. Shift+Enter inserts a newline in text content.
Ctrl+A selects the field contents; Ctrl+C/X copies/cuts the whole field;
Ctrl+V pastes. The installed family list is discovered at startup. Type a known
family or use Next available font. Platform fallback handles missing glyphs;
inspection reports unavailable requested fonts and missing glyphs.

Layout controls set pixel geometry, align to artboard edges/centre, move a layer
to front/back, duplicate or delete. The artwork dimensions stay fixed when the
window resizes; the canvas fits the available space.

Set a project path in Canvas settings before Save/Open. Save and Ctrl+S write
the project; Open and Ctrl+O reopen the current path. The library also opens
saved projects in that directory. This version uses an editable path field,
rather than a native file picker. An open or template switch can be undone.
Ctrl+Z undoes; Ctrl+Shift+Z or Ctrl+Y redoes.

Export PNG or Ctrl+E writes beside the project. For `name.specimen.json` the
export is `name.png`; other project filenames receive an additional `.png`
suffix. Export refuses to replace non-PNG files, directories or symlinks.
PNG replacement is staged, validated, synced and atomically renamed.
Closing a dirty document retains a uniquely named recovery project in the
evidence directory. Uncommitted field drafts are not saved.

## Eye and Hand

The native window starts a token-gated loopback inspector and writes discovery
to `target/hamesh-inspect.json`. The included `hamesh.exe` is the upstream CLI:

```powershell
.\target\debug\hamesh.exe map
.\target\debug\hamesh.exe detail canvas
.\target\debug\hamesh.exe click add-text
.\target\debug\hamesh.exe type field-text "NEW SIGNAL"
.\target\debug\hamesh.exe key Enter
.\target\debug\hamesh.exe snapshot outputs/window.png
```

Use map first, then details on demand. Click/key accept `--expect-rev N` before
the operation. A stale call is rejected without a state change. The reused
upstream type message lacks that guard; serialize it and inspect afterwards.
Artwork element nodes (`art-ID`) expose read-only document and screen geometry;
select through the canvas or layer controls.

Physical input and external interaction share the same Hamesh hit tests and
application handlers. Each trace labels the interaction mechanism.
`scripts/creative-demo.ps1` is the separate-process creative acceptance workflow.

## Direct document tools

```powershell
cargo run -- --fonts
cargo run -- --check outputs/after-frequency.specimen.json
cargo run -- --render outputs/after-frequency.specimen.json work/re-render.png
cargo run -- --render-template=0 work/starter.png
```

These are explicitly direct document operations, not GUI proof.

## Verification

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release --bins
cargo run --bin hamesh-probe
```

Tests cover model/persistence/rendering and 27 workspace configurations.
The ignored starter generator is an optional artifact-producing diagnostic;
it is not an untested acceptance feature. Live GUI evidence and direct-document
evidence are separated in the [delivery report](docs/DELIVERY.md).

## Boundaries

The first host uses Hamesh's CPU renderer. GPU comparison, AccessKit and live
IME verification remain unverified. There is no image editing, vector pen,
rotation, font installation, cloud service or embedded AI chat.
Canvas selection uses frontmost element bounding boxes. The library shows a
small paged layer list and two saved projects from the selected directory.
Very long inspector text is clipped to its field; full text is retained in
the model and Eye.

See [architecture](docs/ARCHITECTURE.md), [native format](docs/FORMAT.md) and
[Hamesh findings](docs/HAMESH-GAPS.md). Hamesh is pinned to
`9fa37c33a92620151e0fbfbd6e22c7ae32bccbf8`; no framework merge or release is part
of this application.

Original Specimen application code is MIT licensed. Reused upstream inspector
transport and CLI retain Hamesh's declared MIT OR Apache-2.0 licensing; see
the provenance report.
