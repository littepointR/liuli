# Memory & Transfer Budget Verification (`v0.2.0`)

All core runtime structures live in fixed-capacity buffers with zero heap allocation (`#![no_std]`, zero `alloc` dependency) and zero-initialized `.bss` storage on bare-metal targets.

## 1. Host `size_of` Breakdown (`x86_64-pc-windows-msvc`)

Measured directly by `cargo test -p liuli --test capacity --locked -- --nocapture`:

| Item | Size (bytes) | Notes |
|---|---|---|
| `RetainedNode` | 224 | 64 slots inside `RetainedTree` |
| `RetainedTree` | 15,760 | Active + candidate trees (`2 × 15,760 = 31,520 B`) |
| `ViewDraft` | 7,312 | Single scoped builder transaction buffer |
| `PaintCmd` | 48 | `256` max commands per `CommandList` |
| `CommandList` | 12,320 | Active + candidate lists (`2 × 12,320 = 24,640 B`) |
| `Storage` (total struct) | 65,088 | Trees, commands, draft, `DamageSet`×2, `AnimPool`, focus/pointer/repeat/clock |
| Dual DMA band buffers (`2 × 320 × 8 × u16`) | 10,240 | 32-byte aligned ping-pong DMA pixel strips (`AlignedBand<2560>`) |
| **Total core working RAM** | **75,328** | **73.56 KiB <= 131,072 B (128 KiB hard limit)** |

Notes:
- Synchronous 8-bit SPI targets (`SpiWindowTarget`) need only a small caller-provided chunk buffer (`RECOMMENDED_SPI_CHUNK_BYTES = 64 B`).
- Asynchronous 8-bit SPI DMA targets whose hardware lacks 16-bit word swap mode require an additional `10,240 B` BSP-owned byte staging pair outside `ReadyBand`/`InFlightBand`, accounted separately in that board's firmware RAM budget.
- Host-only simulation buffers (`MemoryScreen` `320 × 240 × u16` = `153,600 B`, PNG/PPM Vecs, HTTP buffers) exist solely in `host/liuli-sim` and are excluded from firmware RAM.

## 2. Bare-Metal ELF Section & Stack Audit (`thumbv7em-none-eabihf`)

Built from `checks/firmware` with `--target thumbv7em-none-eabihf --release --locked` (`opt-level = "s"`, `lto = true`, `panic = "abort"`, `codegen-units = 1`, linker `rust-lld`, `FLASH = 256K`, `RAM = 136K`), linking `TerminalApp` (`AuditSink` DMA + `SpiWindowTarget` sync paths):

### Demo Profile (`liuli-firmware v0.2.0`, entry `0x08000009`)

| Region / Section | VMA | Size (bytes) | Notes |
|---|---|---|---|
| `.vector_table` (Flash) | `0x08000000` | 8 | Reset MSP (`0x20022000`, 8-byte aligned) + `Reset` vector |
| `.text` (Flash) | `0x08000008` | 78,476 | Startup, `TerminalApp` 3-page demo, layout, widgets, dual LF4 text, sync SPI & DMA paths |
| `.rodata` (Flash) | `0x08013298` | 28,472 | Includes `settings.lf4` (`16,331 B`) + `small12.lf4` (`10,915 B`) = `27,246 B` |
| `.ARM.exidx` (Flash) | `0x0801a1d0` | 16 | ARM index table |
| **Total Flash `PT_LOAD` span** | — | **106,976** | **104.47 KiB <= 196,608 B target (`262,144 B` hard limit)** |
| `.data` (RAM) | `0x20000000` | 0 | Zero initialized static `.data` (`<= 1,024 B` limit) |
| `.bss` (RAM) | `0x20000000` | 73,248 | `LIULI_DMA_BAND_A` (`5,120 B`) + `LIULI_DMA_BAND_B` (`5,120 B`) + `LIULI_STORAGE` (`63,008 B`) |
| **Total static RAM** | — | **73,248** | **71.53 KiB <= 81,920 B Demo budget (`131,072 B` hard limit)** |
| **Reserved stack (`_stack_start - _ram_used_end`)** | — | **66,016** | **64.47 KiB >= 8,192 B minimum (`link.x` & `audit-firmware.ps1`)** |

### StartupProbe Profile (`--features startup-probe`)

Verified via `checks/audit-firmware.ps1 -Profile StartupProbe` to exercise `.data` LMA-to-VMA copy alongside `.bss` zeroing in thumb startup assembly:
- `FlashSpan = 107,028 B`, `.data = 4 B` (`LIULI_PROBE_DATA`), `.bss = 73,256 B` (`LIULI_PROBE_BSS = 4 B`, `LIULI_DMA_BAND_A/B = 5,120 B` each, `LIULI_STORAGE = 63,008 B`), `StaticRAM = 73,288 B`, `StackReserve = 65,976 B`.

### Static Call-Chain Stack Audit (`checks/audit-stack.ps1`)

Measured on final release Thumb machine code via `llvm-objdump` (`-MaxThreadBytes 6144`):
- **Deepest reachable call chain from `Reset`**: **`4,688 B`** (`<= 6,144 B` limit; `61,328 B` headroom below `66,016 B` `StackReserve`).
- **Largest individual frames**:
  - `Engine<TerminalApp>::commit_view_inner`: `1,888 B`
  - `firmware_main`: `1,352 B`
  - `liuli_core::layout::arrange_iterative`: `1,144 B`
  - `liuli_render::raster::render_band::<RetainedTree>`: `816 B`
  - `liuli_settings::pages::dashboard::build_dashboard`: `744 B`
- Note: Worst-case interrupt stack nesting must still be added when signing off a board-specific BSP.

## 3. Zero-Allocation Proof & Transfer Bounds

- **Zero heap allocations**: Verified single-threaded in `host/liuli-sim/tests/allocation.rs` (`--test-threads=1`) across two full suites:
  - `runtime_loop_performs_zero_heap_allocations` (`SettingsApp` legacy workflow + DMA): `alloc == 0`, `alloc_zeroed == 0`, `realloc == 0`.
  - `terminal_three_page_full_lifecycle_performs_zero_heap_allocations` (`TerminalApp` initial SPI frame via `SpiWindowTarget`, telemetry ticks, long-press repeat, 3-page navigation & focus/scroll restore, `EaseOutCubic` toggle animation, modal confirm, `Toast` visibility/expiry, and `poll_dma` frame with `Busy` lock): `alloc == 0`, `alloc_zeroed == 0`, `realloc == 0`.
- **No global allocator in firmware**: `checks/firmware` omits any `#[global_allocator]` while reaching both sync SPI (`SpiWindowTarget`) and ping-pong DMA (`AuditSink` + `Engine::poll_dma`).
- **Bounded traversal stack**: Layout/arrange iterate over fixed stacks (`[MeasureFrame; MAX_DEPTH + 2]`, `[ArrangeFrame; MAX_NODES]`) without recursion or stack-allocated `Storage`/`CommandList` temporaries.
- **Pixel transfer bounds** (`host/liuli-sim/tests/incremental.rs`):
  - Initial present (`SettingsApp` & `TerminalApp`): exact `76,800` pixels (`30` bands of height `8`).
  - Idle re-present: `0` bands, `0` pixels.
  - Clipped offscreen temperature update (`SettingsApp`, `scroll_y = 0`): `measured_nodes == 0`, `0` bands, `0` pixels.
  - Visible temperature update (`SettingsApp`, `scroll_y = 91`): `measured_nodes == 0`, `<= 12,800` pixels (`320 × 40`).
  - Scroll viewport update (`SettingsApp`): fixed header (`y < 32`) and footer (`y >= 216`) stay outside damage (`58,880` px vs `76,800` px).
  - Dashboard single telemetry sample (`TerminalApp`, `t = 1,000 ms`): damage stays strictly inside card readouts/waveform (`32 <= y`, `bottom <= 176`), transferring `0 < pixels_sent <= upper_bound < 76,800` bounded by `DamageSet` + `BandIter` over the 5 updated widgets.
