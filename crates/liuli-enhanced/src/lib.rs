#![no_std]
#![forbid(unsafe_code)]
#[cfg(feature = "text")]
extern crate alloc;
#[cfg(feature = "field")]
pub mod field;
#[cfg(feature = "gestures")]
pub mod gestures;
#[cfg(feature = "text")]
pub mod glyphs;
#[cfg(feature = "text")]
mod text;
#[cfg(feature = "text")]
pub use text::*;
