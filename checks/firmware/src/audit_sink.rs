use core::convert::Infallible;
use core::hint::black_box;

use liuli::prelude::*;

pub struct AuditSpiBus {
    windows: u32,
    bytes: u32,
    flushes: u32,
    digest: u32,
}

impl AuditSpiBus {
    pub const fn new() -> Self {
        Self {
            windows: 0,
            bytes: 0,
            flushes: 0,
            digest: 0x811C_9DC5,
        }
    }

    pub const fn summary(&self) -> (u32, u32, u32, u32) {
        (self.windows, self.bytes, self.flushes, self.digest)
    }
}

impl SpiWindowBus for AuditSpiBus {
    type Error = Infallible;

    fn set_window(&mut self, rect: Rect) -> Result<(), Self::Error> {
        self.windows = self.windows.saturating_add(1);
        self.digest ^= (rect.x as u32).wrapping_mul(31)
            ^ ((rect.y as u32) << 8)
            ^ ((rect.w as u32) << 16)
            ^ ((rect.h as u32) << 24);
        black_box(self.digest);
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.bytes = self.bytes.saturating_add(bytes.len() as u32);
        let mut h = self.digest;
        for &b in bytes {
            h = h.wrapping_mul(16777619) ^ (b as u32);
        }
        self.digest = h;
        black_box(self.digest);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.flushes = self.flushes.saturating_add(1);
        black_box((self.windows, self.bytes, self.flushes, self.digest));
        Ok(())
    }
}

pub struct AuditSink<'a> {
    in_flight: Option<InFlightBand<'a>>,
    pending_polls: u8,
    slot_mask: u8,
    started_count: u32,
    completed_count: u32,
    seq_xor: u32,
    pixel_digest: u32,
    cache: NoCacheOps,
}

impl<'a> AuditSink<'a> {
    pub const fn new() -> Self {
        Self {
            in_flight: None,
            pending_polls: 0,
            slot_mask: 0,
            started_count: 0,
            completed_count: 0,
            seq_xor: 0,
            pixel_digest: 0x811C_9DC5,
            cache: NoCacheOps,
        }
    }

    pub const fn slot_mask(&self) -> u8 {
        self.slot_mask
    }

    pub const fn completed_count(&self) -> u32 {
        self.completed_count
    }

    pub const fn summary(&self) -> (u8, u32, u32, u32, u32) {
        (
            self.slot_mask,
            self.started_count,
            self.completed_count,
            self.seq_xor,
            self.pixel_digest,
        )
    }
}

impl<'a> DmaSink<'a> for AuditSink<'a> {
    type Error = Infallible;

    fn try_start(&mut self, band: ReadyBand<'a>) -> StartOutcome<'a, Self::Error> {
        if self.in_flight.is_some() {
            return StartOutcome::Busy(band);
        }
        let addr = band.pixels().as_ptr() as usize;
        let len_bytes = band.valid_bytes();
        self.cache.clean_dcache_range(addr, len_bytes);
        self.cache.data_sync_barrier();

        self.slot_mask |= 1u8 << (band.slot() as u8);
        self.started_count = self.started_count.saturating_add(1);
        self.pending_polls = 1;
        self.in_flight = Some(band.into_in_flight());
        black_box((self.slot_mask, self.started_count));
        StartOutcome::Started
    }

    fn poll_complete(&mut self) -> PollOutcome<'a, Self::Error> {
        if self.pending_polls > 0 {
            self.pending_polls -= 1;
            return PollOutcome::Pending;
        }
        let Some(band) = self.in_flight.take() else {
            return PollOutcome::Pending;
        };
        let meta = band.meta();
        let mut h = self.pixel_digest;
        for &px in band.pixels() {
            h = h.wrapping_mul(16777619) ^ (px as u32);
        }
        self.pixel_digest = h;
        self.completed_count = self.completed_count.saturating_add(1);
        self.seq_xor ^= meta.seq.wrapping_add(1);
        black_box((self.completed_count, self.pixel_digest));
        PollOutcome::Completed {
            slot: meta.slot,
            frame: meta.frame,
            seq: meta.seq,
            band: band.confirm_stopped(),
        }
    }
}
