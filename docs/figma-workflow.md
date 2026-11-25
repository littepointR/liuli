# Figma 设计到代码与 Token 离线工作流

`liuli` 提供零外部依赖的纯 Python 3 离线工具 `tools/liuli-figma.py`，把设计系统的 W3C Design Tokens 与 Figma REST JSON 直接检查、编译、预览并做 RGB565 像素回归，全流程无需网络访问，生成的 Rust 代码兼容 `#![no_std]`、零堆分配约束。

## 工具子命令总览

### 1. 生成 W3C Design Token Rust 代码 (`tokens`)

将 W3C Design Tokens JSON 中的 `$type: "color"` 与 `$type: "dimension"` 编译为常量与 `Theme` 片段：

```sh
python tools/liuli-figma.py tokens --input design/tokens.json --output src/tokens_gen.rs --strict
```

- `--input <path>`：必填，W3C Tokens JSON 文件路径。
- `--output <path>`：可选，输出 `.rs` 文件路径；省略时输出到标准输出。
- `--strict`：对无法解析的颜色或维度直接以非零退出码报错。
- 颜色映射为 `rgb8(r, g, b)` / RGB565 常量，同时对含前背景配对的令牌自动校验 WCAG AA 对比度。

### 2. 编译 Figma JSON 为 `ViewBuilder` / `NodeProps` 代码 (`compile`)

```sh
python tools/liuli-figma.py compile --input design/screen.json --output src/screen_view.rs --frame MainScreen --fn-name build_main
```

- `--input <path>`：必填，导出的 Figma 文档 JSON。
- `--output <path>`：可选，目标 Rust 源文件；省略时打印到标准输出。
- `--frame <name|id>`：可选，按画框名称或节点 ID 选取目标画框（默认取首个顶层画框）。
- `--fn-name <ident>`：可选，自定义生成的 Rust 视图函数名。

布局与属性自动映射规则：
- `layoutMode: "HORIZONTAL"` + `layoutWrap: "WRAP"` -> `ui.wrap(...)`；无 wrap -> `ui.row(...)`。
- `layoutMode: "VERTICAL"` -> `ui.column(...)`。
- `layoutMode: "NONE"`（含子节点）-> `ui.stack(...)`。
- `TEXT` 节点 -> `ui.label(Key(...), NodeProps::new().fg(...).font(...).text_align(...).text(TextRef::Static(...)))`。
- Auto-Layout `primaryAxisAlignItems` (`CENTER` / `MAX` / `SPACE_BETWEEN`) -> `Justify::{Center, End, SpaceBetween}`；`counterAxisAlignItems` (`CENTER` / `MAX` / `STRETCH`) -> `Align::{Center, End, Stretch}`。
- `itemSpacing` -> `.gap(...)`，`padding*` -> `.padding(EdgeInsets { .. })`，填充/描边/圆角 -> `.bg(...)` / `.border(...)` / `.radius(...)`。

### 3. MCU 容量预算与可用性检查 (`lint`)

在写代码或烧录前校验设计稿是否超出目标档位预算并检查可访问性：

```sh
python tools/liuli-figma.py lint --input design/screen.json --frame MainScreen --profile default --min-touch 32 --strict --json
```

- `--profile {default,128,256}`：选择容量档案进行节点数与嵌套深度预算审查。
- `--min-touch <px>`：默认 `32`，检查交互热区尺寸下限。
- `--strict`：把预算警告升级为阻塞检查项。
- `--json`：输出机器可读的 JSON 检查报告（适合 CI）。

### 4. RGB565 像素回归比对 (`diff`)

不依赖 Pillow 等第三方库，直接按像素比对 PNG、二进制 P6 PPM 或裸 RGB565 图像：

```sh
python tools/liuli-figma.py diff --expected ref.png --actual sim_frame.png --rgb565-quantize --max-mismatch-pct 0.5 --diff-out diff.png
```

- `--expected` / `--actual`：参考图与实测图（支持 `.png`、`.ppm` 及配合 `--width W --height H` 的裸 `.rgb565` 缓冲）。
- `--rgb565-quantize`：先把预期图量化到 RGB565 色深再比较，消除 24-bit 设计稿与 16-bit 屏显的量化尾差。
- `--channel-tolerance <n>` / `--max-mismatch-pct <p>`：配置单通道容差与最大容许失配百分比。
- `--diff-out <png>`：生成高亮失配区域的可视化差分图。
- `--json`：输出 `mismatch_pct`、`mae`、`max_channel_delta` 与 `ok` 判定 JSON。

## 推荐端到端迭代闭环

1. **预算拦截**：`python tools/liuli-figma.py lint --input figma.json --profile default --strict` 确保节点深度与触控热区合理。
2. **代码与 Token 生成**：使用 `tokens` 与 `compile` 生成零堆 Rust 代码并在应用中引入或微调。
3. **模拟器热预览与耗时分析**：在主机预览通过 `PreviewServer::watch_path(...)` 监听生成源码或资源变化，在浏览器开启“性能 HUD”查看 `GET /api/profile` 返回的 `step_us`、`raster_us`、`dirty_pct_x10` 与 `fps_x10`，同时 `/api/watch` 自动触发界面重载。
4. **像素验收**：把模拟器截屏与设计导出图交给 `liuli-figma.py diff --rgb565-quantize` 做回归门禁，离线单元测试可通过 `python tools/test-figma.py` 一键自检。
