//! Optional UART envelope. Its payload is the existing desktop trace syntax.
use core::ffi::{c_char, c_int};
use liuli::prelude::{InstantMs, RawInput};

unsafe extern "C" {
    fn printf(format: *const c_char, ...) -> c_int;
}

pub(super) struct Recorder {
    origin: InstantMs,
    sequence: u32,
}
impl Recorder {
    pub fn start(origin: InstantMs) -> Self {
        unsafe { printf(c"LIULI_TRACE 0 liuli-trace 1\n".as_ptr()) };
        Self {
            origin,
            sequence: 0,
        }
    }
    pub fn tick(&mut self, now: InstantMs) {
        self.sequence = self.sequence.wrapping_add(1);
        unsafe {
            printf(
                c"LIULI_TRACE %u %llu tick\n".as_ptr(),
                self.sequence,
                now.saturating_sub(self.origin),
            )
        };
    }
    pub fn input(&mut self, now: InstantMs, input: RawInput) {
        let elapsed = now.saturating_sub(self.origin);
        let (kind, id, pos) = match input {
            RawInput::PointerDown { id, pos } => (c"down", id, Some(pos)),
            RawInput::PointerMove { id, pos } => (c"move", id, Some(pos)),
            RawInput::PointerUp { id, pos } => (c"up", id, Some(pos)),
            RawInput::PointerCancel { id } => (c"cancel", id, None),
            _ => return,
        };
        self.sequence = self.sequence.wrapping_add(1);
        if let Some(pos) = pos {
            unsafe {
                printf(
                    c"LIULI_TRACE %u %llu kind=%s&v=%u&x=%d&y=%d\n".as_ptr(),
                    self.sequence,
                    elapsed,
                    kind.as_ptr(),
                    u32::from(id.0),
                    pos.x,
                    pos.y,
                )
            };
        } else {
            unsafe {
                printf(
                    c"LIULI_TRACE %u %llu kind=cancel&v=%u\n".as_ptr(),
                    self.sequence,
                    elapsed,
                    u32::from(id.0),
                )
            };
        }
    }
}
