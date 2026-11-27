#![no_std]
#![forbid(unsafe_code)]
use core::num::NonZeroU16;
use liuli::prelude::*;
use liuli_widgets::{Navigator, Theme, paint_meter, virtual_list_with_policy};
const LIST: Key = Key(10);
const METER: Key = Key(20);
const FIRST: u32 = 100;
const COUNT: u32 = 1000;
const ROW: u16 = 32;
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Page {
    List,
    Detail(u32),
    Edit(u32),
}
pub struct Model {
    pub routes: Navigator<Page, 2>,
    pub threshold: i16,
    pub draft: i16,
    pub offset: i32,
    pub focus: Option<Key>,
    thresholds: [u8; COUNT as usize],
    viewport: u16,
    pub confirm: bool,
    pub light: bool,
}
impl Model {
    pub fn new(height: u16) -> Self {
        Self {
            routes: Navigator::new(Page::List),
            thresholds: [50; COUNT as usize],
            threshold: 50,
            draft: 50,
            offset: 0,
            focus: None,
            viewport: height.saturating_sub(32),
            confirm: false,
            light: false,
        }
    }
}
#[derive(Copy, Clone, Debug)]
pub enum Msg {
    Open(u32),
    Edit,
    Adjust(i16),
    Ask,
    Save,
    Back,
    Scroll(i32),
    Theme,
}
pub struct Alarms;
impl App for Alarms {
    type Model = Model;
    type Msg = Msg;
    fn update(m: &mut Model, msg: Msg, _: InstantMs) -> Change {
        match msg {
            Msg::Open(id) if m.routes.current() == Page::List && id < COUNT => {
                m.threshold = m.thresholds[id as usize].into();
                let _ = m.routes.push(Page::Detail(id));
            }
            Msg::Edit => {
                if let Page::Detail(id) = m.routes.current() {
                    m.draft = m.threshold;
                    let _ = m.routes.push(Page::Edit(id));
                }
            }
            Msg::Adjust(v) => m.draft = v.clamp(0, 100),
            Msg::Ask => m.confirm = true,
            Msg::Save if m.confirm => {
                if let Ok(result) = m.routes.pop_with(m.draft)
                    && let Page::Edit(id) = result.from
                {
                    m.thresholds[id as usize] = result.value as u8;
                    m.threshold = result.value;
                }
                m.confirm = false;
            }
            Msg::Back => {
                if m.confirm {
                    m.confirm = false
                } else {
                    m.routes.pop();
                }
            }
            Msg::Scroll(y) => m.offset = y,
            Msg::Theme => m.light = !m.light,
            _ => return Change::Unchanged,
        }
        Change::View
    }
    fn view(m: &Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let t = if m.light { Theme::LIGHT } else { Theme::DARK };
        let button = |s| NodeProps {
            text: TextRef::Static(s),
            ..t.control()
        };
        let fill = NodeProps {
            width: Length::Fill(1),
            height: Length::Fill(1),
            bg: Some(t.background),
            ..NodeProps::new()
        };
        out.root(Key(1), fill, |root| {
            root.column(
                Key(2),
                NodeProps {
                    align_cross: Align::Stretch,
                    ..fill
                },
                |ui| {
                    match m.routes.current() {
                        Page::List => {
                            ui.button(Key(3), button("告警 / 主题"))?;
                            virtual_list_with_policy(
                                ui,
                                [LIST, Key(11), Key(12)],
                                COUNT,
                                NonZeroU16::new(ROW).unwrap(),
                                m.offset,
                                m.viewport,
                                t.scroll_policy(ScrollPolicy::FREE),
                                |list, id| {
                                    let text = list.text_fmt(format_args!("告警 {:04}", id + 1))?;
                                    list.button(
                                        Key(FIRST + id),
                                        NodeProps {
                                            text,
                                            long_press: true,
                                            swipeable: true,
                                            ..t.control()
                                        },
                                    )
                                },
                            )?;
                        }
                        Page::Detail(id) | Page::Edit(id) => {
                            let text = ui.text_fmt(format_args!("告警 {:04}", id + 1))?;
                            ui.label(
                                Key(4),
                                NodeProps {
                                    text,
                                    fg: t.text,
                                    height: Length::Px(32),
                                    ..NodeProps::new()
                                },
                            )?;
                            let edit = matches!(m.routes.current(), Page::Edit(_));
                            let value = if edit { m.draft } else { m.threshold };
                            ui.custom(
                                METER,
                                NodeProps {
                                    value,
                                    height: Length::Px(12),
                                    width: Length::Fill(1),
                                    bg: Some(t.surface),
                                    accent: t.accent,
                                    ..NodeProps::new()
                                },
                            )?;
                            if edit {
                                liuli_widgets::number_editor(ui, Key(5), 0..=100, value, 1, t)?;
                                ui.button(Key(6), button("保存"))?;
                            } else {
                                ui.button(Key(7), button("编辑阈值"))?;
                            }
                            ui.button(Key(8), button("Back"))?;
                        }
                    }
                    Ok(())
                },
            )?;
            if m.confirm {
                liuli_widgets::confirm_dialog(
                    root,
                    [Key(30), Key(31), Key(32), Key(33)],
                    [TextRef::Static("确认"), TextRef::Static("取消")],
                    t,
                )?;
            }
            Ok(())
        })
    }
    fn on_action(_: &Model, a: UiAction) -> Option<Msg> {
        Some(match (a.key.0, a.kind) {
            (_, ActionKind::Dismiss) | (8 | 33, ActionKind::Activate) => Msg::Back,
            (5, ActionKind::SetValue(v)) => Msg::Adjust(v),
            (3, ActionKind::Activate) => Msg::Theme,
            (6, ActionKind::Activate) => Msg::Ask,
            (7, ActionKind::Activate) => Msg::Edit,
            (32, ActionKind::Activate) => Msg::Save,
            (
                k,
                ActionKind::Activate
                | ActionKind::LongPress
                | ActionKind::SwipeLeft
                | ActionKind::SwipeRight,
            ) if (FIRST..FIRST + COUNT).contains(&k) => Msg::Open(k - FIRST),
            _ => return None,
        })
    }
    fn paint_custom(
        key: Key,
        p: &StoredProps,
        canvas: &mut WidgetCanvas<'_>,
    ) -> Result<(), BuildError> {
        if key == METER {
            paint_meter(p, canvas)?;
        }
        Ok(())
    }
    fn on_scroll(_: &Model, key: Key, y: i32) -> Option<Msg> {
        (key == LIST).then_some(Msg::Scroll(y))
    }
    fn page_id(m: &Model) -> u8 {
        match m.routes.current() {
            Page::List => 0,
            Page::Detail(_) => 1,
            Page::Edit(_) => 2,
        }
    }
    fn page_restore(m: &Model, page: u8) -> PageRestore {
        if page == 0 {
            PageRestore {
                scroll_key: Some(LIST),
                scroll_y: m.offset,
                focus_key: m.focus,
            }
        } else {
            PageRestore::default()
        }
    }
    fn on_page_committed(
        m: &mut Model,
        old: u8,
        focus: Option<Key>,
        key: Option<Key>,
        offset: i32,
    ) {
        if old == 0 && key == Some(LIST) {
            m.offset = offset;
            m.focus = focus;
        }
    }
}
