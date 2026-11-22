# `liuli` Architecture

`liuli` is a pure-Rust `#![no_std]`, zero-`alloc`, `#![forbid(unsafe_code)]` embedded GUI engine built on `embedded-graphics-core` (`0.4.1`) and `embedded-graphics` (`0.8.2`), paired with a pure-Rust host simulator and reproducible CJK bitmap font packer.

## Workspace Layout

- `crates/liuli-core`: Fixed-capacity retained tree (`MAX_NODES = 64`, `MAX_DEPTH = 12`, `MAX_TEXT_BYTES = 1024`), bounded-stack layout engine, key-based reconciliation, damage tracking (`MAX_DAMAGE = 12`), flat paint command list (`MAX_COMMANDS = 256`), unified input router, focus/modal trapping, monotonic animations (`MAX_ANIMATIONS = 8`), and virtual-timer state.
- `crates/liuli-text`: LF4 v1 binary bitmap font parser, glyph iterator/line wrapper (`TextMeasurer`), and RGB565 4-bit coverage (`a = 0..15`) channel blender.
- `crates/liuli-render`: Strip buffer (`BandTarget` implementing `OriginDimensions` + `DrawTarget<Color = Rgb565>`), scene rasterizer, synchronous `DrawTarget` uploader (`present_sync` / `present_sync_with`), and compile-time-safe ping-pong DMA ownership protocol (`FreeBand` -> `ReadyBand` -> `InFlightBand`).
- `crates/liuli`: Pre-allocated `Storage` + `Engine<A: App>` orchestrating atomic `ViewDraft` commit, incremental diffing, animation/timer scheduling (`NextWakeup`), and frame presentation.
- `apps/liuli-settings`: 320x240 Chinese device settings page implementing `App` (`SettingsApp`), light/dark RGB565 themes, touch & rotary-knob interaction, temperature sampling timer, and confirmation modal.
- `tools/liuli-fontpack`: Reproducible 4x-supersampled PBM + TSV -> LF4 v1 packer and verifier (`pack`, `verify`).
- `host/liuli-sim`: Host `MemoryScreen` (`DrawTarget`), independent fullscreen reference renderer, pure-Rust P6 PPM & RGB8 PNG codec, deterministic replay CLI, and `127.0.0.1` HTTP `PreviewServer`.
- `checks/firmware`: Standalone bare-metal `thumbv7em-none-eabihf` binary linking `liuli-settings` + `liuli` with `rust-lld` and enforcing the 128 KiB static RAM + 8 KiB stack assertions.

## Transaction & Rendering Pipeline

1. **Pure View Transaction**: `App::view(&model, &mut ViewBuilder)` writes into `Storage.draft`. Structural errors (`DuplicateKey`, `UnmatchedEnd`, `UnclosedContainer`, `NestedModal`, `InvalidScrollFill`, overflow) abort before modifying the active tree, commands, focus, or pointer capture.
2. **Iterative Reconciliation & Layout**: `reconcile_into` preserves slots and `NodeId` generations for matching `(Key, NodeKind)` pairs, frees deleted slots (monotonic `generation_hwm` without wraparound), applies the same-metrics text fast path to skip parent relayout when glyph dimensions fit cleanly inside fixed bounds, and merges dirty rectangles into `DamageSet`.
3. **Strip Rendering**: `DamageSet` splits each dirty region into horizontal bands (`BandIter`). Every band initializes from opaque `root_bg` and replays intersecting `PaintCmd`s in z-order into a caller-provided `&mut [u16]` slice (`BandTarget`).
4. **Output Separation**: Synchronous `DrawTarget` targets upload bands via `fill_contiguous` (with optional `finish` flush boundary). Asynchronous DMA targets pass `ReadyBand` ownership into `DmaSink::try_start`; safe code cannot mutate or reclaim the slice until `poll_complete` returns a confirmed-stopped `FreeBand` matching `(frame, seq, slot)`.
5. **Monotonic Time & Sleep**: Platform supplies `InstantMs(u64)`. `Engine::next_wakeup` computes `Now`, `At(t)` (earliest of animation tick and app deadline), or `OnEvent` (zero periodic polling when monitoring and animations are idle).
