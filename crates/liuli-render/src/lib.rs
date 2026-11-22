#![no_std]
#![forbid(unsafe_code)]

pub mod band;
pub mod dma;
pub mod raster;
pub mod sync;

pub use band::{BandIter, BandTarget};
pub use dma::{
    BandMeta, BandSlot, DmaCoordinator, DmaSink, FreeBand, InFlightBand, PollOutcome, ReadyBand,
    SealError, StartOutcome,
};
pub use raster::{draw_eg, render_band};
pub use sync::{present_sync, present_sync_with};
