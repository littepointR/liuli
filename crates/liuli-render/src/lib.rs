#![no_std]
#![forbid(unsafe_code)]

#[cfg(all(test, feature = "layers"))]
extern crate alloc;

pub mod band;
pub mod dma;
mod effects;
pub mod raster;
pub mod spi;
pub mod sync;
pub mod vector;
pub mod wire;

pub use band::{BandIter, BandTarget};
pub use dma::{
    AlignedBand, BandMeta, BandSlot, CACHE_LINE_BYTES, DmaCacheOps, DmaCoordinator, DmaSink,
    FreeBand, InFlightBand, NoCacheOps, PollOutcome, ReadyBand, SealError, StartOutcome, TeError,
    TeGate, TeMode, TeWait,
};
pub use raster::{draw_eg, render_band, render_band_with_provider};
pub use spi::{RECOMMENDED_SPI_CHUNK_BYTES, SpiWindowBus, SpiWindowTarget};
pub use sync::{FrameError, present_sync, present_sync_with, present_sync_with_provider};
pub use wire::{BufferTooSmall, ByteOrder, encode_rgb565};
