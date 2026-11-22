#![no_std]
#![forbid(unsafe_code)]

pub mod engine;
pub mod storage;

pub use liuli_core as core;
pub use liuli_render as render;
pub use liuli_text as text;

pub use engine::Engine;
pub use storage::Storage;

pub mod prelude {
    pub use embedded_graphics_core::geometry::{Point, Size};
    pub use embedded_graphics_core::pixelcolor::{PixelColor, Rgb565};
    pub use embedded_graphics_core::primitives::Rectangle;

    pub use liuli_core::capacity::{
        BuildError, DEFAULT_BAND_HEIGHT, DEFAULT_BAND_PIXELS, DEFAULT_BAND_WIDTH,
        DEFAULT_SCREEN_HEIGHT, DEFAULT_SCREEN_WIDTH, DispatchError, FrameMetrics, LayoutError,
        MAX_ANIMATIONS, MAX_COMMANDS, MAX_DAMAGE, MAX_DEPTH, MAX_MODALS, MAX_NODES, MAX_TEXT_BYTES,
        PresentError, PresentStatus, TimeError,
    };
    pub use liuli_core::damage::DamageSet;
    pub use liuli_core::geometry::Rect;
    pub use liuli_core::input::{ActionKind, NavDir, PointerId, RawInput, UiAction};
    pub use liuli_core::time::{InstantMs, NextWakeup};
    pub use liuli_core::view::{
        Align, App, Change, EdgeInsets, IconKind, Key, Length, NodeKind, NodeProps, TextRef,
        ViewBuilder,
    };
    pub use liuli_render::{
        BandSlot, DmaCoordinator, DmaSink, FreeBand, PollOutcome, ReadyBand, StartOutcome,
    };
    pub use liuli_text::Lf4Font;

    pub use crate::{Engine, Storage};
}
