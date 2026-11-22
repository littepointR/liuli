# `liuli` — Pure-Rust `#![no_std]` Embedded GUI Engine on `embedded-graphics`

`liuli` (琉璃) is a deterministic, zero-heap-allocation (`#![no_std]`, `#![forbid(unsafe_code)]`) embedded GUI engine and Chinese settings demo targeting `embedded-graphics` (`0.8.2`) / `embedded-graphics-core` (`0.4.1`) RGB565 displays and microcontrollers within a **128 KiB static RAM budget**.

## Highlights

- **Strict Resource Determinism**: Pre-allocated `Storage` (`MAX_NODES = 64`, `MAX_COMMANDS = 256`, `MAX_TEXT_BYTES = 1024`, `MAX_DEPTH = 12`) with zero runtime heap allocations and compile-time/runtime overflow errors that roll back transactions atomically.
- **`embedded-graphics` Strip Architecture**: Renders incremental dirty bands (`BandTarget: DrawTarget<Color = Rgb565>`) into a small strip buffer (`320×8×u16 = 5,120 B`), supporting both synchronous `present_sync` (`DrawTarget`) and move-semantic ping-pong DMA ownership (`FreeBand` -> `ReadyBand` -> `InFlightBand`).
- **Offline CJK A4 Bitmap Font (`LF4 v1`)**: Embedded 16 px Noto Sans CJK SC font (`settings.lf4`, 11,699 bytes, 187 glyphs) packed reproducibly from 4x supersampled PBM/TSV with exact integer RGB565 alpha blending (`a = 0..15`).
- **Power-Aware Scheduling**: Monotonic `InstantMs` virtual timer + linear 160 ms animation tracks that converge cleanly to `NextWakeup::OnEvent` (zero wakeups when idle).
- **Zero Native C/Windowing Dependencies**: Pure-Rust host simulator (`liuli-sim`) with independent fullscreen reference renderer, P6 PPM + RGB8 PNG codecs, deterministic event replay CLI, and localhost HTTP preview server (`127.0.0.1`).

## Commands

Run from workspace root:

```powershell
# Format, check, unit & integration tests, doctests (including compile_fail DMA ownership)
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo test -p liuli-render --doc --locked

# Single-threaded zero-allocation audit and feature tree inspection
cargo test -p liuli-sim --test allocation --locked -- --test-threads=1
cargo tree -p liuli-settings -e features --locked

# Font verification, headless PPM output, full settings replay (PNG)
cargo run -p liuli-fontpack --locked -- verify
cargo run -p liuli-sim --example headless --locked
cargo run -p liuli-sim --locked -- replay host/liuli-sim/tests/fixtures/settings.events --out target/liuli-demo --format png

# Interactive browser preview (opens HTTP server on http://127.0.0.1:7878/)
cargo run -p liuli-sim --locked -- preview

# Bare-metal Cortex-M4F check & static RAM/stack linker verification
rustup target add thumbv7em-none-eabihf
cargo check -p liuli-settings --target thumbv7em-none-eabihf --no-default-features --locked
cargo build --manifest-path checks/firmware/Cargo.toml --target thumbv7em-none-eabihf --release --locked
```

## Documentation

- [Architecture Overview](docs/architecture.md)
- [LF4 Bitmap Font Format](docs/font-format.md)
- [Event Replay Script Format](docs/replay-format.md)
- [Memory & Transfer Budget](docs/memory-budget.md)

## Design Boundaries (v0.1)

- **Horizontal LTR CJK + ASCII text**: Complex BiDi/shaping (Arabic/Indic/emoji sequences) and variable font sizes are out of scope for v0.1.
- **Single Modal Layer & Single Pointer**: Bounded to 1 active modal dialog and single active touch pointer ID (`MAX_MODALS = 1`).
- **BSP Integration Scope**: Physical SPI/LTDC cache maintenance, pin/clocks, and display TE vsync tearing prevention remain the board support package's responsibility behind `DmaSink` and `DrawTarget`.

## License

Dual-licensed under [Apache-2.0 or MIT](LICENSE). Font atlas generated from SIL OFL 1.1 Noto Sans CJK SC ([FONT-LICENSE.txt](tools/liuli-fontpack/source/FONT-LICENSE.txt)).
