//! Goodix status 0x814e; track ID at 0x814f, X at 0x8150 on RGB050M1.
use crate::hardware::{Pin, delay};
use liuli::prelude::{Point, PointerId, RawInput};

const SCL: Pin = Pin::Scl;
const SDA: Pin = Pin::Sda;
const INT: Pin = Pin::Interrupt;
const RESET: Pin = Pin::Reset;
fn high() -> Result<(), ()> {
    SCL.set(true);
    for _ in 0..250 {
        if SCL.get() {
            delay(3);
            return Ok(());
        }
        delay(1);
    }
    Err(())
}
fn start() -> Result<(), ()> {
    SDA.set(true);
    delay(3);
    high()?;
    if !SDA.get() {
        return Err(());
    }
    SDA.set(false);
    delay(3);
    SCL.set(false);
    Ok(())
}
fn stop() -> Result<(), ()> {
    SCL.set(false);
    SDA.set(false);
    delay(3);
    high()?;
    SDA.set(true);
    delay(3);
    Ok(())
}
fn pulse(bit: bool) -> Result<bool, ()> {
    SDA.set(bit);
    delay(3);
    high()?;
    let result = SDA.get();
    SCL.set(false);
    delay(2);
    Ok(result)
}
fn send(byte: u8) -> Result<(), ()> {
    for i in (0..8).rev() {
        pulse(byte & (1 << i) != 0)?;
    }
    if pulse(true)? { Err(()) } else { Ok(()) }
}
fn access(address: u8, reg: u16, read: &mut [u8]) -> Result<(), ()> {
    let result = (|| {
        start()?;
        send(address << 1)?;
        for b in reg.to_be_bytes() {
            send(b)?;
        }
        if read.is_empty() {
            send(0)?;
        } else {
            start()?;
            send(address << 1 | 1)?;
            let n = read.len();
            for (i, b) in read.iter_mut().enumerate() {
                *b = 0;
                for _ in 0..8 {
                    *b = (*b << 1) | u8::from(pulse(true)?);
                }
                pulse(i + 1 == n)?;
            }
        }
        Ok(())
    })();
    result.and(stop())
}

pub struct Touch {
    addr: u8,
    pub product: u32,
    width: u16,
    height: u16,
    previous: [Option<Point>; 16],
}
impl Touch {
    pub fn detect() -> Result<Self, ()> {
        RESET.set(false);
        RESET.mode(true, false);
        INT.set(false);
        INT.mode(true, false);
        delay(20000);
        RESET.set(true);
        delay(50000);
        INT.mode(false, false);
        delay(50000);
        for p in [SCL, SDA] {
            p.set(true);
            p.mode(true, true);
        }
        for _ in 0..9 {
            if SDA.get() {
                break;
            }
            SCL.set(false);
            delay(3);
            high()?;
        }
        stop()?;
        for addr in [0x5d, 0x14] {
            let mut id = [0; 10];
            if access(addr, 0x8140, &mut id).is_err() {
                continue;
            }
            if !matches!(&id[..4], b"911\0" | b"9110" | b"5688") {
                continue;
            }
            let width = u16::from_le_bytes([id[6], id[7]]);
            let height = u16::from_le_bytes([id[8], id[9]]);
            if !matches!((width, height), (800, 480) | (1024, 600)) {
                continue;
            }
            access(addr, 0x814e, &mut [])?;
            return Ok(Self {
                addr,
                product: u32::from_le_bytes(id[..4].try_into().unwrap()),
                width,
                height,
                previous: [None; 16],
            });
        }
        Err(())
    }
    fn sample(&self) -> Result<Option<[Option<Point>; 16]>, ()> {
        let mut status = [0];
        access(self.addr, 0x814e, &mut status)?;
        if status[0] & 0x80 == 0 {
            return Ok(None);
        }
        let count = usize::from(status[0] & 15);
        let mut points = [None; 16];
        let result = (|| {
            let mut bytes = [0; 8 * 10];
            let data = bytes.get_mut(..8 * count).ok_or(())?;
            if count > 0 {
                access(self.addr, 0x814f, data)?;
            }
            for b in data.chunks_exact(8) {
                let id = usize::from(b[0] & 15);
                let x = u16::from_le_bytes([b[1], b[2]]);
                let y = u16::from_le_bytes([b[3], b[4]]);
                if points[id].is_some() || x >= self.width || y >= self.height {
                    return Err(());
                }
                points[id] = Some(Point::new(
                    i32::from(x) * 800 / i32::from(self.width),
                    i32::from(y) * 480 / i32::from(self.height),
                ));
            }
            Ok(())
        })();
        let ack = access(self.addr, 0x814e, &mut []);
        result.and(ack).map(|()| Some(points))
    }
    pub fn poll(&mut self, mut input: impl FnMut(RawInput)) -> bool {
        let report = self.sample();
        let points = match report {
            Ok(None) => return true,
            Ok(Some(p)) => p,
            Err(()) => [None; 16],
        };
        for (i, old) in self.previous.iter().enumerate() {
            let id = PointerId(i as u8);
            if let (Some(pos), None) = (*old, points[i]) {
                input(if report.is_err() {
                    RawInput::PointerCancel { id }
                } else {
                    RawInput::PointerUp { id, pos }
                });
            }
        }
        for (i, &point) in points.iter().enumerate() {
            let id = PointerId(i as u8);
            if let Some(pos) = point {
                if self.previous[i].is_none() {
                    input(RawInput::PointerDown { id, pos });
                } else if self.previous[i] != point {
                    input(RawInput::PointerMove { id, pos });
                }
            }
        }
        self.previous = points;
        report.is_ok()
    }
}
