use liuli_core::color::PixelFormat;
use liuli_core::geometry::{Point, Rect, Rotation};
use liuli_core::input::TouchPoint;

use crate::display::ByteWindowBus;

/// Low-level command + bulk pixel + delay interface implemented by SPI, QSPI, or parallel BSPs.
pub trait DisplayCmdBus {
    type Error;

    fn write_command(&mut self, cmd: u8, data: &[u8]) -> Result<(), Self::Error>;
    fn write_pixels(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
    fn delay_ms(&mut self, ms: u32);
}

/// Geometry and panel orientation descriptor shared across MIPI-DCS display controllers.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PanelConfig {
    pub width: u16,
    pub height: u16,
    pub x_offset: u16,
    pub y_offset: u16,
    pub rotation: Rotation,
    pub bgr: bool,
    pub invert: bool,
    pub pixel_format: Option<PixelFormat>,
}

impl PanelConfig {
    #[inline]
    pub const fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            x_offset: 0,
            y_offset: 0,
            rotation: Rotation::Deg0,
            bgr: false,
            invert: false,
            pixel_format: None,
        }
    }

    #[inline]
    pub const fn with_offset(mut self, x_offset: u16, y_offset: u16) -> Self {
        self.x_offset = x_offset;
        self.y_offset = y_offset;
        self
    }

    #[inline]
    pub const fn with_rotation(mut self, rotation: Rotation) -> Self {
        self.rotation = rotation;
        self
    }

    #[inline]
    pub const fn with_bgr(mut self, bgr: bool) -> Self {
        self.bgr = bgr;
        self
    }

    #[inline]
    pub const fn with_invert(mut self, invert: bool) -> Self {
        self.invert = invert;
        self
    }

    #[inline]
    pub const fn with_pixel_format(mut self, format: PixelFormat) -> Self {
        self.pixel_format = Some(format);
        self
    }
}

#[derive(Copy, Clone)]
struct ControllerSpec {
    madctl_by_rot: [u8; 4],
    init_cmds: &'static [(u8, &'static [u8], u32)],
    ramwr_cmd: u8,
    default_format: PixelFormat,
}

const MADCTL_DEFAULT: [u8; 4] = [0x00, 0x60, 0xC0, 0xA0];
const MADCTL_ILI9341: [u8; 4] = [0x40, 0x20, 0x80, 0xE0];

const ST7789_SPEC: ControllerSpec = ControllerSpec {
    madctl_by_rot: MADCTL_DEFAULT,
    init_cmds: &[
        (0x01, &[], 120),                           // SWRESET
        (0x11, &[], 120),                           // SLPOUT
        (0x3A, &[0x55], 0),                         // COLMOD RGB565 16-bit
        (0xB2, &[0x0C, 0x0C, 0x00, 0x33, 0x33], 0), // PORCTRL
        (0xB7, &[0x35], 0),                         // GCTRL
        (0xBB, &[0x19], 0),                         // VCOMS
        (0x13, &[], 0),                             // NORON
        (0x29, &[], 20),                            // DISPON
    ],
    ramwr_cmd: 0x2C,
    default_format: PixelFormat::Rgb565Be,
};

const ILI9341_SPEC: ControllerSpec = ControllerSpec {
    madctl_by_rot: MADCTL_ILI9341,
    init_cmds: &[
        (0x01, &[], 100), // SWRESET
        (0x11, &[], 120), // SLPOUT
        (0xCB, &[0x39, 0x2C, 0x00, 0x34, 0x02], 0),
        (0xCF, &[0x00, 0xC1, 0x30], 0),
        (0xE8, &[0x85, 0x00, 0x78], 0),
        (0xC0, &[0x23], 0), // PWCTRL1
        (0xC1, &[0x10], 0), // PWCTRL2
        (0x3A, &[0x55], 0), // COLMOD RGB565
        (0x29, &[], 20),    // DISPON
    ],
    ramwr_cmd: 0x2C,
    default_format: PixelFormat::Rgb565Be,
};

const ILI9488_SPEC: ControllerSpec = ControllerSpec {
    madctl_by_rot: MADCTL_ILI9341,
    init_cmds: &[
        (0x01, &[], 100),         // SWRESET
        (0x11, &[], 120),         // SLPOUT
        (0xC0, &[0x17, 0x15], 0), // PWCTRL1
        (0xC1, &[0x41], 0),       // PWCTRL2
        (0x3A, &[0x66], 0),       // COLMOD 18-bit RGB666 for SPI
        (0x29, &[], 20),          // DISPON
    ],
    ramwr_cmd: 0x2C,
    default_format: PixelFormat::Rgb666,
};

const GC9A01_SPEC: ControllerSpec = ControllerSpec {
    madctl_by_rot: MADCTL_DEFAULT,
    init_cmds: &[
        (0xEF, &[], 0),
        (0xEB, &[0x14], 0),
        (0xFE, &[], 0),
        (0xEF, &[], 0),
        (0x3A, &[0x05], 0), // COLMOD 16-bit RGB565
        (0x11, &[], 120),   // SLPOUT
        (0x29, &[], 20),    // DISPON
    ],
    ramwr_cmd: 0x2C,
    default_format: PixelFormat::Rgb565Be,
};

const RM67162_SPEC: ControllerSpec = ControllerSpec {
    madctl_by_rot: MADCTL_DEFAULT,
    init_cmds: &[
        (0x11, &[], 120),   // SLPOUT
        (0x3A, &[0x55], 0), // COLMOD RGB565 16-bit
        (0x53, &[0x20], 0), // WRCTRLD brightness control enabled
        (0x51, &[0xFF], 0), // WRDISBV max brightness
        (0x29, &[], 20),    // DISPON
    ],
    ramwr_cmd: 0x2C,
    default_format: PixelFormat::Rgb565Be,
};

struct MipiDcsPanel<B> {
    bus: B,
    cfg: PanelConfig,
    spec: ControllerSpec,
}

impl<B> MipiDcsPanel<B> {
    #[inline]
    const fn new(bus: B, cfg: PanelConfig, spec: ControllerSpec) -> Self {
        Self { bus, cfg, spec }
    }

    #[inline]
    fn bounds(&self) -> Rect {
        let logical = self
            .cfg
            .rotation
            .logical_size(liuli_core::geometry::Size::new(
                u32::from(self.cfg.width),
                u32::from(self.cfg.height),
            ));
        Rect::new(0, 0, logical.width, logical.height)
    }
}

impl<B: DisplayCmdBus> MipiDcsPanel<B> {
    fn pixel_format(&self) -> PixelFormat {
        self.cfg.pixel_format.unwrap_or(self.spec.default_format)
    }

    fn madctl_byte(&self, rot: Rotation) -> u8 {
        let base = self.spec.madctl_by_rot[rot as usize];
        if self.cfg.bgr || self.pixel_format().is_bgr() {
            base | 0x08
        } else {
            base
        }
    }

    fn init(&mut self) -> Result<(), B::Error> {
        for &(cmd, data, delay) in self.spec.init_cmds {
            if cmd == 0x3A
                && let Some(fmt) = self.cfg.pixel_format
                && let Some(colmod) = fmt.mipi_colmod()
            {
                let byte = if matches!(data, [0x05] | [0x06]) {
                    colmod & 0x0F
                } else {
                    colmod
                };
                self.bus.write_command(cmd, &[byte])?;
            } else {
                self.bus.write_command(cmd, data)?;
            }
            if delay > 0 {
                self.bus.delay_ms(delay);
            }
        }
        let inv_cmd = if self.cfg.invert { 0x21 } else { 0x20 };
        self.bus.write_command(inv_cmd, &[])?;
        self.set_rotation(self.cfg.rotation)
    }

    fn set_rotation(&mut self, rotation: Rotation) -> Result<(), B::Error> {
        self.cfg.rotation = rotation;
        let val = self.madctl_byte(rotation);
        self.bus.write_command(0x36, &[val])
    }

    fn set_window(&mut self, rect: Rect) -> Result<(), B::Error> {
        if rect.is_empty() {
            return Ok(());
        }
        let (x_off, y_off) = match self.cfg.rotation {
            Rotation::Deg0 | Rotation::Deg180 => (self.cfg.x_offset, self.cfg.y_offset),
            Rotation::Deg90 | Rotation::Deg270 => (self.cfg.y_offset, self.cfg.x_offset),
        };
        let x0 = (rect.x.max(0) as u16).saturating_add(x_off);
        let y0 = (rect.y.max(0) as u16).saturating_add(y_off);
        let x1 = x0.saturating_add((rect.w.saturating_sub(1)).min(u32::from(u16::MAX)) as u16);
        let y1 = y0.saturating_add((rect.h.saturating_sub(1)).min(u32::from(u16::MAX)) as u16);
        let x_bytes = [(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8];
        let y_bytes = [(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8];
        self.bus.write_command(0x2A, &x_bytes)?;
        self.bus.write_command(0x2B, &y_bytes)?;
        self.bus.write_command(self.spec.ramwr_cmd, &[])
    }
}

macro_rules! define_panel_driver {
    ($name:ident, $spec:expr, $doc:expr) => {
        #[doc = $doc]
        pub struct $name<B> {
            inner: MipiDcsPanel<B>,
        }

        impl<B> $name<B> {
            #[inline]
            pub const fn new(bus: B, cfg: PanelConfig) -> Self {
                Self {
                    inner: MipiDcsPanel::new(bus, cfg, $spec),
                }
            }

            #[inline]
            pub fn bounds(&self) -> Rect {
                self.inner.bounds()
            }

            #[inline]
            pub fn config(&self) -> &PanelConfig {
                &self.inner.cfg
            }

            #[inline]
            pub fn pixel_format(&self) -> PixelFormat {
                self.inner
                    .cfg
                    .pixel_format
                    .unwrap_or(self.inner.spec.default_format)
            }

            #[inline]
            pub fn bus(&self) -> &B {
                &self.inner.bus
            }

            #[inline]
            pub fn bus_mut(&mut self) -> &mut B {
                &mut self.inner.bus
            }

            #[inline]
            pub fn into_bus(self) -> B {
                self.inner.bus
            }
        }

        impl<B: DisplayCmdBus> $name<B> {
            #[inline]
            pub fn init(&mut self) -> Result<(), B::Error> {
                self.inner.init()
            }

            #[inline]
            pub fn set_rotation(&mut self, rotation: Rotation) -> Result<(), B::Error> {
                self.inner.set_rotation(rotation)
            }

            #[inline]
            pub fn set_window(&mut self, rect: Rect) -> Result<(), B::Error> {
                self.inner.set_window(rect)
            }
        }

        impl<B: DisplayCmdBus> ByteWindowBus for $name<B> {
            type Error = B::Error;

            #[inline]
            fn set_window(&mut self, rect: Rect) -> Result<(), Self::Error> {
                self.inner.set_window(rect)
            }

            #[inline]
            fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
                self.inner.bus.write_pixels(bytes)
            }
        }
    };
}

define_panel_driver!(
    St7789,
    ST7789_SPEC,
    "Turnkey ST7789 TFT controller driver implementing [`ByteWindowBus`]."
);
define_panel_driver!(
    Ili9341,
    ILI9341_SPEC,
    "Turnkey ILI9341 TFT controller driver implementing [`ByteWindowBus`]."
);
define_panel_driver!(
    Ili9488,
    ILI9488_SPEC,
    "Turnkey ILI9488 18-bit SPI/parallel TFT controller driver implementing [`ByteWindowBus`]."
);
define_panel_driver!(
    Gc9a01,
    GC9A01_SPEC,
    "Turnkey GC9A01 round TFT controller driver implementing [`ByteWindowBus`]."
);
define_panel_driver!(
    Rm67162,
    RM67162_SPEC,
    "Turnkey RM67162 QSPI AMOLED controller driver implementing [`ByteWindowBus`]."
);

impl<B: DisplayCmdBus> Rm67162<B> {
    /// Set AMOLED panel display brightness (`0..=255`) via MIPI-DCS `WRDISBV` (`0x51`).
    #[inline]
    pub fn set_brightness(&mut self, brightness: u8) -> Result<(), B::Error> {
        self.inner.bus.write_command(0x51, &[brightness])
    }
}

/// GT911 capacitive touch packet parser (status register `0x814E` + 8-byte point records at `0x8150`).
pub struct Gt911Decoder;

impl Gt911Decoder {
    pub const STATUS_REG: u16 = 0x814E;
    pub const POINTS_REG: u16 = 0x8150;
    pub const POINT_STRIDE: usize = 8;

    /// Returns `Some(count)` when buffer-ready bit `0x80` is set, or `None` if frame is not ready yet.
    pub fn decode(status: u8, points_buf: &[u8], out: &mut [TouchPoint]) -> Option<usize> {
        if (status & 0x80) == 0 {
            return None;
        }
        let touches = usize::from(status & 0x0F)
            .min(points_buf.len() / Self::POINT_STRIDE)
            .min(out.len());
        for (i, slot) in out[..touches].iter_mut().enumerate() {
            let p = &points_buf[i * Self::POINT_STRIDE..(i + 1) * Self::POINT_STRIDE];
            let id = p[0] & 0x0F;
            let x = i32::from(u16::from_le_bytes([p[1], p[2]]));
            let y = i32::from(u16::from_le_bytes([p[3], p[4]]));
            *slot = TouchPoint::new(id, Point::new(x, y));
        }
        Some(touches)
    }
}

/// FocalTech FT3168 / FT6x36 capacitive touch decoder starting at register `0x02` (`TD_STATUS`).
pub struct Ft3168Decoder;

impl Ft3168Decoder {
    pub const TD_STATUS_REG: u8 = 0x02;
    pub const POINT_STRIDE: usize = 6;

    pub fn decode(buf: &[u8], out: &mut [TouchPoint]) -> usize {
        let Some(&status) = buf.first() else {
            return 0;
        };
        let count = usize::from(status & 0x0F);
        let available = buf.len().saturating_sub(1) / Self::POINT_STRIDE;
        let limit = count.min(available).min(out.len());
        let mut written = 0usize;
        for i in 0..limit {
            let p = &buf[1 + i * Self::POINT_STRIDE..1 + (i + 1) * Self::POINT_STRIDE];
            let event = p[0] >> 6;
            // 0 = Press Down, 2 = Contact; 1 = Lift Up (skip so TouchTracker emits PointerUp).
            if event == 1 {
                continue;
            }
            let x = i32::from((u16::from(p[0] & 0x0F) << 8) | u16::from(p[1]));
            let id = p[2] >> 4;
            let y = i32::from((u16::from(p[2] & 0x0F) << 8) | u16::from(p[3]));
            out[written] = TouchPoint::new(id, Point::new(x, y));
            written += 1;
        }
        written
    }
}

/// CST816S/D/T single-point capacitive touch packet decoder (registers `0x01..=0x06`).
pub struct Cst816Decoder;

impl Cst816Decoder {
    pub const START_REG: u8 = 0x01;

    pub fn decode(buf: &[u8; 6]) -> Option<TouchPoint> {
        let finger_num = buf[1] & 0x0F;
        if finger_num == 0 {
            return None;
        }
        let event = buf[2] >> 6;
        if event == 1 {
            return None;
        }
        let x = i32::from((u16::from(buf[2] & 0x0F) << 8) | u16::from(buf[3]));
        let y = i32::from((u16::from(buf[4] & 0x0F) << 8) | u16::from(buf[5]));
        Some(TouchPoint::new(0, Point::new(x, y)))
    }
}

/// Pure fixed-point resistive touch calibrator (2-point axis-aligned or 3-point affine) for XPT2046.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Xpt2046Calibrator {
    ax: i64,
    bx: i64,
    cx: i64,
    ay: i64,
    by: i64,
    cy: i64,
    det: i64,
}

impl Xpt2046Calibrator {
    /// Axis-aligned calibration mapping two opposite raw ADC samples `[min, max]` to display points.
    pub fn from_two_points(
        raw_a: (u16, u16),
        raw_b: (u16, u16),
        screen_a: Point,
        screen_b: Point,
    ) -> Option<Self> {
        let dx_raw = i64::from(raw_b.0) - i64::from(raw_a.0);
        let dy_raw = i64::from(raw_b.1) - i64::from(raw_a.1);
        let det = dx_raw.checked_mul(dy_raw)?;
        if det == 0 {
            return None;
        }
        let dx_scr = i64::from(screen_b.x) - i64::from(screen_a.x);
        let dy_scr = i64::from(screen_b.y) - i64::from(screen_a.y);
        let ax = dx_scr * dy_raw;
        let cx = i64::from(screen_a.x) * det - i64::from(raw_a.0) * ax;
        let by = dy_scr * dx_raw;
        let cy = i64::from(screen_a.y) * det - i64::from(raw_a.1) * by;
        Some(Self {
            ax,
            bx: 0,
            cx,
            ay: 0,
            by,
            cy,
            det,
        })
    }

    /// Full 3-point affine calibration solving linear ADC `(x, y)` -> display `(x, y)` with rotation/skew.
    pub fn from_three_points(raw: [(u16, u16); 3], screen: [Point; 3]) -> Option<Self> {
        let x0 = i64::from(raw[0].0);
        let y0 = i64::from(raw[0].1);
        let x1 = i64::from(raw[1].0);
        let y1 = i64::from(raw[1].1);
        let x2 = i64::from(raw[2].0);
        let y2 = i64::from(raw[2].1);

        let det = (x0 - x2) * (y1 - y2) - (x1 - x2) * (y0 - y2);
        if det == 0 {
            return None;
        }

        let sx0 = i64::from(screen[0].x);
        let sx1 = i64::from(screen[1].x);
        let sx2 = i64::from(screen[2].x);
        let ax = (sx0 - sx2) * (y1 - y2) - (sx1 - sx2) * (y0 - y2);
        let bx = (x0 - x2) * (sx1 - sx2) - (sx0 - sx2) * (x1 - x2);
        let cx =
            y0 * (x2 * sx1 - x1 * sx2) + y1 * (x0 * sx2 - x2 * sx0) + y2 * (x1 * sx0 - x0 * sx1);

        let sy0 = i64::from(screen[0].y);
        let sy1 = i64::from(screen[1].y);
        let sy2 = i64::from(screen[2].y);
        let ay = (sy0 - sy2) * (y1 - y2) - (sy1 - sy2) * (y0 - y2);
        let by = (x0 - x2) * (sy1 - sy2) - (sy0 - sy2) * (x1 - x2);
        let cy =
            y0 * (x2 * sy1 - x1 * sy2) + y1 * (x0 * sy2 - x2 * sy0) + y2 * (x1 * sy0 - x0 * sy1);

        Some(Self {
            ax,
            bx,
            cx,
            ay,
            by,
            cy,
            det,
        })
    }

    #[inline]
    pub fn map(&self, raw_x: u16, raw_y: u16) -> Point {
        let rx = i64::from(raw_x);
        let ry = i64::from(raw_y);
        let num_x = self.ax * rx + self.bx * ry + self.cx;
        let num_y = self.ay * rx + self.by * ry + self.cy;
        let sign = self.det.signum();
        let den = self.det.abs();
        let x = ((num_x * sign + den / 2 * num_x.signum() * sign) / den) as i32;
        let y = ((num_y * sign + den / 2 * num_y.signum() * sign) / den) as i32;
        Point::new(x, y)
    }
}
