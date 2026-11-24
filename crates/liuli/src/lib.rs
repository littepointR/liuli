#![no_std]
#![forbid(unsafe_code)]

pub mod engine;
pub mod storage;
#[cfg(feature = "enhanced")]
pub use liuli_enhanced as enhanced;

pub use liuli_core as core;
pub use liuli_render as render;
pub use liuli_text as text;

pub use engine::Engine;
pub use storage::Storage;

pub mod prelude {
    pub use embedded_graphics_core::geometry::{Point, Size};
    pub use embedded_graphics_core::pixelcolor::{PixelColor, Rgb565};
    pub use embedded_graphics_core::primitives::Rectangle;

    pub use liuli_core::bitmap::Bitmap;
    pub use liuli_core::capacity::{
        BuildError, DEFAULT_BAND_HEIGHT, DEFAULT_BAND_PIXELS, DEFAULT_BAND_WIDTH,
        DEFAULT_SCREEN_HEIGHT, DEFAULT_SCREEN_WIDTH, DispatchError, FrameMetrics, LayoutError,
        MAX_ANIMATIONS, MAX_COMMANDS, MAX_DAMAGE, MAX_DEPTH, MAX_MODALS, MAX_NODES,
        MAX_SPARKLINE_SAMPLES, MAX_SPARKLINES, MAX_TEXT_BYTES, PresentError, PresentStatus,
        TimeError, TransactionError,
    };
    pub use liuli_core::damage::DamageSet;
    pub use liuli_core::effects::{Layer, PressDepth, Projection, Shadow, SurfaceStyle};
    pub use liuli_core::geometry::Rect;
    pub use liuli_core::input::TextInput;
    pub use liuli_core::input::{
        ActionKind, GesturePhase, HoldKey, NavDir, PointerCaptureMode, PointerEvent, PointerId,
        PointerPhase, RawInput, TransformGesture, UiAction,
    };
    pub use liuli_core::paint::{
        PaintOverride, PaintState, SceneResources, SliceSceneResources, TextId, WidgetCanvas,
    };
    pub use liuli_core::part_style::{
        IconPosition, LayoutExtra, VisualState, WidgetPart, WidgetPartStyle,
    };
    pub use liuli_core::time::{
        AnimProp, Easing, InstantMs, MAX_REPEAT_STEPS_PER_ADVANCE, NextWakeup, REPEAT_DELAY_MS,
        REPEAT_PERIOD_MS, Tween,
    };
    pub use liuli_core::view::StoredProps;
    pub use liuli_core::view::{
        Align, App, Change, Children, EdgeInsets, FocusRingStyle, FontId, IconKind, Key, Length,
        NodeKind, NodeProps, PageRestore, PaintMode, PressFeedback, SampleRing, StackAlign,
        SubpixelOrder, TextAlign, TextGamma, TextRef, TextStyleFlags, ViewBuilder, WrapMode,
    };
    pub use liuli_render::{
        AlignedBand, BandSlot, BufferTooSmall, ByteOrder, CACHE_LINE_BYTES, DmaCacheOps,
        DmaCoordinator, DmaSink, FreeBand, InFlightBand, NoCacheOps, PollOutcome,
        RECOMMENDED_SPI_CHUNK_BYTES, ReadyBand, SpiWindowBus, SpiWindowTarget, StartOutcome,
        TeError, TeGate, TeMode, TeWait, encode_rgb565,
    };
    pub use liuli_text::{
        BlockCache, FontCacheStats, FontFace, FontProvider, FontSet, FontSlot, GlyphPoolCache,
        HybridFontSet, Lf4Font, MetricsCache, SparseIndex, StreamingFont,
    };

    pub use crate::{Engine, Storage};
}
