#![no_std]
#![forbid(unsafe_code)]

pub mod band;
pub mod dma;
pub mod raster;
pub mod spi;
pub mod sync;
pub mod wire;

pub use band::{BandIter, BandTarget};
pub use dma::{
    AlignedBand, BandMeta, BandSlot, CACHE_LINE_BYTES, DmaCacheOps, DmaCoordinator, DmaSink,
    FreeBand, InFlightBand, NoCacheOps, PollOutcome, ReadyBand, SealError, StartOutcome, TeError,
    TeGate, TeMode, TeWait,
};
pub use raster::{draw_eg, render_band};
pub use spi::{RECOMMENDED_SPI_CHUNK_BYTES, SpiWindowBus, SpiWindowTarget};
pub use sync::{present_sync, present_sync_with};
pub use wire::{BufferTooSmall, ByteOrder, encode_rgb565};
