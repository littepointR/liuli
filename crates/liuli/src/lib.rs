#![no_std]
#![forbid(unsafe_code)]

pub mod engine;
mod scroll;
pub mod storage;
#[cfg(any(feature = "complex-text", feature = "multitouch"))]
pub use liuli_enhanced as enhanced;

pub use liuli_core as core;
pub use liuli_core::component;
pub use liuli_core::scene;
pub use liuli_render as render;
pub use liuli_text as text;

pub use engine::{Engine, InitialFocusRing};
pub use storage::Storage;

pub mod prelude {
    pub use embedded_graphics_core::geometry::{Point, Size};
    pub use embedded_graphics_core::pixelcolor::{PixelColor, Rgb565};
    pub use embedded_graphics_core::primitives::Rectangle;
    pub use liuli_core::scene::{Scene, layout, scene};

    pub use liuli_core::animation::{
        Animation, CubicBezier, Curve, Keyframe, Keyframes, Playback, Repeat, StyleCurve, Timing,
    };
    pub use liuli_core::bitmap::{
        Bitmap, BitmapView, L565_FLAG_ALPHA4, L565_FLAG_CHROMA_KEY, L565_FLAG_INDEXED4,
        L565_FLAG_INDEXED8, L565_FLAG_MASK_A4, L565_FLAG_RLE,
    };
    pub use liuli_core::capacity::{
        BuildError, DEFAULT_BAND_HEIGHT, DEFAULT_BAND_PIXELS, DEFAULT_BAND_WIDTH,
        DEFAULT_SCREEN_HEIGHT, DEFAULT_SCREEN_WIDTH, DispatchError, FrameMetrics, LayoutError,
        MAX_ANIMATIONS, MAX_COMMANDS, MAX_DAMAGE, MAX_DEPTH, MAX_MODALS, MAX_NODES,
        MAX_SPARKLINE_SAMPLES, MAX_SPARKLINES, MAX_TEXT_BYTES, NodeMask, PresentError,
        PresentStatus, TimeError, TransactionError,
    };
    pub use liuli_core::color::{
        Gray8, Hsl, Hsv, PixelFormat, Rgb888, Rgba8888, contrast_ratio_x10, darken_rgb565,
        gray8_to_rgb565, hsl_to_rgb565, hsl_to_rgb888, hsv_to_rgb565, hsv_to_rgb888, invert_rgb565,
        lighten_rgb565, mix_rgb565, parse_hex_str, relative_luminance_scaled, rgb565_from_hex,
        rgb565_swap_rb, rgb565_to_gray8, rgb565_to_rgb888, rgb565_u16_to_abgr8888,
        rgb565_u16_to_argb8888, rgb565_u16_to_rgb666_bytes, rgb565_u16_to_rgb888, rgb888_from_hex,
        rgb888_to_gray8, rgb888_to_hsl, rgb888_to_hsv, rgb888_to_rgb565_round,
        rgb888_to_rgb565_trunc,
    };
    pub use liuli_core::composition::{
        FocusPart, FocusTarget, HitArea, Mask, Outline, PartBounds, ShadowSource,
        SurfaceComposition, WidgetGeometry,
    };
    pub use liuli_core::damage::DamageSet;
    pub use liuli_core::dither::{
        DitherMode, bayer_4x4, gray8_to_gray4_dithered, gray8_to_mono1_dithered,
        rgb565_lerp_dithered, rgb888_to_rgb565_dithered,
    };
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
    #[cfg(feature = "part-painters")]
    pub use liuli_core::paint::{PaintPart, PartPainter};
    pub use liuli_core::part_style::{
        IconPosition, IndicatorLayout, IndicatorMotion, IndicatorShape, IndicatorSpan, LayoutExtra,
        LayoutMotion, PartMetrics, VisualState, WidgetPart, WidgetPartStyle,
    };
    pub use liuli_core::path::{FillRule, Path, PathPaint, PathVerb, PreparedPath};
    pub use liuli_core::time::{
        AnimProp, ClockTracker, Easing, InstantMs, MAX_REPEAT_STEPS_PER_ADVANCE, MonotonicClock32,
        NextWakeup, REPEAT_DELAY_MS, REPEAT_PERIOD_MS, ScrollPhysics, Tween,
    };
    pub use liuli_core::view::{
        Align, App, Change, Children, EdgeInsets, FocusRingStyle, FontId, IconKind, Justify, Key,
        Length, MotionState, NodeKind, NodeProps, PageRestore, PaintMode, PressFeedback,
        SampleRing, ScrollPolicy, StackAlign, StateMotion, StoredProps, SubpixelOrder, TextAlign,
        TextGamma, TextRef, TextStyleFlags, ViewBuilder, WrapMode,
    };
    pub use liuli_render::{
        AlignedBand, BandMeta, BandSlot, BufferTooSmall, ByteOrder, ByteWindowBus, ByteWindowSink,
        CACHE_LINE_BYTES, Cst816Decoder, DisplayCmdBus, DisplaySink, DmaCacheOps, DmaCoordinator,
        DmaSink, DrawAccel, EgDisplaySink, FrameError, FreeBand, Ft3168Decoder, Gc9a01,
        Gt911Decoder, Ili9341, Ili9488, InFlightBand, NoCacheOps, PanelConfig, PixelArgb8888,
        PixelRgb565, PollOutcome, RECOMMENDED_SPI_CHUNK_BYTES, RECOMMENDED_WIRE_CHUNK_BYTES,
        ReadyBand, Rm67162, RotatedDisplaySink, SealError, SoftAccel, SpiWindowBus,
        SpiWindowTarget, St7789, StartOutcome, StoredBitmap, StoredBitmapError, SurfacePixel,
        TeError, TeGate, TeMode, TeWait, TypedBandTarget, Xpt2046Calibrator,
        byte_order_to_pixel_format, encode_rgb565, encode_rgb565_in_place, encode_rgb565_row,
        encode_rgb565_words, render_band_with_accel,
    };
    pub use liuli_text::storage::{GlyphPixels, InlinePixels, IoMetrics};
    pub use liuli_text::{
        BlockCache, FontCacheStats, FontFace, FontProvider, FontSet, FontSlot, GlyphPoolCache,
        HybridFontSet, Lf4Font, MetricsCache, ReadAt, ReadError, SparseIndex, StoredFont,
        StreamingFont,
    };

    pub use crate::{Engine, InitialFocusRing, Storage};
}
