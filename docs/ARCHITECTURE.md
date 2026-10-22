# Application boundaries

Specimen is a separate application repository. Its Hamesh dependency is pinned
to public commit 9fa37c33a92620151e0fbfbd6e22c7ae32bccbf8; Muse's working checkout
is neither a dependency path nor a mutation target.

- `document.rs`: validated versioned value model, atomic persistence and bounded
  transactional history. This is the mutable document authority.
- `artwork.rs`: system font inventory, Parley shaping, Swash glyph rasterization,
  tiny-skia shapes and one full-resolution artwork buffer. Font/style handling
  is an explicitly documented bridge over Hamesh's current size-only wrapper.
- `editor.rs`: application actions and field bindings, Hamesh Blocks composition,
  Hamesh layout/scene, selection and document-coordinate canvas transform.
  Inspector text uses Hamesh's grapheme-aware Composer. Draft editing is distinct
  from validated committed document state. A drag is one history transaction.
- `main.rs`: winit native window, Hamesh CPU scene presentation through
  softbuffer, physical/external event convergence, diagnostics and snapshots.
- `inspect_srv.rs` and `bin/hamesh.rs`: reused upstream transport and external
  CLI. See HAMESH-GAPS.md for exact source identities and licensing provenance.
- `editor_tests.rs`: controller/layout checks, clearly separate from live GUI
  acceptance.

Hamesh owns all application chrome geometry, rendering, component identity,
hit-testing and Eye serialization. An artwork canvas is application content,
not a second widget/layout framework. Read-only ghost artwork nodes are added
to the Eye from the canonical document-to-canvas transform after measuring
chrome, avoiding Hamesh's still-in-flow overlays. They never intercept input.
Canvas selection is document paint order; layer buttons use Hamesh hit tests.

A physical click and an external Hand click both call Host::press, then
Editor::press, then hamesh_automation::click_human. The hit component goes
through the same application action handler. Physical text and external type
share Editor::insert and Hamesh Composer. External type must first reach the
ordinary input through actual hit testing. Every external state change is
serialized on the UI thread. There is no hidden set-document automation
endpoint.

Map/detail/snapshot are observations. Click/key/type are human-equivalent
events. `--render`, `--render-template`, `--check` and model tests are direct
document operations and must not be used as GUI proof. The reused protocol
guards click/key with optional expected revisions; upstream type lacks that
guard and callers must serialize it and inspect the result.

The host currently uses CPU rendering only. No GPU parity, AccessKit or live
IME acceptance is claimed. These and other integration requests are recorded
without modifying or masking upstream source defects.
