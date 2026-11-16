# Moving to a newer API layer

The layer is the `// LAYER N` line at the top of `tl/api.tl`; `LAYER` in this crate and the
`initConnection` / `invokeWithLayer` wrappers of `mtg-mtsender` follow it. Layers are listed at
<https://core.telegram.org/api/layers>.

1. `./update-layer.sh [FILE_OR_URL]` takes the scheme (default: tdesktop's `api.tl`), refuses an
   older or equal layer (`FORCE=1` to override), prints the removed, added and changed
   constructors, replaces `tl/api.tl` and builds this crate. Read the list: removed and changed
   constructors are what breaks downstream.
2. `cargo build --workspace --all-targets --all-features`. The compiler points at every use of a
   removed or changed constructor in `mtg-client` (wrappers in `src/`), `mtg-session`
   (message box, adaptors) and `mtg-secret` / `mtg-calls`. Fix them, one commit per crate.
3. Check by hand what the compiler cannot see:
   - new flags that change behaviour of things we already call (read the layer's changelog);
   - `mtg-secret` has its own secret-chat layer (`schema::LAYER`) and `mtg-calls` its call
     library layers: they move only when the official client moves them, not with `api.tl`;
   - tests that hold a constructor id as a number (`mtg-testkit`: `invokeWithLayer` 0xda9b0d0d)
     are not affected by the layer, but fixtures built from changed types are.
4. `cargo test --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
5. Push `mtg`, then in the client repos bump the `rev` of the `mtg-*` dependencies together
   (`core/Cargo.toml`, `ferrogram-android/Cargo.toml`) and change the layer number in
   `ferrogram/CLAUDE.md`. The reference client checkout (`../Telegram`) should be at the same layer.
6. A manual run on the test DC (`MTG_T2_PLAN.md`) is the check a fake server cannot give: a layer
   Telegram no longer serves answers `CONNECTION_LAYER_INVALID`.
