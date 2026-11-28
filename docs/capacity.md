# 从组件声明生成固件容量

零堆应用可以随模型创建、删除和重排控件。固件为同时存在的最大视图保留内存。用业务 ID 生成稳定键，删除前一行后，其他行仍使用原键。

下面的视图允许最多八条连接。每条连接可以换成第三方包返回的复合组件。内置组件和外部控件包都实现 `liuli::component::Component`。

```rust
use liuli::component::{Bounded, Component, leaf};
use liuli::prelude::*;

fn connections(ids: &[u32]) -> impl Component {
    Bounded::<8>::rows(Key(20), ids.iter().copied(), |id| {
        leaf(Key(100 + id), NodeKind::Button, || {
            NodeProps::new().text(TextRef::Static("Connect"))
        })
    })
}
```

应用通过 `fn view(model: &Model) -> impl Scene` 返回整页声明。例如，`scene(Key(1), NodeProps::new(), connections(&model.ids))` 把上面的组件挂到根节点。容量示例直接读取该应用入口。静态组合可用 `AppType::view(&model).static_demand().unwrap()`，含作用域布局的应用可在宿主使用 `liuli::scene::capacity::<AppType>(&model)`。固件、模拟器和控件包使用同一个视图。以下命令在离线 SDK 根目录运行。

```sh
python tools/build-ui.py --app device-ui/Cargo.toml --plan-only --offline
python tools/build-ui.py --app device-ui/Cargo.toml --offline -- --release
python tools/build-ui.py --app device-ui/Cargo.toml --offline --action run -- --example preview -- --headless
python device-ui/board/build.py --profile basic --offline --out target/starter
```

| 声明 | 峰值计算 |
|---|---|
| 元组、定长数组 | 同级子树求和，深度取最大值 |
| 容器、根节点 | 加一个节点和一层 |
| `Option<C>`、`Either<A,B>` | 包含可选项，互斥项按每种资源分别取最大值 |
| `Bounded::<N>::rows` | 行组件容量乘业务上限 N |
| `VirtualRows::<RECORDS, ROW_HEIGHT, MAX_VIEWPORT>::rows` | 按最大视口、行高、部分可见行和预加载行计算，受记录总数限制，另计滚动容器和两个占位节点 |

若过渡时要同时显示两页，应把它们声明为并存子树。通用叶子允许运行时选择节点种类，每叶保守预留一个文字槽。业务数量或视口超上限时，事务返回 `ItemLimit`，`build_failure()` 提供页面和键。上次提交的树和命令继续有效。应用修正模型后可再次提交。

构建配置作用于整份固件。节点索引支持最多 250 个同时存在的节点，可通过虚拟化支撑更多记录。双树支持失败恢复，运行时删除控件会腾出槽位，已链接数组保持固定大小。初始化引擎时，编译器校验产品已提供应用计划。宿主测试可在开发依赖显式启用 `host-testing`，以便同一测试二进制运行多种正常和故障场景。MCU 固件始终要求产品计划。单节点控件可以用 `component::single(key, |ui| ...)` 直接接入，包括需要设置 revision 或装饰的控件。选择器、下拉框、表格和页签布局也有 `liuli_widgets::components` 接口，以选项数、行数等业务值推导。新复合控件使用 `container`、`leaf`、`text_leaf` 和组合器；底层实现也可自行实现 `Component` 并负责其峰值声明。

作用域布局用 `layout(key, props, |ui| ...)` 返回一个场景。添加有业务上限的组件用 `ui.show(Bounded::<8>::rows(...))`。规划时即使迭代器为空，也会计入八行的完整容量。改变树结构的互斥分支用 `ui.variant(current, &[...])`，可并存的条件子树用 `ui.optional(visible)`。宿主枚举所有声明的分支；普通 Rust 的 `if` 适合仅修改色彩、文本等值的条件。Tabs 从注册的页面表遍历每个内容页，虚拟列表计入部分可见行和预加载。固件编译剔除宿主遍历状态。构建多种屏幕尺寸或可切换多个应用的桌面程序时，对各场景的需求逐项取最大值。

板包读取自身的视口和功能配置。单独检查 S3 竖屏 Widget Demo 容量可以运行 `python tools/build-ui.py --app liuli-widget-demo/Cargo.toml --app-features enhanced --plan-only --plan-arg showcase --plan-arg 240 --plan-arg 536 --offline`。SDK 中的桌面 Widget Demo 则覆盖 Showcase、Studio 和两种屏幕。开发者可用 `--action run` 或 `--action test` 在同样的容量下运行回放。独立示例可用 `--plan-example` 指定其宿主规划入口。

绘制命令数、动态文字字节、图层像素和业务模型数组有各自预算。一个自绘节点可以输出多条命令，这些预算独立于节点树。

默认容量示例为 `examples/capacity.rs`。多包工作区可在每个包的 `[package.metadata.liuli]` 中设置独立的 `capacity-example` 名称，避免 Cargo 在同一输出目录生成同名可执行文件。

本次参考构建的实际静态容量如下。桌面组合程序覆盖两个应用和两种屏幕，独立固件只计入所选应用与该板的视口。

| 产品 | 节点 | 深度 | 文字槽 |
|---|---:|---:|---:|
| S3 Widget Demo，240 × 536 | 57 | 10 | 41 |
| H743 Widget Demo，800 × 480 | 59 | 9 | 43 |
| 增强桌面 Widget Demo，含 Studio | 66 | 10 | 43 |
| Cortex-M 终端 | 44 | 8 | 23 |
| S3 告警 | 29 | 4 | 24 |

## 选择产品能力

Cargo 特性 `owned-images`、`layers`、`multitouch`、`complex-text`、`text-field` 可以分别启用，`enhanced` 提供组合预设。启用 `part-painters` 后，可在主题和实例的 `WidgetPartStyle` 中注册静态 `PartPainter`。回调获得部件角色、实测边界、当前状态和受裁剪的画布。滑动指示器使用页签条坐标和选中页签状态。第三方角色使用 `WidgetPart::Custom`。渲染后端可通过 `Engine::present_with_accel` 和 `poll_dma_with_accel` 注入 `DrawAccel`。

产品可在 Cargo 配置的 `[env]` 中设置带 `LIULI_` 前缀的预算和开关。完整范围和独立预算默认值见[构建定义](../crates/liuli-core/build.rs)。

| 类别 | 参数 |
|---|---|
| 资源预算 | `MAX_COMMANDS`、`MAX_TEXT_BYTES`、`MAX_DAMAGE`、`MAX_EFFECTS`、`MAX_ANIMATIONS`、`MAX_PART_STYLES`、`MAX_LAYOUT_EXTRAS`、`MAX_SPARKLINES`、`MAX_SPARKLINE_SAMPLES` |
| 0 或 1 开关 | `MASKS`、`SHADOWS`、`ICONS`、`TAB_INDICATOR`（滑动标记）、`SCROLL`（滚动物理）、`POINTER`、`REPEAT`、`METRICS`、`DMA`（引擎异步传输会话） |
| 交互和布局 | `DRAG_THRESHOLD_PX`、`REPEAT_DELAY_MS`、`REPEAT_PERIOD_MS`、`SAMPLE_INTERVAL_MS`、`FRICTION_NUM`、`MAX_CUSTOM_CHILDREN`、`MAX_CLIP_DEPTH`、`FOCUS_HISTORY` |
| 字体与输入法 | `FONT_SLOTS`、`KERN_CACHE_SLOTS`、`IME_PREEDIT_BYTES`、`IME_DISPLAY_BYTES`、`IME_CANDIDATES` |

减少动态效果是运行时偏好，静态开关决定内存布局。关闭指标后计数查询返回零。字体角色保留原有常量，扩展角色可用 `FontId::from_raw` 指定，使用 `with_slot` 前为该索引配置足够的槽位。检查器通过 `tree.text_cache(slot)` 读取文字缓存，趋势线池通过 `count()` 返回长度。

SDK 内的 `capacity-consumer` 使用独立控件包，验证增删、重排、弹层、错误恢复和千条虚拟列表。仓库的[链接探针](../checks/cost-probe/measure.py)比较同一程序五种配置的 Flash 和静态 RAM。
