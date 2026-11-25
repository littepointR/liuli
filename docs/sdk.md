# 从空目录使用琉璃 SDK

解压对应操作系统的 SDK 到空目录。桌面开发需要 Rust 1.100 和系统链接器。SDK 内的 .cargo/config.toml 已指向随包供应的 Cargo 源。保留该目录和 vendor，然后在 SDK 根目录运行：

~~~
cargo run --offline --manifest-path device-ui/Cargo.toml --example preview -- --headless
cargo run --offline --manifest-path device-ui/Cargo.toml --example preview
~~~

浏览器打开终端打印的本机地址。第一条命令会用模板应用构建两帧，对照完整重绘。第二条命令会提供交互预览。在 device-ui/src/lib.rs 实现 App，Model 保存业务数据，update 返回 Change::View 时框架重建视图。桌面和设备使用相同的键、布局和逻辑坐标。在 api/liuli/index.html 和 api/liuli_widgets/index.html 查看 API。

## 无人值守的 UI 回归

安装 Python 3 和桌面 Rust 工具链后，在解压的 SDK 根目录执行以下命令。源码仓库也有相同入口，首次使用源码时先用 `cargo fetch --locked` 准备依赖。

~~~sh
python tools/check-ui.py
python tools/check-ui.py --screenshots shots
~~~

三平台打包 CI 会在空 Cargo 缓存下运行第一条命令。它直接驱动应用的输入、命中测试和单调时钟，比较增量渲染与完整重绘，同时检查十页控件、焦点框的四条边、告警编辑后返回的位置、输入法组合与故障恢复。失败会以非零状态退出，并输出失败的场景或断言。第二条命令还会保存控件画面。整个过程使用随包夹具，可以在没有浏览器、串口或开发板的构建机上执行。

为应用添加回归时，可以直接编写逻辑坐标和时间戳。例如下面的夹具在同一位置按住 900 ms。中间的 `tick` 在手指静止时推进时钟，供引擎触发长按。将轨迹交给 `replay_trace`，用应用模型断言目标条目、打开的页面或焦点，并启用逐帧像素对照。随包的 `liuli-alarms/tests/workflow.rs` 展示了如何在抬手前断言页面变化。

~~~text
liuli-trace 1
0 kind=down&v=0&x=92&y=389
800 tick
900 kind=up&v=0&x=92&y=389
~~~

发生实板特有的问题时，可自愿将该次输入从串口导出，作为另一条夹具。随包的实板记录可以离线反复重放。要在参考板上测量像素计算或 QSPI 传输时间，可直接启动 `benchmark.bin`，它会自动执行固定输入和像素对照。[参考板录制和基准](waveshare-s3.md)

## 控件与主题

从包含 Spool 的 SDK 运行 tools/bin/liuli-widget-demo（Windows 使用 .exe），打开终端打印的网址即可进入便携录音台。在 Deck、Tapes 和 Mixer 之间可以播放、保存素材、调音和编辑。示例源码位于包根目录的 liuli-widget-demo。板级固件使用同一份应用代码。[控件清单和运行方法](widget-demo.md)

用下面的命令启动控件目录，切换 Controls / Meter，操作选择器、数值编辑器和确认框。控件目录可独立从供应的控件包构建。

~~~
cargo run --offline --manifest-path liuli-widgets/Cargo.toml --example catalog -- device-ui/assets/body.lf4
cargo run --offline --manifest-path liuli-alarms/Cargo.toml --example preview -- liuli-alarms/assets/body.lf4
cargo test --offline --manifest-path liuli-alarms/Cargo.toml --release
~~~

| 控件 | 应用接入方式 |
|---|---|
| 按钮、开关、滑块、进度条、曲线 | 调用 Children 或 ThemedUi 上的对应构建方法。通过 UiAction 处理激活、切换和数值变化。 |
| 虚拟列表 | virtual_list 接收非零行高、记录数、偏移和视口高度。为每个数据 ID 提供稳定 Key，在 App::on_scroll 中更新偏移。 |
| 数值编辑、选择器、页签 | number_editor 产生 SetValue。selector 和 tabs 使用调用方提供的选项键。应用保存选中项。 |
| 对话框、提示条 | confirm_dialog / centered_confirm_dialog 使用模态焦点域和 Dismiss 动作。toast 使用无焦点标签，显示时长由应用的时钟控制。 |
| 自绘 | Children::custom 建立节点，App::measure_custom 给出内容尺寸，App::paint_custom 可调用 WidgetCanvas::fill 或 bitmap。绘制使用相对坐标，框架负责裁剪。 |

组件本地状态可放在模型的 KeyedState<T, N> 中，按页面和稳定控件键查找。超容量返回 StateCapacity，保留旧条目。由组件在退出页面或删除数据后调用 retain 释放对应条目。

复合组件可以是接收 Children、键和主题的普通 Rust 函数。liuli-widgets 中的 paint_meter、confirm_dialog 和 virtual_list 是可运行范例。为复合组件的每个子节点提供稳定键。将决定自绘内容的值保存在 NodeProps 中，框架依据提交前后的属性差异决定损伤区域。

把 Tween::at(initial) 存入组件状态，用 animate_to(now, target, duration_ms, easing) 控制位置、缩放或透明度。App::next_deadline 返回 tween.next_deadline 的结果，App::on_timer 读取 tween.value(now) 更新绘制属性。重新设置目标会从当前取值续接，cancel(now) 可在页面退出时冻结动画。

### 主题系统与按能力细粒度配置

主题由正交、可独立替换和逐项微调的配置令牌组成：`ColorScheme`（画布、三级抬升表面、`border_strong`、主文字/次文字/禁用文字、`accent`、`danger`、`success`、`warning`、`focus`）、`Density`（控件高度、间距、内边距、标题/图元高度）、`Shape`（三档圆角与边框线宽）、`Typography`（标题/正文/说明字体槽）、`ElevationScale`（`Raised`/`Floating` 阴影偏移、模糊、透明度与色调抬升）、`GlassStyle`（毛玻璃模糊半径、透色遮罩颜色与透明度、高光边框）、`InteractionStyle`（`FocusRingStyle` 启用/线宽/外扩/圆角模式、`PressFeedback` 暗化/提亮/强调染色/禁用、模态遮罩透明度）、`Motion`（`PressDepth` 时长/缩放比例/缓动、`Toggle` 缓动、提示与翻转时长）以及 `EffectsConfig`。

- 预设构造：`Theme::DARK`、`Theme::LIGHT` 与 `Theme::preset(ThemePreset::{Liuli, Classic, Industrial}, dark)`；`EffectTier::{Flat, Depth, Tinted, Glass}` 作为构造 `EffectsConfig` 的快捷挡位。
- 逐项独立覆盖：所有结构字段和单能力开关都可以单独调整，例如 `theme.with_shadows(false)`、`theme.with_press_depth(false)`、`theme.with_glass_blur(false)`、`theme.with_blur_radius(3)`、`theme.with_glass_tint(None, 8)`、`theme.with_rounded_focus(true)`、`theme.with_control_height(38)`、`theme.with_motion(Motion::SNAPPY)`，也保留 1.x `Theme::with(Style { .. })` 的局部覆盖兼容。
- 语义化容器与按钮变体：`SurfaceSpec` 统一组合 `Material::{Solid, Glass { .. }, Transparent}`、`Elevation::{Flat, Raised, Floating}` 与 `Compositor`（组透明度与 3D 投影）；`ButtonVariant` 提供 `Surface`、`Primary`、`Secondary`、`Outline`、`Ghost`、`Danger`。通过 `ThemedUi` 作用域构建器声明 `card` / `surface_column` / `button` 时，框架按 `EffectsConfig` 自动降级（例如关闭模糊但保留透色玻璃，或无图层后端时退回实色抬升卡片），同时保持 `Compositor` 变更可命中 `reuse_groups` 缓存。

## 表面效果与图层合成

在父容器的构建闭包中先声明控件，再用同一个稳定键调用 `decorate`（或直接使用 `ThemedUi::surface_column` / `card`）。基础档和增强档都可以使用 `theme.elevation(Elevation::Raised)`。也可直接传入 `Shadow` 与 `PressDepth`（如 `PressDepth::OFF` 或 `PressDepth::custom(ms, scale, easing)`）独立控制阴影偏移、模糊宽度、0 到 15 的透明度和按压收缩曲线。阴影服从父级裁剪，保留原来的布局、点击区域和绘制顺序。

增强配置下，`Layer` 支持 `backdrop_radius`（0 到 8）、`tint_color` 与 `tint_alpha`（0 到 15，为透明毛玻璃提供可读的磨砂透色层）、组 `opacity` 与 `Projection` 3D 透视。下例用 `ThemedUi` 与 `SurfaceSpec` 让玻璃卡片随能力配置自适应，完整交互调参见 Widget Demo 的 `src/depth.rs`。

```rust
use liuli::prelude::*;
use liuli_widgets::{ButtonVariant, Compositor, Elevation, Material, SurfaceSpec, Theme, ThemedUi};

fn card(ui: &mut Children<'_, '_>, theme: Theme) -> Result<(), BuildError> {
    let mut tui = ThemedUi::new(ui, theme);
    let spec = SurfaceSpec::CARD
        .with_material(Material::glass(5, 7))
        .with_elevation(Elevation::Raised)
        .with_compositor(Compositor::isolated(12, Projection(32)));
    tui.card(Key(30), spec, |body| {
        body.button(Key(31), "Open", IconKind::None, ButtonVariant::Primary)
    })
}
```

Projection 用 256 个单位表示一整圈，围绕卡片的竖直中心轴投影到布局框中。64 和 192 正好侧对屏幕，此时卡片的画面和触摸区域均为空。背面按物理翻转镜像显示。Demo 在越过侧面时切换背面内容和朝向，保持文字可读。引擎用同一逆映射进行绘制、点选和点击命中。用 Tween 更新角度或整组透明度即可制作转场。

每棵树可装饰八个稳定键。基础档按全局像素坐标直接绘制阴影。增强档先在局部矩形里绘制整棵子树，再合成一次。透明区域保留背景，文字和按钮重叠处具有一致的组透明度。毛玻璃使用半径 0 到 8 的可分离盒式滤波，从先前绘制的背景读取包含边缘邻域的像素。移动或移除表面时会更新旧、新覆盖区域。受裁剪的表面在自身可见区域内采样，并受父级裁剪约束。

增强档默认给每份命令列表 512 KiB 像素预算，两份命令列表用于事务提交。单个图层每个可见像素占 2.5 字节（RGB565 色彩与 2 像素共 1 字节的打包 A4 透明度），启用毛玻璃时还需为含采样边缘的背景区域预留每像素 2 字节（原地盒式模糊与预烘焙透色遮罩，无额外 scratch 缓冲）。可通过 `Engine::set_layer_budget(bytes)` 调整单份预算。超出预算会返回 `BuildError::LayerPixels`，从 `build_failure` 可取得控件键。上一帧继续有效，修正后可通过 `commit_view` 重试。检查器的帧报告列出活动列表的 `layer_pixel_bytes` 和本帧跨帧复用的子树数量 `reused_groups`。

## 字库与图片

device-ui/assets/fonts.toml 中的 source、charset 和 output 都相对配置文件。每个 [[font]] 提供源字体、Unicode 字符集文件、字号和一个源字体具备的回退字符。budget 限制所有槽位生成的总字节数。把纯数字项目的 fallback 设置为项目中已有的字符即可；配置文件支持多项和自定义像素字号。

~~~
tools/bin/liuli-fontpack pack --project device-ui/assets/fonts.toml
tools/bin/liuli-fontpack verify --project device-ui/assets/fonts.toml
tools/bin/liuli-image logo.ppm device-ui/assets/logo.l565
~~~

图片工具接收二进制 RGB8 PPM（P6）。在应用里用 Bitmap::from_asset(include_bytes!("../assets/logo.l565")) 建立静态图片，在 paint_custom 里向 canvas.bitmap 传入目标矩形、旋转角和 0 到 15 的透明度。一个整圈是 256 个角度单位。样例使用的字体子集随附 OFL 许可证；替换字体时保留新字体的再分发许可。

基础档可用 Lf4Font::from_bytes 和 FontSet::single 直接读取 LF4 v1。增强参考板把索引映射到地址空间，传给 Engine::present_sync_with_pixels 的 GlyphPixels 仅在字形与条带相交时读取位图。StoredFont、ReadAt、Cached 分离索引验证、按偏移读、缓存预算和调用方提供的字形缓冲区。未找到的字符、格式错误和底层读取错误分别返回。两种字体槽使用不同资源时，读取实现需按 FontId 分派。

## 增强档文本框

用下面的命令打开单行编辑器。输入控件取得框架焦点后，预览器会显示“输入到当前文本框”。在此处使用系统输入法、粘贴文本或用左右方向键和退格键编辑。用 --headless 可重放组合更新、提交和取消，逐帧检查增量绘制与全屏参考图像。

~~~
cargo run --offline --manifest-path liuli-alarms/Cargo.toml --example editor --features enhanced -- boards/waveshare-s3-amoled/bench-fonts/Arabic.ttf boards/waveshare-s3-amoled/bench-fonts/Devanagari.ttf
~~~

示例把 TextField 放在模型中，用 accepts_text 和 on_text 接收焦点路由的 TextInput。预编辑与已提交字符串分开保存，按字素进行删除，按视觉顺序移动光标。paint_model 提交不可变的 RGB565 文本图，保留到该帧被替换。单行控件在创建时指定最大 UTF-8 字节数，容量或栅格化失败时保留上次成功的文字和图像。

## 录制、排查与回归

在浏览器里 Shift + 点击节点可查看键、祖先、声明尺寸、测量结果、裁剪与最终颜色，同时可对照实际输入目标。Theme::control 记录主题、声明位置以及每个局部覆盖的来源，检查器展示当前已提交视图的记录。帧诊断记录触发输入和命中路径、实际变色像素、传输字节、内嵌字形请求次数和字节数、跨条带重复请求及容量峰值。状态面板显示焦点、捕获者、节点数、命令数、文本池字节数和本帧传输像素。Damage 和 Bands 标注重绘与发送区域。点击“导出操作回放”，将文件放入应用的测试夹具目录。以下命令在从初始模型启动的模板上重放，包括计时器推进及触点 ID：

~~~
cargo run --offline --manifest-path device-ui/Cargo.toml --example preview -- --replay recorded.trace
~~~

对告警应用，先给 preview 传入字库路径，再传 --replay 与文件路径。回放逐帧比较增量渲染与全屏重绘；用模型、焦点或截图断言额外验证业务结果。每次预览保留最近 16 帧的 PNG，完整事件记录最多 8 MiB，达到上限会明确报错。

在 SDK 根目录运行以下诊断和编辑器回放。两条命令都使用随包资源。

~~~sh
cargo run --offline --manifest-path liuli-alarms/Cargo.toml --example diagnostics
cargo run --offline --manifest-path liuli-alarms/Cargo.toml --example editor --features enhanced -- --replay docs/hardware/rc1-browser-ime.trace
~~~

诊断示例在同一个 96 × 48 场景内重现三类故障。点 (8, 8) 应当选中按钮 11，命中报告给出挡住按钮的可点击标签 12。将覆盖层的 clickable 设为 false 即可让点击落到按钮。第二帧只修改根节点的 theme_id，报告显示像素变化为零，仍传输了 9,216 B；移除没有视觉变化的版本递增可避免这次整屏发送。最后一次更新耗尽节点容量，错误列出页面 7、键 160 和 Nodes。例子断言上一帧仍然可用，再通过 AppSession::recover 缩小活动窗口并提交视图。

## 从 v0.2 迁移与恢复

两个原有应用、LF4 v1 和旧 .events 回放保留原入口。新应用使用 AppSession::<YourApp>::new 设置模型、FontSet 和显示尺寸。设备使用 Engine::new_in_place_with_fonts，在单调毫秒时钟下依次调用 tick（或 try_advance）、handle_input（或 try_dispatch）和 present。

## 硬件移植（统一接口）

所有硬件胶水类型统一通过 `use liuli::prelude::*;` 导出，按能力解耦、与底层总线协议（SPI / QSPI / 8080 / RGB-LTDC）无关：

| 抽象能力 | 核心接口 | 适用场景与说明 |
|---|---|---|
| 同步显示输出 | `DisplaySink`（`flush_rect(&mut self, rect, pixels: &[u16])` + `finish_frame()`） | 引擎直接递交整块连续 `&[u16]` 条带像素，无需经由 `DrawTarget` 逐像素迭代器转换；适用于 SPI、QSPI、8080 并口或 SDRAM/LTDC 双缓冲。已有 `DrawTarget<Color = Rgb565>` 可用 `EgDisplaySink` 包装，字节窗口总线可用 `ByteWindowBus` + `ByteWindowSink`（1.x 仍兼容 `SpiWindowBus`、`SpiWindowTarget` 与 `present_sync*`）。 |
| 异步双缓冲 DMA | `DmaSink` + `DmaCoordinator` | 使用 `FreeBand -> ReadyBand -> InFlightBand` 编译期所有权流转；通过 `DmaCoordinator::with_wire_order(ByteOrder::BigEndian)` 或 `encode_rgb565_words` 完成字节序转换，`Engine::poll_dma`（或 `poll_dma_with_provider`）驱动零拷贝流水线。 |
| 多指触摸追踪 | `TouchTracker<N>` + `TouchPoint` | 仅向 `tracker.update(&contacts[..count], \|ev\| ...)` 或 `cancel_all` 传入当前采样到的触点数组，框架自动差分生成顺序正确的 `PointerUp` / `PointerMove` / `PointerDown` / `PointerCancel`。 |
| 32 位环形时钟 | `MonotonicClock32` | 通过 `MonotonicClock32::new(ticks_per_ms)` 或 `from_hz(hz)` 将 32 位溢出计数器转换为无回退的 `InstantMs`。 |
| 非 XIP 外置字库 | `ReadAt` + `BlockCache` + `StreamingFont` / `HybridFontSet` | 与片内 `FontSet` 统一接入 `Engine::present` 与 `Engine::poll_dma_with_provider`，按需流式读取 Flash/SD 字体。 |

设备主循环典型写法（同步输出路径）：

```rust
let mut tracker = TouchTracker::<5>::new();
let mut clock = MonotonicClock32::from_hz(CPU_HZ);
loop {
    let now = clock.sample(read_hw_counter());
    engine.tick(now)?;
    match touch.poll_contacts(&mut points) {
        Ok(n) => tracker.try_update(&points[..n], |ev| engine.handle_input(ev, now).map(|_| ()))?,
        Err(_) => tracker.try_cancel_all(|ev| engine.handle_input(ev, now).map(|_| ()))?,
    }
    engine.present(&mut display_sink, &mut scratch)?;
    sleep_until(engine.wakeup(clock.now()));
}
```

try_message、try_dispatch 和 try_advance 提供事务结果。Busy 会交还尚待处理的消息或输入，在 DMA 完成后重试。WentBackwards 要求宿主修正时间。Build 表示提交失败，活动树和之前完成的帧仍可使用；模型可能已更新。读取 build_failure，按其中的页面、可用的控件键和资源类别修正模型，再调用 commit_view。重复业务消息可能重复执行它的副作用。AppSession::recover 可在桌面执行同样的修复和像素校验。

导航示例在 liuli-alarms/src/lib.rs。Navigator 以 const 容量保存携带参数的页面枚举。确认框关闭后才处理页面返回。离开列表前保存条目键、偏移和焦点，返回时通过 page_restore 提供这些值。运行 workflow 测试可检查整条路径。


## 版本与兼容性

1.0.0 沿用 RC1 已冻结的应用、组件与资源接口。应用、控件、模拟器、资源工具和参考板使用同一个 SDK 版本。升级时先保留原 LF4 和应用回放，再在新 SDK 的供应目录中构建并核对它们。安装包自带 liuli-sim 命令，可在空目录内重放两个 v0.2 应用。打包验收会把 12 幅输出和原始 PPM 逐字节比较。

~~~
tools/bin/liuli-sim replay vendor/liuli-sim-1.0.0/tests/fixtures/settings.events --out shots/settings --format ppm
tools/bin/liuli-sim replay vendor/liuli-sim-1.0.0/tests/fixtures/terminal.events --out shots/terminal --format ppm
~~~

| 接口或数据 | 1.0 兼容约定 |
|---|---|
| 应用 SDK 与正式控件 API | 1.x 中保持已有公开调用和事务行为。破坏已有调用的变更随新主版本发布，并给出迁移示例。 |
| 字体与图片 | LF4 v1、L565 v1 按各自文件头选取解码器。格式变更引入新的版本头。保留旧字库与像素对照。 |
| 操作回放 | 旧 .events 使用原入口。新预览事件在首行声明 liuli-trace 1，时间单位为毫秒。未知版本会返回错误。 |
| 平台后端 | 基础档保持有界存储和零堆。增强特性单独启用。每个支持平台用相同的逻辑坐标与裁剪规则验证。 |
