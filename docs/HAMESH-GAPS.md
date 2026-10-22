# Hamesh integration findings for Specimen

Inspected 22 October 2026 against upstream commit `9fa37c33a92620151e0fbfbd6e22c7ae32bccbf8` (published `hamesh/blocks-b1`). Inspection used `git show`, so Muse's uncommitted changes are not the baseline. This report is source evidence; it does not claim a rendered runtime reproduction or CPU/GPU equivalence.

## Reused source provenance

`src/inspect_srv.rs` is an exact copy of upstream `crates/hamesh-window/src/inspect_srv.rs`, Git blob `452a4561c767952470674979e8032e63b71ade92`.

`src/bin/hamesh.rs` is an exact copy of upstream `apps/hamesh-cli/src/main.rs`, Git blob `665cbba08979a881ff5f452240b3c2912ced747b`.

Both were extracted from commit `9fa37c33a92620151e0fbfbd6e22c7ae32bccbf8` in [Matthew's Hamesh repository](https://github.com/matthewjameswatkins1978-cyber/hameshgui). The upstream workspace declares `MIT OR Apache-2.0` in Cargo.toml and README; that commit contains no LICENSE or COPYING file to copy. Exact source identity was independently verified with `git hash-object` against the upstream blob IDs above. These files are transport adapters, not a second GUI framework. Future changes should preserve this provenance and explicitly document divergence.

## Integration capabilities and missing APIs

| Area | Verified source evidence | Consequence / narrow integration |
|---|---|---|
| Native host | `crates/hamesh-window/Cargo.toml` declares only `hamesh-window-demo`; `main.rs` privately declares `gpu` and `inspect_srv`. | Missing reusable host API. Specimen needs its own small winit/softbuffer host. Extracting the host/server into Hamesh is an upstream request, not an application framework rewrite. |
| Eye and Hand transport | `hamesh-inspect` exposes compact map/detail; the loopback JSONL server and winit proxy live in private `inspect_srv.rs`. CLI/MCP speak that wire format. | Reuse the real Eye map/detail and compatible transport. Keep external events serialized onto the same UI thread and handlers as OS input. |
| Application commands | `hamesh-automation::click_human` performs framework hit-testing and disabled checks, then private `apply_command` handles only `increment`/`reset`; other registered commands merely bump revision. | Missing generic host command dispatch. After a successful framework hit, dispatch the hit component's action into Specimen's canonical reducer. Never interpret the framework's success alone as proof the document changed. |
| Inspector editing | `key_human`/`type_human` use `hamesh_widgets::Widgets`, one composer/feed, and composer-specific key handling. | Missing general editable-field binding. Application field editing must use shared physical/inspector keyboard handlers. Targeted direct document writes cannot be called GUI interaction. |
| Blocks | Published builders include row/column/card/button/badge/icon/text/divider/spacer/progress/metric. No generic dropdown, colour picker, numeric input or canvas block at this baseline. | Use Blocks for available chrome; keep field bindings and canvas presentation at a replaceable application boundary. Coordinate future Blocks controls with Muse. |
| Typography | `hamesh_text::TextLayout::new(text,width,size)` pushes only `StyleProperty::FontSize`. Scene text carries size and token colour, not family/weight/tracking/leading/alignment. CPU text assumes an 8 px inset and vertical centring. | Missing styled typography/canvas-text API. A thin artwork renderer can use the same proven Parley/Swash stack with document styles; do not claim the current Hamesh text wrapper supplied these features. |
| Colour | `hamesh_theme::resolve` resolves a fixed semantic token table; unknown strings become surface colour. | Arbitrary artwork hex colours require document renderer handling. Setting `surface="#ff0000"` does not make a red Hamesh component at this baseline. |
| Rendering | CPU scene rendering is exported by `hamesh-render`; GPU host code is private to the demo binary. | A CPU application is supported. GPU parity remains unverified until a real GPU run and image comparison exist. |

## Candidate defects and reproducible cases for Muse

### Equal-z overlap order is unspecified

Source: `hamesh-layout/src/lib.rs::compute/hit_test` and `hamesh-scene/src/lib.rs::build`.

Overlay components use `overlay=true`, `ox`, `oy`, `w`, `h` to receive absolute **viewport** rectangles and fixed z=100. Normal z is tree depth. Scene creation collects a HashMap and sorts only by z; hit-testing iterates a separate HashMap and lets the final equal-z match win. No stable sibling order tie-breaker exists. This contradicts the hit-test comment promising "later in tree".

Reproduction: create two enabled overlay buttons A then B with identical rectangles, give each a distinct fill/action, compute the layout and scene, inspect the last painted overlay and hit-test their shared centre. Reconstruct the App in a loop or restart processes. Assert the final tree child B consistently paints and wins. The baseline cannot guarantee that assertion, and paint/hit ordering may disagree. This is a source-confirmed ordering defect; a captured runtime demonstration is still outstanding.

Specimen should avoid using overlapping equal-z overlay controls as its layer-order authority. A single Hamesh canvas node with domain element selection and document painter order is the narrowest integration. If exposing separate Eye element geometry, keep it read-only/ghost and derive it from the canonical canvas transform. Do not invent a parallel UI layout engine.

### Overlays still affect normal flex layout

Source: `hamesh-layout/src/lib.rs::child_style/compute`.

The Taffy style is built before overlay rectangle substitution; `child_style` does not exclude overlay nodes or set absolute positioning. Reproduction: create a column containing a growing canvas; compute its height. Add an overlay sibling with h=300 and the same viewport; recompute. The overlay still consumes/shrinks flex space even though its output rectangle is later moved to ox/oy. This is an observed implementation limitation; whether upstream intends overlays to leave flow is a product/API decision.

### Clipping truth is diagnostic, not ancestor paint clipping

Source: `hamesh-layout/src/lib.rs` marks `clipped_by_parent`; `hamesh-render/src/lib.rs::render` skips invisible/ghost nodes but does not apply an ancestor clip mask. Text clips only to its own rectangle. Hit-testing can also reach the child portion beyond its parent.

Reproduction: parent rect 100x100; child overlay rect x=80,y=20,w=100,h=40. Inspect the clipped flag, render, then hit-test x=150,y=30. The child can paint/hit outside the parent while the Eye reports clipping. Treat this as a source-confirmed rendering/interaction discrepancy against strict clipping expectations, not a visually reproduced fault. Specimen must keep all interactive chrome within its panels and its artwork within the canvas.

## First live application observation: intrinsic label sizing

The first native Specimen run produced `outputs/initial-window.png` and `work/initial-map.json`. Both were inspected independently. The screenshot shows header/toolbar button labels wrapping into narrow vertical strips; the Eye map gives `save`, `open`, `export` and `add-text` widths of **16 px**, and `toolbar-name`/`canvas-title` widths of **24 px**. Their reported `clipped` flags are false despite unusable text. This is runtime evidence of missing intrinsic text measurement/minimum sizing, not merely a proposed fault case.

Cause confirmed in source: Taffy receives no text measurement function; auto-width leaf controls size around padding rather than shaped text. The narrow Specimen fix is explicit supported `w` properties for row controls and labels, plus bounded panel heights. That keeps layout in Hamesh and is replaceable when Blocks adds measured preferred/minimum sizes. Font-aware preferred sizing remains an upstream framework capability request.

The initial map also placed `engine-label` at y=1034 for a 980 px-high captured frame while reporting `clipped=false`. Eye consumers should independently check viewport containment; immediate-parent clipping alone is insufficient evidence of reachability. This initial frame is diagnostic evidence, not the final visual acceptance result.

## Required end-to-end evidence

- Launch a real native Specimen window and discover it from a separate CLI/MCP process.
- Fetch Hamesh Eye map/detail; assert intended control is visible, enabled, unclipped and has useful geometry.
- Click through real Hamesh Hand hit-testing; key/type through the same focused field handler as physical input.
- Record mechanism, requested and actual hit, app/scene revision, command outcome and document state after each step.
- Deliberately introduce a bounded alignment error through controls, detect it from live geometry, correct through controls, then re-inspect.
- Activate ordinary Save and Export controls. Reload the project and compare exported pixel data with the canonical canvas artwork buffer.
- Capture the actual rendered application frame; visually inspect it and the exported artwork. A direct document/render test supplements this but cannot replace GUI proof.
- Reject stale revisions and confirm no document change. Confirm disabled/covered targets do not edit a document.
- Resize to supported minimum/default/large window sizes and check control bounds, text clipping and canvas aspect ratio.
- Keep CPU-only evidence explicit. Do not report GPU testing, AccessKit or live IME verification without a fresh real run.
