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

    pub use liuli_core::bitmap::{
        Bitmap, BitmapView, L565_FLAG_ALPHA4, L565_FLAG_CHROMA_KEY, L565_FLAG_RLE,
    };
    pub use liuli_core::capacity::{
        BuildError, DEFAULT_BAND_HEIGHT, DEFAULT_BAND_PIXELS, DEFAULT_BAND_WIDTH,
        DEFAULT_SCREEN_HEIGHT, DEFAULT_SCREEN_WIDTH, DispatchError, FrameMetrics, LayoutError,
        MAX_ANIMATIONS, MAX_COMMANDS, MAX_DAMAGE, MAX_DEPTH, MAX_MODALS, MAX_NODES,
        MAX_SPARKLINE_SAMPLES, MAX_SPARKLINES, MAX_TEXT_BYTES, NodeMask, PresentError,
        PresentStatus, TimeError, TransactionError,
    };
    pub use liuli_core::damage::DamageSet;
    pub use liuli_core::effects::{Layer, PressDepth, Projection, Shadow, SurfaceStyle};
    pub use liuli_core::geometry::{Rect, Rotation, rgb565_to_u16, u16_to_rgb565};
    pub use liuli_core::i18n::{
        LocaleId, LocaleTable, PluralForm, TemplateDisplay, TextKey, format_template, plural_cjk,
        plural_en,
    };
    pub use liuli_core::input::TextInput;
    pub use liuli_core::input::{
        ActionKind, GesturePhase, HoldKey, NavDir, PointerCaptureMode, PointerEvent, PointerId,
        PointerPhase, RawInput, TouchPoint, TouchTracker, TransformGesture, UiAction,
    };
    pub use liuli_core::paint::{
        GRADIENT_DIAGONAL, GRADIENT_HORIZONTAL, GRADIENT_VERTICAL, PaintOverride, PaintState,
        SceneResources, SliceSceneResources, TextId, WidgetCanvas,
    };
    pub use liuli_core::part_style::{
        IconPosition, LayoutExtra, VisualState, WidgetPart, WidgetPartStyle,
    };
    pub use liuli_core::time::{
        AnimProp, ClockTracker, Easing, InstantMs, MAX_REPEAT_STEPS_PER_ADVANCE, MonotonicClock32,
        NextWakeup, REPEAT_DELAY_MS, REPEAT_PERIOD_MS, ScrollPhysics, Tween,
    };
    pub use liuli_core::view::StoredProps;
    pub use liuli_core::view::{
        Align, App, Change, Children, EdgeInsets, FocusRingStyle, FontId, IconKind, Justify, Key,
        Length, NodeKind, NodeProps, PageRestore, PaintMode, PressFeedback, SampleRing, StackAlign,
        SubpixelOrder, TextAlign, TextGamma, TextRef, TextStyleFlags, ViewBuilder, WrapMode,
    };
    pub use liuli_render::{
        AlignedBand, BandMeta, BandSlot, BufferTooSmall, ByteOrder, ByteWindowBus, ByteWindowSink,
        CACHE_LINE_BYTES, DisplaySink, DmaCacheOps, DmaCoordinator, DmaSink, DrawAccel,
        EgDisplaySink, FrameError, FreeBand, InFlightBand, NoCacheOps, PollOutcome,
        RECOMMENDED_SPI_CHUNK_BYTES, RECOMMENDED_WIRE_CHUNK_BYTES, ReadyBand, RotatedDisplaySink,
        SealError, SoftAccel, SpiWindowBus, SpiWindowTarget, StartOutcome, StoredBitmap,
        StoredBitmapError, TeError, TeGate, TeMode, TeWait, encode_rgb565, encode_rgb565_in_place,
        encode_rgb565_words, render_band_with_accel,
    };
    pub use liuli_text::storage::{GlyphPixels, InlinePixels, IoMetrics};
    pub use liuli_text::{
        BlockCache, FontCacheStats, FontFace, FontProvider, FontSet, FontSlot, GlyphPoolCache,
        HybridFontSet, Lf4Font, MetricsCache, ReadAt, ReadError, SparseIndex, StoredFont,
        StreamingFont,
    };

    pub use crate::{Engine, Storage};
}
