# Event Replay Script Format (`liuli-sim replay`)

Replay scripts are UTF-8 line-oriented text files parsed by `host/liuli-sim/src/replay.rs`. Comments starting with `#` and blank lines are ignored.

An optional header line may appear before the first timestamped event to select the application scenario (default when omitted: `settings` for backwards compatibility with v0.1 scripts):

```text
scenario terminal
```

Supported scenarios: `settings` (`SettingsApp`), `terminal` (`TerminalApp`).

Each subsequent non-empty line has the form:

```text
<time_ms> <op> [args...]
```

Timestamps (`u64` milliseconds) must be non-decreasing across lines. Every mutating op uses the shared deterministic simulation step (`advance` -> `dispatch` -> capture pre-present damage/bands -> `present_sync` -> snapshot + wakeup) and verifies pixel-for-pixel agreement against `render_reference_fullscreen`. Any syntax error, decreasing timestamp, unknown op, wrong arg count, invalid shot name, or failed `expect` reports the 1-based line number, timestamp, and op and exits non-zero.

## Supported Operations

| Op | Args | Effect |
|---|---|---|
| `next` | *(none)* | Dispatch `RawInput::Nav(NavDir::Next)` |
| `prev` | *(none)* | Dispatch `RawInput::Nav(NavDir::Prev)` |
| `activate` | *(none)* | Dispatch `RawInput::Activate` |
| `adjust` | `<steps:i16>` | Dispatch `RawInput::Adjust(steps)` |
| `back` | *(none)* | Dispatch `RawInput::Back` |
| `key_down` | `<activate\|adjust_prev\|adjust_next>` | Dispatch `RawInput::KeyDown(hold_key)` |
| `key_up` | `<activate\|adjust_prev\|adjust_next>` | Dispatch `RawInput::KeyUp(hold_key)` |
| `down` | `<id:u8> <x:i32> <y:i32>` | Dispatch `RawInput::PointerDown` |
| `move` | `<id:u8> <x:i32> <y:i32>` | Dispatch `RawInput::PointerMove` |
| `up` | `<id:u8> <x:i32> <y:i32>` | Dispatch `RawInput::PointerUp` |
| `cancel` | `<id:u8>` | Dispatch `RawInput::PointerCancel` |
| `page` | `<dashboard\|settings\|diagnostics>` | Switch page via `TerminalMsg::SelectPage` (`terminal` scenario only) |
| `advance` | *(none)* | Advance timers, animations, and long-press repeat up to `time_ms` |
| `shot` | `<name>` | Validate `<name>` (`[A-Za-z0-9_-]`, unique in script), save `<out_dir>/<name>.(ppm\|png)` |
| `expect` | `<field> <value>` | Assert engine/model/focus/metrics/wakeup state (see below) |

## `shot` Filename Rules

- `<name>` must be a single non-empty filename segment composed strictly of `[A-Za-z0-9_-]`.
- Directory separators (`/`, `\`), drive colons (`:`), `.`, `..`, and duplicate shot names within the same script are rejected with a line-numbered error.

## `expect` Fields

- `page`: `dashboard`, `settings`, `diagnostics`
- `focus`: `brightness`, `sound`, `theme`, `monitor`, `reset`, `cancel`, `confirm`, `tab_dashboard`, `tab_settings`, `tab_diagnostics`, `dash_pause`, `dash_dec`, `dash_inc`, `bright_dec`, `bright_inc`, `diag_refresh`, `diag_monitor`, `none`, or raw key number
- `ring`: `true` / `false`
- `editing`: `true` / `false`
- `modal`: `open` / `closed`
- `toast`: exact toast string (e.g. `已恢复默认`, `诊断已刷新`) or `none`
- `capture`: `none`, `button`, `slider`, `candidate`, `scrolling`
- `scroll`: signed integer (`ScrollY` offset in px)
- `brightness`: integer (`0..=100`)
- `sound`: `on` / `off`
- `theme`: `light` / `dark`
- `monitoring`: `on` / `off`
- `temp`: Celsius value (`23.0`) or integer decidegrees (`230`)
- `load`: integer (`5..=100`)
- `samples`: Sparkline sample count (`0..=32`)
- `measured`: `measured_nodes` on last step
- `bands_le`: upper bound on bands transferred by last step
- `pixels_le`: upper bound on pixels transferred by last step
- `wakeup`: `now`, `on_event`, `at` (any `NextWakeup::At`), or `at:<ms>`

## CLI Usage

```powershell
python tools/build-ui.py --app apps/liuli-settings/Cargo.toml --firmware host/liuli-sim/Cargo.toml --action run --locked -- --bin liuli-sim -- replay host/liuli-sim/tests/fixtures/terminal.events --out target/liuli-v02-png --format png
python tools/build-ui.py --app apps/liuli-settings/Cargo.toml --firmware host/liuli-sim/Cargo.toml --action run --locked -- --bin liuli-sim -- preview --port 7878 --scenario terminal
```
