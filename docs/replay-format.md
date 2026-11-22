# Event Replay Script Format (`liuli-sim replay`)

Replay scripts are UTF-8 line-oriented text files parsed by `host/liuli-sim/src/replay.rs`. Comments starting with `#` and blank lines are ignored.

Each non-empty line has the form:

```text
<time_ms> <op> [args...]
```

Timestamps (`u64` milliseconds) must be non-decreasing across lines. Before running `<op>` at `time_ms`, the replay runner advances virtual time and processes any expired timers or animations up to `time_ms`. Every mutating op presents pending dirty bands via `present_sync` and verifies pixel-for-pixel agreement against `render_reference_fullscreen`. Any syntax error, decreasing timestamp, unknown op, wrong arg count, or failed `expect` reports the exact 1-based line number and exits non-zero.

## Supported Operations

| Op | Args | Effect |
|---|---|---|
| `next` | *(none)* | Dispatch `RawInput::Nav(NavDir::Next)` |
| `prev` | *(none)* | Dispatch `RawInput::Nav(NavDir::Prev)` |
| `activate` | *(none)* | Dispatch `RawInput::Activate` |
| `adjust` | `<steps:i16>` | Dispatch `RawInput::Adjust(steps)` |
| `back` | *(none)* | Dispatch `RawInput::Back` |
| `down` | `<id:u8> <x:i32> <y:i32>` | Dispatch `RawInput::PointerDown` |
| `move` | `<id:u8> <x:i32> <y:i32>` | Dispatch `RawInput::PointerMove` |
| `up` | `<id:u8> <x:i32> <y:i32>` | Dispatch `RawInput::PointerUp` |
| `cancel` | `<id:u8>` | Dispatch `RawInput::PointerCancel` |
| `advance` | *(none)* | Present any changes from timer/animation advance up to `time_ms` |
| `shot` | `<name>` | Save `<out_dir>/<name>.(ppm\|png)` and append `Shot` to `ReplayReport` |
| `expect` | `<field> <value>` | Assert engine/model/focus/wakeup state (see below) |

## `expect` Fields

- `focus`: `brightness`, `sound`, `theme`, `monitor`, `reset`, `cancel`, `confirm`, `none`, or raw key number
- `ring`: `true` / `false`
- `editing`: `true` / `false`
- `modal`: `open` / `closed`
- `capture`: `none`, `button`, `slider`, `candidate`, `scrolling`
- `scroll`: signed integer (`ScrollY` offset in px)
- `brightness`: integer (`0..=100`)
- `sound`: `on` / `off`
- `theme`: `light` / `dark`
- `monitoring`: `on` / `off`
- `temp`: Celsius value (`23.0`) or integer decidegrees (`230`)
- `wakeup`: `now`, `on_event`, `at` (any `NextWakeup::At`), or `at:<ms>`

## CLI Usage

```powershell
cargo run -p liuli-sim --locked -- replay host/liuli-sim/tests/fixtures/settings.events --out target/liuli-demo --format png
```
