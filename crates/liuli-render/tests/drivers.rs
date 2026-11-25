use liuli_core::geometry::{Point, Rect, Rotation};
use liuli_core::input::TouchPoint;
use liuli_render::{
    ByteOrder, ByteWindowSink, Cst816Decoder, DisplayCmdBus, DisplaySink, Ft3168Decoder, Gc9a01,
    Gt911Decoder, Ili9341, PanelConfig, Rm67162, St7789, Xpt2046Calibrator,
};

#[derive(Default)]
struct MockCmdBus {
    cmds: Vec<(u8, Vec<u8>)>,
    pixels: Vec<u8>,
    delays: Vec<u32>,
}

impl DisplayCmdBus for MockCmdBus {
    type Error = ();

    fn write_command(&mut self, cmd: u8, data: &[u8]) -> Result<(), Self::Error> {
        self.cmds.push((cmd, data.to_vec()));
        Ok(())
    }

    fn write_pixels(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.pixels.extend_from_slice(bytes);
        Ok(())
    }

    fn delay_ms(&mut self, ms: u32) {
        self.delays.push(ms);
    }
}

#[test]
fn panel_drivers_init_rotate_set_window_and_stream_pixels() {
    let mut st = St7789::new(
        MockCmdBus::default(),
        PanelConfig::new(240, 320)
            .with_offset(0, 20)
            .with_rotation(Rotation::Deg90)
            .with_bgr(true)
            .with_invert(true),
    );
    assert_eq!(st.bounds(), Rect::new(0, 0, 320, 240));
    st.init().unwrap();
    // Includes INVON (0x21) and MADCTL (0x36 = 0x60 | 0x08 = 0x68)
    assert!(st.bus().cmds.iter().any(|(c, _)| *c == 0x21));
    assert!(
        st.bus()
            .cmds
            .iter()
            .any(|(c, d)| *c == 0x36 && d == &[0x68])
    );

    let mut wire_buf = [0u8; 64];
    let bounds = st.bounds();
    let mut sink = ByteWindowSink::new(st, bounds, ByteOrder::BigEndian, &mut wire_buf).unwrap();
    sink.flush_rect(Rect::new(10, 5, 2, 1), &[0xF800, 0x001F])
        .unwrap();
    let bus = sink.into_bus();
    assert_eq!(bus.bus().pixels, vec![0xF8, 0x00, 0x00, 0x1F]);

    // Check ILI9341, GC9A01, RM67162 init & rotation
    let mut ili = Ili9341::new(MockCmdBus::default(), PanelConfig::new(240, 320));
    ili.init().unwrap();
    assert!(
        ili.bus()
            .cmds
            .iter()
            .any(|(c, d)| *c == 0x36 && d == &[0x40])
    );

    let mut gc = Gc9a01::new(
        MockCmdBus::default(),
        PanelConfig::new(240, 240).with_invert(true),
    );
    gc.init().unwrap();
    assert!(gc.bus().cmds.iter().any(|(c, _)| *c == 0xEF));

    let mut rm = Rm67162::new(MockCmdBus::default(), PanelConfig::new(240, 536));
    rm.init().unwrap();
    rm.set_brightness(180).unwrap();
    assert!(rm.bus().cmds.iter().any(|(c, d)| *c == 0x51 && d == &[180]));
}

#[test]
fn touch_decoders_and_xpt2046_calibration_decode_accurately() {
    // GT911 not ready vs 2-point ready
    assert_eq!(Gt911Decoder::decode(0x02, &[0; 16], &mut []), None);
    let mut gt_buf = [0u8; 16];
    gt_buf[0] = 3;
    gt_buf[1..3].copy_from_slice(&120u16.to_le_bytes());
    gt_buf[3..5].copy_from_slice(&240u16.to_le_bytes());
    gt_buf[8] = 5;
    gt_buf[9..11].copy_from_slice(&300u16.to_le_bytes());
    gt_buf[11..13].copy_from_slice(&80u16.to_le_bytes());
    let mut out = [TouchPoint::new(0, Point::new(0, 0)); 4];
    let n = Gt911Decoder::decode(0x82, &gt_buf, &mut out).unwrap();
    assert_eq!(n, 2);
    assert_eq!(out[0], TouchPoint::new(3, Point::new(120, 240)));
    assert_eq!(out[1], TouchPoint::new(5, Point::new(300, 80)));

    // FT3168 2 points (1 Contact + 1 Lift Up filtered)
    let ft_buf = [
        0x02, // TD_STATUS = 2
        0x80 | 0x01,
        0x2C, // Contact, X = 300
        0x20,
        0xC8, // ID = 2, Y = 200
        0,
        0,
        0x40,
        0x10, // Lift up (event = 1) -> ignored
        0x10,
        0x10,
        0,
        0,
    ];
    let fn_cnt = Ft3168Decoder::decode(&ft_buf, &mut out);
    assert_eq!(fn_cnt, 1);
    assert_eq!(out[0], TouchPoint::new(2, Point::new(300, 200)));

    // CST816
    assert_eq!(Cst816Decoder::decode(&[0, 0, 0, 0, 0, 0]), None);
    let pt = Cst816Decoder::decode(&[0x05, 0x01, 0x81, 0x04, 0x00, 0x90]).unwrap();
    assert_eq!(pt, TouchPoint::new(0, Point::new(260, 144)));

    // XPT2046 2-point & 3-point affine calibration (including rotated touchscreen)
    let cal2 = Xpt2046Calibrator::from_two_points(
        (200, 3800),
        (3800, 200),
        Point::new(0, 0),
        Point::new(320, 240),
    )
    .unwrap();
    assert_eq!(cal2.map(2000, 2000), Point::new(160, 120));

    let cal3 = Xpt2046Calibrator::from_three_points(
        [(300, 300), (3500, 350), (320, 3500)],
        [Point::new(20, 20), Point::new(20, 300), Point::new(220, 20)],
    )
    .unwrap();
    assert_eq!(cal3.map(300, 300), Point::new(20, 20));
    assert_eq!(cal3.map(3500, 350), Point::new(20, 300));
    assert_eq!(cal3.map(320, 3500), Point::new(220, 20));
}
