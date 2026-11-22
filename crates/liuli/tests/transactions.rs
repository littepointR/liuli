use core::convert::Infallible;

use embedded_graphics_core::Pixel;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{OriginDimensions, Size};
use embedded_graphics_core::pixelcolor::{IntoStorage, Rgb565};
use liuli::prelude::*;

const SETTINGS_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");

struct VecTarget {
    w: u32,
    h: u32,
    fb: Vec<u16>,
}

impl VecTarget {
    fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            fb: vec![0; (w * h) as usize],
        }
    }
}

impl OriginDimensions for VecTarget {
    fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }
}

impl DrawTarget for VecTarget {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            if p.x >= 0 && p.x < self.w as i32 && p.y >= 0 && p.y < self.h as i32 {
                self.fb[(p.y as usize) * (self.w as usize) + (p.x as usize)] = c.into_storage();
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum FaultMode {
    Normal,
    DuplicateKey,
    UnmatchedEnd,
    DepthOverflow,
    TextOverflow,
    NodeOverflow,
    KindChangedToLabel,
}

struct FaultModel {
    mode: FaultMode,
    counter: u16,
}

#[derive(Debug)]
enum FaultMsg {
    SetMode(FaultMode),
}

struct FaultApp;

impl App for FaultApp {
    type Model = FaultModel;
    type Msg = FaultMsg;

    fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
        match msg {
            FaultMsg::SetMode(m) => {
                model.mode = m;
                model.counter += 1;
                Change::View
            }
        }
    }

    fn view(model: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let mut root_p = NodeProps::new();
        root_p.bg = Some(Rgb565::new(2, 4, 2));
        root_p.width = Length::Px(120);
        root_p.height = Length::Px(80);
        out.begin(Key(1), NodeKind::Root, root_p)?;

        match model.mode {
            FaultMode::Normal => {
                let mut col_p = NodeProps::new();
                col_p.width = Length::Fill(1);
                col_p.height = Length::Fill(1);
                out.begin(Key(2), NodeKind::Column, col_p)?;

                let mut bp = NodeProps::new();
                bp.width = Length::Px(80);
                bp.height = Length::Px(30);
                bp.bg = Some(Rgb565::new(8, 20, 12));
                bp.fg = Rgb565::new(31, 63, 31);
                bp.focusable = true;
                bp.text = out.text_fmt(format_args!("OK {}", model.counter))?;
                out.leaf(Key(10), NodeKind::Button, bp)?;

                out.end()?;
                out.end()?;
                Ok(())
            }
            FaultMode::DuplicateKey => {
                out.leaf(Key(10), NodeKind::Label, NodeProps::new())?;
                out.leaf(Key(10), NodeKind::Label, NodeProps::new())?;
                out.end()
            }
            FaultMode::UnmatchedEnd => {
                out.leaf(Key(10), NodeKind::Label, NodeProps::new())?;
                // Missing out.end() for Root
                Ok(())
            }
            FaultMode::DepthOverflow => {
                for i in 0..MAX_DEPTH {
                    out.begin(Key(100 + i as u32), NodeKind::Column, NodeProps::new())?;
                }
                Ok(())
            }
            FaultMode::TextOverflow => {
                let mut p = NodeProps::new();
                let s = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
                for i in 0..20 {
                    p.text = out.text_fmt(format_args!("{s}{s}{i}"))?;
                }
                out.leaf(Key(10), NodeKind::Label, p)?;
                out.end()
            }
            FaultMode::NodeOverflow => {
                for i in 0..MAX_NODES {
                    out.leaf(Key(200 + i as u32), NodeKind::Label, NodeProps::new())?;
                }
                out.end()
            }
            FaultMode::KindChangedToLabel => {
                let mut lp = NodeProps::new();
                lp.width = Length::Px(80);
                lp.height = Length::Px(30);
                lp.text = TextRef::Static("Label");
                out.leaf(Key(10), NodeKind::Label, lp)?;
                out.end()
            }
        }
    }

    fn on_action(_model: &Self::Model, _event: UiAction) -> Option<Self::Msg> {
        None
    }
}

#[test]
fn failed_transactions_keep_active_frame_and_recover() {
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 120, 80);
    let mut storage = Box::new(Storage::new());
    let mut scratch = [0u16; 120 * 8];
    let mut target = VecTarget::new(120, 80);

    let mut engine = Engine::<FaultApp>::new_in_place(
        &mut storage,
        FaultModel {
            mode: FaultMode::Normal,
            counter: 0,
        },
        font,
        screen,
        InstantMs(0),
    )
    .unwrap();

    // Start pointer capture on button Key(10)
    engine
        .dispatch(
            RawInput::PointerDown {
                id: PointerId(1),
                pos: Point::new(20, 15),
            },
            InstantMs(10),
            false,
        )
        .unwrap();
    engine.present_sync(&mut target, &mut scratch).unwrap();
    let baseline_fb = target.fb.clone();
    let id_before = engine.active_tree().get_id(Key(10)).unwrap();

    for mode in [
        FaultMode::DuplicateKey,
        FaultMode::UnmatchedEnd,
        FaultMode::DepthOverflow,
        FaultMode::TextOverflow,
        FaultMode::NodeOverflow,
    ] {
        engine
            .message(FaultMsg::SetMode(mode), InstantMs(20), false)
            .unwrap();
        assert!(engine.view_pending());
        // Must NOT spin NextWakeup::Now when damage is empty & view failed!
        assert_eq!(
            engine.next_wakeup(InstantMs(20), false),
            NextWakeup::OnEvent
        );
        assert_eq!(engine.active_tree().get_id(Key(10)), Some(id_before));
        assert!(engine.pointer().captured_slot().is_some());
        assert!(engine.damage().is_empty());

        // Re-rendering active snapshot matches pixel-for-pixel
        let mut check_target = VecTarget::new(120, 80);
        let mut check_dmg = DamageSet::new();
        check_dmg.mark_full_screen(screen);
        liuli::render::present_sync(
            &mut check_target,
            &mut scratch,
            screen,
            8,
            engine.active_commands(),
            &mut check_dmg,
            engine.active_tree(),
            engine.font(),
        )
        .unwrap();
        assert_eq!(check_target.fb, baseline_fb);
    }

    // Recover with valid build
    engine
        .message(FaultMsg::SetMode(FaultMode::Normal), InstantMs(30), false)
        .unwrap();
    assert!(!engine.view_pending());
    assert_eq!(engine.active_tree().get_id(Key(10)), Some(id_before));

    // Kind change clears pointer capture and issues new generation
    engine
        .message(
            FaultMsg::SetMode(FaultMode::KindChangedToLabel),
            InstantMs(40),
            false,
        )
        .unwrap();
    assert!(engine.pointer().captured_slot().is_none());
    let id_after = engine.active_tree().get_id(Key(10)).unwrap();
    assert_ne!(id_before, id_after);
    assert!(!engine.active_tree().is_id_valid(id_before));
}
