# 琉璃 · MCU UI 框架

**琉璃 1.0.0** 提供可分别安装的桌面 SDK 和参考板固件包。基础档使用有界的 Rust 状态和 RGB565 条带渲染。增强档在 ESP32-S3 上增加 PSRAM 字库缓存和像素混合加速。两个配置共用应用的 `App::view` 和 `App::update`。

| 从这里开始 | 内容 |
|---|---|
| [Widget Demo 全控件演示](docs/widget-demo.md) | 直接运行桌面程序，或刷入独立的参考板演示固件 |
| [独立 SDK 与组件教程](docs/sdk.md) | 离线安装、应用模板、控件、主题、字体、回放和事务恢复 |
| [参考板指南](docs/waveshare-s3.md) | 构建、分区、烧录、输入、接线与恢复 |
| [硬件与发布记录](docs/qualification.md) | 测量边界、实板结果和 1.0 验收状态 |
| [底层架构](docs/architecture.md) | 树、布局、按条带绘制和 DMA |

在仓库或解压后的 SDK 根目录运行 `python tools/check-ui.py`，即可离线回归控件、焦点框、告警触摸流程、长按、输入法和诊断工具。加上 `--screenshots shots` 可保存本次控件画面。三平台打包 CI 执行相同命令。[编写自己的回放](docs/sdk.md#无人值守的-ui-回归)

从源码运行产品应用：

```sh
cargo run -p liuli-alarms --example preview -- apps/liuli-alarms/assets/body.lf4
cargo test -p liuli-alarms --release
cargo run -p liuli-fontpack -- verify --project apps/liuli-alarms/assets/fonts.toml
cargo fetch --locked
python checks/package-sdk.py --fetch-toolchain-inputs
```

以下保留 v0.2 的两个兼容应用、回放入口及其历史基线。

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

## Historical Design Boundaries (`v0.2.0`)

- **Horizontal LTR CJK + ASCII text**: Fixed dual bitmap slots (`Body16` & `Small12`); complex BiDi/shaping (Arabic/Indic/emoji sequences) remains out of scope.
- **Single Modal Layer & Single Pointer**: Bounded to `MAX_MODALS = 1` active modal dialog and single active touch pointer ID (`Toast` overlays sit non-focusable and click-through).
- **Engine Maturity vs. Board Hardware Sign-off**: `liuli` v0.2 delivers verified engine, simulator, dual-font, protocol-model, ELF section/stack, and reachable Thumb firmware gates; integrating into a physical MCU board still requires board-specific BSP sign-off for cold-boot silicon init, physical DMA/SPI timing, TE polarity/jitter, cache-line maintenance, and worst-case IRQ stack nesting.

## License

Code is licensed under [MIT](LICENSE). Font atlas generated from SIL OFL 1.1 Noto Sans CJK SC ([FONT-LICENSE.txt](tools/liuli-fontpack/source/FONT-LICENSE.txt)).
