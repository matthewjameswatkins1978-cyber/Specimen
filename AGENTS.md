# Specimen
Independent Rust application. Never edit the Hamesh working tree as part of this app.
Hamesh is pinned in Cargo.toml; update deliberately after integration evidence.
Canonical document/history: src/document.rs. Canonical artwork raster: src/artwork.rs.
Hamesh component tree/layout/scene/Blocks: src/editor.rs.
Native host and common physical/external event routing: src/main.rs.
The upstream inspector transport is attributed in src/inspect_srv.rs.
Use Threadmoth preview, mutate, certificate for supported edits.
Checks: cargo fmt --check; cargo test; cargo clippy --all-targets -- -D warnings.
GUI acceptance must use the live native window via Eye/Hand. Document tests are not GUI evidence.
Preserve outputs evidence. Do not claim GPU parity: initial host uses Hamesh CPU renderer.
