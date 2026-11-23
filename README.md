# `liuli` (`v0.2.0`) — Pure-Rust `#![no_std]` Embedded GUI Engine on `embedded-graphics`

`liuli` (琉璃) is a deterministic, zero-heap-allocation (`#![no_std]`, `#![forbid(unsafe_code)]`) embedded GUI engine and 3-page Chinese terminal demo targeting `embedded-graphics` (`0.8.2`) / `embedded-graphics-core` (`0.4.1`) RGB565 displays and Cortex-M microcontrollers within a **128 KiB static RAM budget** (`73,248 B` measured on `thumbv7em-none-eabihf`, `0 B` `.data`).

## Highlights (`v0.2.0`)

- **Strict Resource & Rollback Determinism**: Pre-allocated `.bss` `Storage` (`65,088 B` host / `63,008 B` Thumb, `RetainedNode = 224 B`, `PaintCmd = 48 B`) with scoped closure builders (`out.root(..., |ui| ...)`), atomic `ViewDraft` rollback, and active DMA frame locks (`Engine::is_dma_busy`, `try_dispatch`, `try_with_model`).
- **Dual-Font CJK A4 Bitmap System (`LF4 v1`)**: Embedded 16 px `Body16` (`settings.lf4`, `221` glyphs, `16,331 B`) and 12 px `Small12` (`small12.lf4`, `178` glyphs, `10,915 B`) — `27,246 B` combined (`<= 32 KiB`) with `TextAlign::{Start, Center, End}`, CRLF, and ellipsis truncation.
- **Richer Controls, Easing & Multi-Page Navigation**: `ProgressBar`, zero-alloc `Sparkline` (`SampleRing<32>`), 11 vector icons, `EaseOutCubic` switch tracks, long-press key/button repeat (`400 ms / 80 ms`), independent per-page scroll & focus memory (`TerminalApp`: `Dashboard` / `Settings` / `Diagnostics`), confirmation modal, and click-through `Toast`.
- **`embedded-graphics` Strip + Hardened SPI/DMA Pipeline**: Renders dirty regions via `BandTarget` (`320×8×u16 = 5,120 B` per band), supporting synchronous `SpiWindowTarget` (64 B wire chunking, BE/LE RGB565, frame flush) and compile-time-safe ping-pong DMA (`AlignedBand`, cache-line ops, `TeGate`, guarded fault recovery, `FreeBand -> ReadyBand -> InFlightBand`).
- **Pure-Rust Host Simulator & Bare-Metal Audits**: Unified `SimSession`, P6 PPM + RGB8 PNG codecs, strict event replay CLI (`settings.events` & `terminal.events`, 12 golden PPMs), localhost time-traveling HTTP preview (`127.0.0.1`, read-only `GET`, `POST /clock`, Damage/Band overlays), plus Cortex-M4F ELF (`FlashSpan = 106,976 B`, `.data = 0 B`, `StaticRAM = 73,248 B`) and `llvm-objdump` stack chain (`4,688 B <= 6,144 B`) checks.

## Commands

Run from workspace root:

```powershell
# Lockfile freeze, formatting, unit/integration tests, doctests (compile_fail proofs)
.\checks\audit-locks.ps1
cargo fmt --all -- --check
cargo fmt --manifest-path checks/firmware/Cargo.toml -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --release --locked
cargo test -p liuli-core --doc --locked
cargo test -p liuli-render --doc --locked

# Single-threaded zero-allocation audit and feature tree inspection
cargo test -p liuli-sim --test allocation --release --locked -- --test-threads=1
cargo tree -p liuli-settings -e features --locked

# Dual-slot font verification, headless PPM output, v0.1 & v0.2 replays (PNG)
cargo run -p liuli-fontpack --locked -- verify
cargo run -p liuli-sim --example headless --locked
cargo run -p liuli-sim --locked -- replay host/liuli-sim/tests/fixtures/settings.events --out target/liuli-v01 --format png
cargo run -p liuli-sim --locked -- replay host/liuli-sim/tests/fixtures/terminal.events --out target/liuli-v02 --format png

# Interactive browser preview (opens HTTP server on http://127.0.0.1:7878/)
cargo run -p liuli-sim --locked -- preview --scenario terminal --port 7878

# Bare-metal Cortex-M4F check, ELF section/BSS/stack audits, and StartupProbe (.data copy)
rustup target add thumbv7em-none-eabihf
cargo check -p liuli-settings --target thumbv7em-none-eabihf --no-default-features --locked
Push-Location checks/firmware
cargo build --target thumbv7em-none-eabihf --release --locked
Pop-Location
.\checks\audit-firmware.ps1 -Elf checks/firmware/target/thumbv7em-none-eabihf/release/liuli-firmware -Profile Demo
.\checks\audit-stack.ps1 -Elf checks/firmware/target/thumbv7em-none-eabihf/release/liuli-firmware -MaxThreadBytes 6144
```

## Documentation

- [Architecture Overview](docs/architecture.md)
- [LF4 Bitmap Font Format](docs/font-format.md)
- [Event Replay Script Format](docs/replay-format.md)
- [Memory & Transfer Budget](docs/memory-budget.md)

## Design Boundaries (`v0.2.0`)

- **Horizontal LTR CJK + ASCII text**: Fixed dual bitmap slots (`Body16` & `Small12`); complex BiDi/shaping (Arabic/Indic/emoji sequences) remains out of scope.
- **Single Modal Layer & Single Pointer**: Bounded to `MAX_MODALS = 1` active modal dialog and single active touch pointer ID (`Toast` overlays sit non-focusable and click-through).
- **Engine Maturity vs. Board Hardware Sign-off**: `liuli` v0.2 delivers verified engine, simulator, dual-font, protocol-model, ELF section/stack, and reachable Thumb firmware gates; integrating into a physical MCU board still requires board-specific BSP sign-off for cold-boot silicon init, physical DMA/SPI timing, TE polarity/jitter, cache-line maintenance, and worst-case IRQ stack nesting.

## License

Dual-licensed under [Apache-2.0 or MIT](LICENSE). Font atlas generated from SIL OFL 1.1 Noto Sans CJK SC ([FONT-LICENSE.txt](tools/liuli-fontpack/source/FONT-LICENSE.txt)).
