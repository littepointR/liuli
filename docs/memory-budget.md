# Memory & Transfer Budget Verification

All core runtime structures live in fixed-capacity buffers with zero heap allocation (`#![no_std]`, zero `alloc` dependency).

## 1. Host `size_of` Breakdown (`x86_64-pc-windows-msvc`)

Measured directly by `cargo test -p liuli --test capacity --locked -- --nocapture`:

| Item | Size (bytes) | Notes |
|---|---|---|
| `RetainedNode` | 232 | 64 slots inside `RetainedTree` |
| `RetainedTree` | 16,008 | Active + candidate trees (`2 × 16,008 = 32,016 B`) |
| `ViewDraft` | 7,184 | Single builder transaction buffer |
| `PaintCmd` | 64 | `256` max commands per `CommandList` |
| `CommandList` | 16,416 | Active + candidate lists (`2 × 16,416 = 32,832 B`) |
| `Storage` (total struct) | 73,120 | Trees + commands + draft + `DamageSet`×2 + `AnimPool` + focus/pointer/clock |
| Dual DMA band buffers (`2 × 320 × 8 × u16`) | 10,240 | Ping-pong DMA pixel strips |
| **Total core working RAM** | **83,360** | **81.41 KiB <= 131,072 B (128 KiB limit)** |

Host-only simulation buffers (`MemoryScreen` `320 × 240 × u16` = `153,600 B`, PNG/PPM Vecs, HTTP buffers) exist solely in `host/liuli-sim` and are excluded from firmware RAM.

## 2. Bare-Metal ELF Section Sizes (`thumbv7em-none-eabihf`)

Built by `checks/firmware` with `--target thumbv7em-none-eabihf --release --locked` (`opt-level = "s"`, `lto = true`, `panic = "abort"`, `codegen-units = 1`, linker `rust-lld`, `FLASH = 256K`, `RAM = 136K`):

| Region / Section | VMA | Size (bytes) | Notes |
|---|---|---|---|
| `.vector_table` (Flash) | `0x08000000` | 8 | Reset SP + `Reset` vector (`entry=0x08000009`) |
| `.text` (Flash) | `0x08000008` | 53,072 | Engine, layout, E-G primitives, LF4 text/A4 blender, settings app |
| `.rodata` (Flash) | `0x0800cf58` | 12,756 | Includes 11,699 B embedded `settings.lf4` font |
| `.ARM.exidx` (Flash) | `0x0801012c` | 16 | ARM index table |
| `.data` LMA image (Flash) | — | 67,456 | Initialized `Storage` image |
| **Total Flash footprint** | — | **133,308** | **130.18 KiB <= 262,144 B (256 KiB)** |
| `.data` (RAM) | `0x20000000` | 67,456 | `STORAGE: SyncCell<Storage>` (32-bit target pointers/usize) |
| `.bss` (RAM) | `0x20010780` | 10,240 | `DMA_BAND_A` (5,120 B) + `DMA_BAND_B` (5,120 B) |
| **Total static working RAM (`.data + .bss`)** | — | **77,696** | **75.88 KiB <= 131,072 B (`link.x` `ASSERT`)** |
| **Reserved stack (`_stack_start - _ebss`)** | — | **61,568** | **60.13 KiB >= 8,192 B (`link.x` `ASSERT`)** |

## 3. Zero-Allocation Proof & Transfer Bounds

- **Zero heap allocations**: Verified in `host/liuli-sim/tests/allocation.rs` (`--test-threads=1`) using a counting `GlobalAlloc` over an entire cycle of `Engine::new_in_place`, initial `present_sync`, navigation, toggle animation, scrolling, timer temperature update, `poll_dma` band transfer, and modal open/close: `alloc == 0`, `alloc_zeroed == 0`, `realloc == 0`.
- **No global allocator in firmware**: `checks/firmware` omits any `#[global_allocator]` while reaching tree construction, command emission, input dispatch, and strip presentation.
- **Bounded traversal stack**: Layout/arrange iterate over fixed stacks (`[MeasureFrame; MAX_DEPTH + 2]`, `[ArrangeFrame; MAX_NODES]`) without unbounded recursion or function-local `Storage`/`CommandList` temporaries.
- **Pixel transfer bounds** (`host/liuli-sim/tests/incremental.rs`):
  - Initial present: exact `76,800` pixels (`30` bands of height 8).
  - Idle re-present: `0` bands, `0` pixels.
  - Clipped offscreen temperature update (`scroll_y = 0`): `measured_nodes == 0`, `0` bands, `0` pixels.
  - Visible temperature update (`scroll_y = 91`): `measured_nodes == 0`, `<= 12,800` pixels (`320 × 40`).
  - Scroll viewport update: header (`y < 32`) and footer (`y >= 216`) remain out of damage (`58,880` px vs `76,800` px).
