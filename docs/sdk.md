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

从 SDK 运行 `tools/bin/liuli-widget-demo`（Windows 使用 .exe）可打开琉璃功能展台，切换仪表、表单、图表、布局和动效页面。传入 `--studio` 可以进入便携录音台。桌面与参考板共用应用源码。[控件清单和运行方法](widget-demo.md)

用下面的命令启动控件目录，切换 Controls / Meter，操作选择器、数值编辑器和确认框。控件目录可独立从供应的控件包构建。

~~~
cargo run --offline --manifest-path liuli-widgets/Cargo.toml --example catalog -- device-ui/assets/body.lf4
cargo run --offline --manifest-path liuli-alarms/Cargo.toml --example preview -- liuli-alarms/assets/body.lf4
cargo test --offline --manifest-path liuli-alarms/Cargo.toml --release
~~~

| 控件 | 应用接入方式 |
|---|---|
| 按钮、开关、滑块、进度条、曲线 | 调用 Children 或 ThemedUi 上的对应构建方法。通过 UiAction 处理激活、切换和数值变化。 |
| 页签 | 用 `TabPage` 声明页面，`Tabs` 统一生成页签栏、选择事件和当前内容页。`ThemedUi::tab_view` 提供常用布局。 |
| 虚拟列表 | virtual_list 接收非零行高、记录数、偏移和视口高度。为每个数据 ID 提供稳定 Key，在 App::on_scroll 中更新偏移。 |
| 数值编辑、选择器、页签 | number_editor 产生 SetValue。selector 和 tabs 使用调用方提供的选项键。应用保存选中项。 |
| 对话框、提示条 | confirm_dialog / centered_confirm_dialog 使用模态焦点域和 Dismiss 动作。toast 使用无焦点标签，显示时长由应用的时钟控制。 |
| 自绘 | Children::custom 建立节点，App::measure_custom 给出内容尺寸，App::paint_custom 可调用 WidgetCanvas::fill 或 bitmap。绘制使用相对坐标，框架负责裁剪。 |

组件本地状态可放在模型的 KeyedState<T, N> 中，按页面和稳定控件键查找。超容量返回 StateCapacity，保留旧条目。由组件在退出页面或删除数据后调用 retain 释放对应条目。

复合组件可以是接收 Children、键和主题的普通 Rust 函数。liuli-widgets 中的 paint_meter、confirm_dialog 和 virtual_list 是可运行范例。为复合组件的每个子节点提供稳定键。将决定自绘内容的值保存在 NodeProps 中，框架依据提交前后的属性差异决定损伤区域。

### 动画

### 页签与内容页

每个 `TabPage<Id, Model>` 声明业务 ID、稳定控件键、可本地化的标题和构建函数。也可以通过 `with_icon` 指定图标。业务 ID 和键在组内唯一，控件键遵守整棵视图树的唯一性约束。应用模型保存选中的 ID。库在生成视图时只调用当前内容页的构建函数，输入通过同一份声明映射回 ID。

```rust
let pages = [
    TabPage::new(Page::Live, Key(11), TextRef::Static("实时"), live_view),
    TabPage::new(Page::History, Key(12), TextRef::Static("历史记录"), history_view),
];
let tabs = Tabs::new(&pages, model.page)?;
tabs.view(column, [Key(10), Key(20)], model, theme)?;
```

在 `App::on_action` 中调用 `tabs.selection(action).map(Msg::SelectPage)`，并在更新函数中保存选中的页面。分离布局可用 `header_with` 放置标题和工具，配合 `panel` 把活动页放入裁剪视口或转场容器。检查器会显示 `TabBar`、`Tab` 和 `TabPanel`，以及内容区对应的页签键。

默认策略按字形、内边距、图标和触摸尺寸求宽；放不下时页签栏可以横滚。`TabLayout::Wrap { max_rows: 2 }` 按可用宽度换行，超过行数上限时回退为单行横滚。工具会按内容测量，空间紧张时页签另起一行。`tabs` 和 `tabs_in_columns` 是单独构建页签栏的兼容入口，后者保留调用方指定的列数。

左右键直接选择页签，顺序导航将一组页签作为一个焦点停靠点。旋钮按下后旋转预选，再按确认，返回键取消。焦点系统记住最近八个面板的子控件键，重新进入时恢复仍然有效的目标。记录和编辑值由模型保存。虚拟列表复用模型中的偏移，普通滚动视口可以用 `NodeProps::initial_scroll_offset` 恢复挂载位置。指示器使用作用域主题中的状态动效，支持本地覆盖和减少动态效果。

#### 控件架构与回归

布局引擎根据父约束和字体度量产生矩形。绘制和命中都使用布局产生的裁剪范围。控件库拥有语义、可选项与事件契约；应用声明页面、数据和业务更新。具有共同测量、输入或焦点语义的需求在相应库层实现，Demo 使用公开接口。修改这些契约时，用一个独立消费者测试及一条真实应用入口回放验证。页签回归同时覆盖临界宽度相差一像素、长标题、焦点、挂载和横滚；H743 验收比对范围包括顶栏标题及两侧操作。

### 控件布局与部件样式

控件通过 `NodeProps` 声明所需宽高，父容器分配空间，命中和焦点使用排版后的控件边界。需要单独限制页签时，在声明页签栏后用稳定键调用 `set_layout_constraints(key, min_w, max_w, min_h, max_h)`。例如将宽度的上下限均设为 `Some(80)`，就能让该页签在有足够空间时保持 80 像素宽。外边距参与排版，`set_offset` 在排版结果上移动控件及其命中区域。

Tabs 把栏底线映射到轨道部件，把选中标记映射到指示器部件。默认外观为整栏宽的 3 像素下划线。下面的主题让标记跟随文字和图标的实际宽度，切页时平移，距底边 2 像素。

```rust
use liuli::prelude::*;
use liuli_widgets::{PartRule, Theme, WidgetClass};

const RULES: &[PartRule] = &[PartRule {
    class: WidgetClass::TAB,
    parts: WidgetPartStyle::new()
        .with_indicator_layout(IndicatorLayout {
            span: IndicatorSpan::Content,
            edge_offset: 2,
            ..IndicatorLayout::UNDERLINE
        })
        .with_indicator_radius(2)
        .with_indicator_motion(IndicatorMotion::Slide),
}];
let theme = Theme::DARK.map_parts(|p| p.with_rules(RULES));
```

`IndicatorShape` 提供底线、顶线、填充和隐藏。`with_track_size(0)` 单独隐藏分隔线。`IndicatorMotion` 提供居中展开、淡变、平移和即时切换，持续时间和缓动取自主题的选中态动效。平移由页签栏统一绘制，换行和滚动使用同一组逻辑坐标。减少动态效果时直接呈现最终状态。

**主题与实例的覆盖顺序。** 控件类别默认值先应用，同一主题中匹配该 `WidgetClass` 的规则按声明顺序合并，最后合并 `NodeProps::with_parts` 或声明后的 `Children::style_parts(key, parts)`。每次合并只覆盖显式提供的属性。`with_rules` 安装整张规则表。对子树使用派生的主题，对某个实例使用按键覆盖。文本、指示器和焦点环分别取色。

**第三方控件。** 控件包可以定义 `const METER: WidgetClass = WidgetClass("vendor.meter");`。声明自绘节点时使用 `NodeProps::new().with_parts(theme.parts.resolve(METER))`。在 `App::measure_custom_styled` 中读取 `PartMetrics` 决定固有尺寸，在 `paint_custom` 中通过 `canvas.parts()`、`foreground()`、`background()` 和 `selection_indicator(control, content)` 读取相同样式和状态。颜色和指示器几何变动只需重绘，参与自绘测量的部件尺寸变动会触发重新测量。旧的 `measure_custom` 实现继续工作。

每个场景最多保存 12 份不同的非空部件样式，相同样式由多个节点共用。容量耗尽通过 `BuildError::PartStyles` 报告。诊断代码通过 `ViewDraft::part_style(key)` 或 `RetainedTree::part_style(key)` 读取合并结果，替代直接访问底层 `part_styles.get(key)`。使用桌面检查器点选页签，也能查看指示器几何、颜色、过渡和分隔线厚度。

### 控件状态动画与主题

控件声明时用 `theme.motion.state(state, local)` 解析当前作用域的规则，再用 `NodeProps::state_motion` 绑定状态。支持 `Pressed`、`Checked`、`Focused`（可见焦点）和 `Value`（按 min/max 归一化）。框架在提交成功后保存上一次目标。按稳定键复用的控件可以在动画中途反向。逻辑状态和操作立即生效，自绘时读取 `canvas.state.anim_progress`，范围为 0 至 1000。内置按钮、开关、复选框、单选项、滑块和进度条共用这条路径，拖动滑块时手柄直接跟随触点。

下面是包外控件声明一条局部规则所需的代码。已有的 `ThemedUi` 会把页面或父组件的主题传给子组件。

~~~rust
let motion = ui.theme().motion;
let local = StateMotion::new(MotionState::Checked, 480, Easing::Linear);
let props = ui.theme().control()
    .checked(selected)
    .clickable(true)
    .focusable(true)
    .state_motion(motion.state(MotionState::Checked, Some(local)));
ui.raw().custom(key, props)?;
~~~


- 可在主题上使用 `Motion::STANDARD`、`SNAPPY` 或 `REDUCED`，也可用 `ThemedUi::with_style` 为一组控件覆盖 `Style.motion`。根主题启用 `with_reduced(true)` 后，子作用域继承该偏好。当前控件在下次提交抵达终态，仍会显示选中、按下和焦点反馈。
- 控件可选内置缓动，或用 `StateMotion::with_curve` / `Motion::with_state_curve` 引用静态 `StyleCurve`。可自定义三次贝塞尔控制点或提供 17 个等间距采样值。每棵树可引用 `MAX_ANIMATIONS` 条不同的自定义曲线，重复引用共用一项，超限返回 `BuildError::MotionCurves`。内置缓动和时长不占这个配额。
- 页面、入场、退场使用 `Motion.page`、`entrance`、`exit`。调用 `PageTransition::play_styled(motion, role, local, now)` 解析预设、时长和曲线。退场自动倒放，节点需保留到结束。应用持有的转场在偏好改变时调用 `apply_motion(motion, now)`，即可在同一事务抵达对应的终帧。业务采样周期和提示停留时间由应用独立设置。

### 模型持有的动画

单个数值变化用 `Tween`，自绘控件可以把它的值解释为角度、颜色混合比例或仪表读数。整页或子树的常用动效用 `PageTransition`。两者都存入应用模型，在 `App::animations` 绑定到稳定键，由宿主原有的 `Engine::tick` 和 `wakeup` 驱动。例如，可以把一张卡片绑定到 `Key(42)`：

```rust
// App::animations
fn animations(m: &mut Self::Model, bind: &mut dyn FnMut(Key, &mut dyn Animation)) {
    bind(Key(42), &mut m.card);
}

// 在 update 中播放并返回 Change::View
m.card.start(TransitionKind::FloatUp, now, 280);

// 在 view 中声明并装饰卡片后
m.card.frame().apply(ui, Key(42))?;
```

引擎合并下一帧、业务定时器和按键重复的唤醒时间。动画延迟期间可以休眠，迟到的帧按单调时钟直接追到当前进度，最后一帧提交后停止唤醒。注册时也应列出属于其他页面的动画，框架会在节点退出已提交的树后取消它们。旧应用可以继续使用手工采样接口。迁移时删除对应的 `on_timer` 和 `next_deadline` 分支，在视图中读取 `Tween::current()` 或 `PageTransition::frame()`。重新调用 `animate_to` 会从事件发生时的取值续接，`cancel(now)` 会冻结当时的值。

| 需求 | 用法 |
|---|---|
| 内置动效 | `TransitionKind::ALL` 包含四向滑入、四向浮入并淡显、淡显、放大进入、缩小进入、弹出、脉冲、摇动、弹跳和翻转。 |
| 缓动 | `Easing::ALL` 列出 12 条曲线，包括线性、二次和三次缓入缓出、SmoothStep、SmootherStep、回弹超调、落地回跳和末端阶跃。`Curve::Bezier(CubicBezier::new(250,100,250,1000).unwrap())` 按时间轴求值。`Curve::Custom(fn(u16) -> i16)` 接受千分比时间，允许中间值超调。 |
| 延迟、倒放、重复、往返 | 用 `Timing { delay_ms, reverse, repeat, alternate, ..Timing::once(ms) }` 和 `PageTransition::play`。`Repeat::Times(n)` 计单程次数，往返一轮取 2。播放退出动效时，需保留节点到终帧，再由应用执行路由切换。 |
| 多段与并行动画 | 给 `Playback` 绑定节点，在视图中用同一个 `progress()` 采样多组 `Keyframes`。每组由调用方提供有序切片，首尾时间为 0 和 1000；每个键控制到下一个键的插值曲线。相邻键取相同值可让某个属性等待其他属性完成。 |
| 自定义转场 | `PageTransition::with_curve` 改变时间曲线，`play_custom` 接受从缓动进度到 `TransitionFrame` 的函数。也可以直接根据模型构造帧，或在自绘控件中驱动其他属性。 |

`TransitionFrame.layout` 的位移按父容器内容框的宽高计算，`grow` 围绕子节点中心改变布局框，子节点的绘制与命中随框移动。文字仍用原来的字号。增强档可叠加 4 位组透明度和绕 Y 轴的投影。基础档的淡显与翻转预设以尺寸变化表达，浮入保留位移。关键帧切片可以放在 Flash，播放和调度均无需堆分配。

### 主题系统与按能力细粒度配置

主题由正交、可独立替换和逐项微调的配置令牌组成：`ColorScheme`（画布、三级抬升表面、`border_strong`、主文字/次文字/禁用文字、`accent`、`danger`、`success`、`warning`、`focus`）、`Density`（控件高度、间距、内边距、标题/图元高度）、`Shape`（三档圆角与边框线宽）、`Typography`（标题/正文/说明字体槽）、`ElevationScale`（`Raised`/`Floating` 阴影偏移、模糊、透明度与色调抬升）、`GlassStyle`（毛玻璃模糊半径、透色遮罩颜色与透明度、高光边框）、`InteractionStyle`（`FocusRingStyle` 启用/线宽/外扩/圆角模式、`PressFeedback` 暗化/提亮/强调染色/禁用、模态遮罩透明度）、`Motion`（状态时长与曲线、页面/入场/退场样式、减少动态效果偏好、按压深度与提示停留时长）以及 `EffectsConfig`。

- 预设构造：`Theme::DARK`、`Theme::LIGHT` 与 `Theme::preset(ThemePreset::{Liuli, Classic, Industrial}, dark)`；`EffectTier::{Flat, Depth, Tinted, Glass}` 作为构造 `EffectsConfig` 的快捷挡位。
- 逐项独立覆盖：所有结构字段和单能力开关都可以单独调整，例如 `theme.with_shadows(false)`、`theme.with_press_depth(false)`、`theme.with_glass_blur(false)`、`theme.with_blur_radius(3)`、`theme.with_glass_tint(None, 8)`、`theme.with_rounded_focus(true)`、`theme.with_control_height(38)`、`theme.with_motion(Motion::SNAPPY)`，也保留 1.x `Theme::with(Style { .. })` 的局部覆盖兼容。
- 语义化容器与按钮变体：`SurfaceSpec` 统一组合 `Material::{Solid, Glass { .. }, Transparent}`、`Elevation::{Flat, Raised, Floating}` 与 `Compositor`（组透明度与 3D 投影）；`ButtonVariant` 提供 `Surface`、`Primary`、`Secondary`、`Outline`、`Ghost`、`Danger`。通过 `ThemedUi` 作用域构建器声明 `card` / `surface_column` / `button` 时，框架按 `EffectsConfig` 自动降级（例如关闭模糊但保留透色玻璃，或无图层后端时退回实色抬升卡片），同时保持 `Compositor` 变更可命中 `reuse_groups` 缓存。

焦点框默认跟随控件的 `radius`。例如按钮半径为 6 px、四周留有空间时，默认焦点框外扩 2 px，外侧半径为 8 px，描边宽 2 px。靠近裁剪边界时，圆角框会等量缩小四边的外扩距离。直角按钮使用直角框；主题或局部作用域可用 `with_rounded_focus(false)` 强制使用直角框。键盘或旋钮导航显示焦点框，触摸按下控件会隐藏它。

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

## 控件主体、焦点与命中

触摸设备和通用模拟器默认使用 `InitialFocusRing::OnNavigation`。首帧会选好逻辑焦点，当前页签仍显示选中指示器。首次键盘导航、旋钮步进或确认时显示焦点框，手指按下时隐藏，点空白处也生效。弹窗关闭后会恢复触发控件的焦点，并沿用最近的输入方式。

纯按键产品需要开机提示操作目标时，可以在构建首帧之前选用 `InitialFocusRing::Visible`。板端使用 `Engine::new_in_place_with_focus(storage, model, fonts, screen, now, InitialFocusRing::Visible)`。桌面使用 `AppSession::with_options`，传入 `SessionOptions { initial_focus: InitialFocusRing::Visible, ..Default::default() }`。运行中仍可通过 `try_set_focus_ring_visible` 调整提示。旧版 `.events` 回放显式选择可见策略，以复现已有首帧金图。

在 `NodeProps::geometry` 声明一份静态 `WidgetGeometry`，内置控件、第三方控件和检查器可使用同一份解析结果。主体支持跟随主题圆角的 `Surface`、直角矩形、指定圆角、居中圆盘、胶囊和带填充规则的静态路径。形状随实际布局尺寸解析。例如宽 80、高 40 的圆形按钮在中间占用 40×40 像素，布局仍占 80×40。普通形状可以共享 Flash 中的声明，使用它们的节点无需占用表面效果槽。

```rust
use liuli::prelude::*;

const CHIP: WidgetGeometry = WidgetGeometry::new(Outline::Capsule);
let props = NodeProps {
    width: Length::Px(80),
    height: Length::Px(32),
    focusable: true,
    focus_ring: FocusRingStyle::DEFAULT.with_gap(2),
    ..NodeProps::new().geometry(&CHIP)
};
```

默认焦点围住主体，触摸命中整个布局框。设 `hit: HitArea::Outline` 则按填充后的形状命中，孔洞允许点选后面的元素。旋钮使用完整圆盘，滑块使用手柄可扫过的区域，页签的键盘焦点与当前页指示器各自独立。组合控件可以通过 `FocusTarget::Part(&FocusPart { part, bounds, outline })` 指定部位、边界及轮廓。边界可以取内边距、每轴 0 到 1024 的相对区域，或随主题手柄尺寸变化的滑动范围。自绘回调可从 `canvas.body_outline()` 取主体边界，也可用 `RetainedTree::focus_outline(key)` 检查实际焦点目标与绘制范围。在桌面预览中按住 Shift 点选控件，检查器会列出焦点形状、部位、主体边界、实际外扩量、线宽和命中策略。

`FocusRingStyle::offset` 沿用外沿相对主体的距离；`with_gap` 表达主体到描边内沿的间隙。线宽为 1–3 个逻辑像素，线宽加间隙最多 7 像素。空间不足时统一收紧外扩量。路径外环在布局后的像素坐标中按填充区域作圆盘膨胀或腐蚀，使用 2×2 子像素采样；凹角、孔洞和非等比拉伸遵循同一规则。焦点环只参与绘制，旧、新绘制范围一起失效。未声明几何的控件继续使用原 `radius`、`FocusRingStyle` 和 `SurfaceComposition::outline`。显式圆盘、胶囊或路径优先于旧表面轮廓，`Outline::Surface` 会继承旧表面配置。命中默认继承 `SurfaceComposition::hit_outline`，也可以显式指定 `Bounds` 或 `Outline`。原先直接读取底层 `StoredProps::static_text` 或 `dyn_text` 的代码请改用 `StoredProps::text()`，按返回的 `TextRef` 区分静态和动态文字。直接构造底层 `PaintCmd::MaskGroup` 的后端需传入节点的 `slot`，以便通过 `SceneResources::geometry(slot)` 解析遮罩轮廓。自定义命令解释器还需处理路径焦点环、大圆角及其阴影。

## 路径、轮廓与子树遮罩

自绘控件使用 `PreparedPath<N>::prepare(verbs, steps)` 定义路径。坐标范围是每轴 0 到 1024，绘制时映射到控件给出的局部矩形。路径支持多段轮廓、直线、二次和三次贝塞尔曲线。填充可选非零环绕或奇偶规则，绘制开放路径时仅填充会自动闭合。描边宽度使用路径坐标单位，端点和接合处为圆形，图元输出裁在目标矩形内。曲线在准备阶段按每段 `steps` 细分，可按最大显示尺寸选择精度；边数超过 `N` 返回 `BuildError::PathEdges`。

固定图形可以在编译期准备，用 `WidgetCanvas::static_path` 提交一次绘制命令。动态路径使用 `WidgetCanvas::path`，引擎将边复制到有界命令表，每四条边占一个槽位，另需一个头部槽位。命令容量不足返回 `BuildError::Commands`。路径填充与描边使用每像素 2×2 覆盖采样。

主题表面通过 `SurfaceSpec::with_composition` 设置轮廓、子树裁剪、透明度遮罩与阴影来源。底层 `ViewBuilder::decorate` 和第三方控件可以传入同一份 `SurfaceComposition`。例如，带外阴影的卡片可以让内容沿主题圆角裁剪，并在底部 40 像素逐渐透明：

```rust
use liuli::prelude::*;
use liuli_widgets::SurfaceSpec;

const CARD: SurfaceComposition = SurfaceComposition {
    clip_content: true,
    mask: Mask::FadeBottom(40),
    ..SurfaceComposition::DEFAULT
};

let spec = SurfaceSpec::FLOATING.with_composition(&CARD);
```

`Outline::Surface` 读取控件解析后的圆角，`Outline::Path` 可复用填充用的静态路径和填充规则。开启 `hit_outline` 后，轮廓的孔洞也会从点选区域中扣除；默认保留矩形触摸目标。`Mask::Alpha` 可以引用静态 L565 位图的透明度，例如 A4 蒙版。遮罩作用于整组绘制完成的子树，重叠的子控件共同渐隐。基础档按每行 16 像素使用有界临时缓冲，支持嵌套裁剪和遮罩。

两档都支持 `ShadowSource::Outline`，从表面轮廓投影。增强档还支持 `ShadowSource::Content`，从合成后的子树透明度投影，适用于有透明孔洞的图标或图片。为基础档声明带内容投影的阴影会返回 `BuildError::UnsupportedEffect`。阴影可以延伸出布局框，最终输出仍受父级可见区域约束。引擎在表面移动或移除时同时考虑旧、新阴影范围。

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

若文本控件需要直接把借用的输入写入模型，可实现 `App::update_text`。M1–M6 演示的表单页将提交、移动光标和退格交给 `MultiLineState::apply_text`，省去了消息中的字符串副本。

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

一页有多个滚动容器时，通过 `page_restore(model, page).scroll_key` 指定要随页保存和恢复的容器。方向键将焦点移入嵌套视口时，发生移动的每一层都会触发一次 `on_scroll`，本次输入的模型更新合并为一次视图提交。


## 容量档位与声明式布局进阶

### 1. 编译期容量档位（无需堆分配）
通过 Cargo feature 在编译期静态调整节点池、文本池、命令缓冲与脏矩形预算，保持零 `alloc` 与静态 `.bss` 分配：

| 档位 Feature | `MAX_NODES` | `MAX_DEPTH` | `MAX_TEXT_BYTES` | `MAX_COMMANDS` | `MAX_DAMAGE` | `MAX_ANIMATIONS` |
|---|---|---|---|---|---|---|
| 默认 (`default`) | 64 | 12 | 1024 | 256 | 12 | 8 |
| `capacity-128` | 128 | 14 | 2048 | 512 | 16 | 12 |
| `capacity-256` | 250 | 16 | 4096 | 1024 | 24 | 16 |

32 位、64 节点配置的 `Storage` 编译上限为 67,456 字节。启用增强档图层后，三份效果表和两份绘制命令列表的元数据另有 256 字节预算。条带缓冲、引擎实例、应用模型和栈单独计量。

### 2. 链式 `NodeProps`、网格、横向滚动与 2D 焦点
- **`const fn` 构建器**：`NodeProps::new().width(...).height(...).padding(...).gap(...).justify(Justify::SpaceBetween).grid_cols(cols).bg(...).fg(...).accent(...).border(color, w).radius(r).focusable(true).clickable(true)`。
- **扩展容器与排版**：`Justify::{Start, Center, End, SpaceBetween, SpaceAround, SpaceEvenly}` 控制主轴分配；`Children::grid(key, cols, props, ...)` 构建固定列数网格；`Children::scroll_x(key, props, ...)` 支持横向滚动容器。
- **2D 方向导航与物理滚动**：`NavDir` 在树中执行几何方向焦点移动；`ScrollPhysics` 提供 Q8 定点滑动惯性、阻力衰减、越界回弹（`with_bounce`）和刻度吸附（`with_snap`）。

`scroll_x`、`scroll_y` 和可横滚的 Tabs 默认支持跟手拖动与抬手惯性。引擎在拖动超过阈值时按方向选择一个视口，从带单调时间的输入估算速度。停住手指后再松开会消除旧速度，新的触摸可以截停滑行。按键或旋钮移动焦点时，布局优先保证焦点可见。

容器可以用 `NodeProps::scroll_policy(ScrollPolicy::snap(158))` 按 158 像素间隔吸附，也可以用 `ScrollPolicy::DIRECT` 仅跟手滚动。主题化容器和 Tabs 继承减少动态效果偏好。自定义控件可用 `theme.scroll_policy(local)` 解析同一偏好。配置吸附的容器在减少动态效果时直接对齐最近的停靠点。虚拟列表通过 `App::on_scroll` 接收拖动和滑行的偏移，按需构建可见行；`virtual_list_with_policy` 支持局部策略。

需要按钮控制时，通过 `App::scroll_controller` 按容器键返回一个模型中的 `ScrollPhysics`。按钮可调用 `scroll_by_stop(1, now, true)` 前往下一项，或用 `scroll_to(offset, now, true)` 前往指定内容坐标。引擎提供测量边界、输入、动画时钟和唤醒调度。旧应用可移除这个控制器在 `App::animations` 或应用定时器中的重复注册。

### 3. 扩展控件、表单、图表与转场
`liuli_widgets` 新增高阶组件，全部保持 `#![no_std]` 与有界内存：
- **表单输入**（`inputs`）：`checkbox` / `paint_checkbox`、`radio` / `paint_radio`、`dropdown`（`DropdownState`）、`roller`（`RollerState`）与多行文本编辑器 `multi_line_editor` / `paint_multi_line_editor`（`MultiLineState`）。
- **数据展示**（`data`）：`data_table`（`TableColumn` 表头与斑马纹行选择）、`gauge` / `paint_gauge`（`GaugeSpec`）、`spinner` / `paint_spinner`、`rich_text` / `paint_rich_text`（多段彩色样式 `TextSpan`）、柱状图 `paint_bar_chart` 与多折线图 `paint_multi_line_chart`（`ChartSeries`）。
- **页面转场**使用 `PageTransition` 和 `TransitionFrame`。预设、组合与自定义用法见本页“动画”。

### 4. L565 v2 压缩纹理、渐变、旋转与硬件 2D 加速
- **位图打包 v2**：`liuli-image` 支持 `--v2`、`--rle`（行索引行程压缩）与 `--chroma-key <hex>`（透明色键）；对应渲染标志 `L565_FLAG_CHROMA_KEY`、`L565_FLAG_ALPHA4`、`L565_FLAG_RLE`。
- **流式解码与特效**：`StoredBitmap` 直接从 `ReadAt` 流式解包并逐条带绘制，无需整图驻留 RAM；`WidgetCanvas::gradient_rect`（`GRADIENT_HORIZONTAL` / `GRADIENT_VERTICAL` / `GRADIENT_DIAGONAL`）绘制平滑渐变矩形。
- **硬件屏幕旋转与 2D 加速**：`RotatedDisplaySink<S, MAX_PIXELS>` 按 `Rotation::{Deg0, Deg90, Deg180, Deg270}` 零堆分块旋转输出；`DrawAccel`（`SoftAccel` 为纯软件默认实现）结合 `render_band_with_accel` 可接入 MCU 2D 填充/混叠外设（如 DMA2D）。

### 5. 零堆多语言国际化（i18n）
- 运行时核心：`LocaleTable::new(code, entries)`、`TextKey(u16)`、`LocaleId(u8)`、`LocaleTable::resolve`（自动回退到默认语言）、`format_template`（支持 `{0}` 位置参数及 `{{`/`}}` 转义）以及 `plural_en` / `plural_cjk`。
- 离线编译检查：`python tools/liuli-i18n.py --input locales/ --output src/i18n_gen.rs --check` 校验各语言键完整性与占位符一致性，生成零堆只读 Rust 常量与静态表。

### 6. 开箱即用显示与触摸驱动（`liuli_render::drivers`）
实现薄适配器 `DisplayCmdBus` 与 `PanelConfig` 后即可直接使用内置 MIPI-DCS 面板驱动（均实现 `ByteWindowBus`，可直接配合 `ByteWindowSink` 使用）：
- **面板驱动**：`St7789`、`Ili9341`、`Gc9a01`、`Rm67162`。
- **触摸解码与校准**：`Gt911Decoder`、`Ft3168Decoder`、`Cst816Decoder` 与电阻屏固定点仿射校准器 `Xpt2046Calibrator`，可直接把原始寄存器数据喂给 `TouchTracker`。

### 7. 模拟器性能分析、热刷新与 Figma 工作流
- **实时性能分析与文件监听**：`host/liuli-sim` 提供 `FrameProfile`（保留最近 64 帧耗时与脏面积统计，`GET /api/profile`）与 `FileWatcher`（通过 `PreviewServer::watch_path` 注册，暴露 `GET /api/watch`）；浏览器预览面板提供“性能 HUD”开关与自动刷新轮询。
- **设计到代码**：参见 [Figma 设计到代码工作流](figma-workflow.md)。

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
