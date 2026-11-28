use liuli::prelude::*;

use crate::model::{SettingsModel, SettingsMsg};
use crate::theme::{DARK_THEME, LIGHT_THEME, Theme};

pub mod keys {
    use liuli::prelude::Key;

    pub const ROOT: Key = Key(1);
    pub const STACK: Key = Key(2);
    pub const MAIN_COL: Key = Key(3);

    pub const HEADER: Key = Key(10);
    pub const HEADER_TITLE: Key = Key(11);

    pub const SCROLL: Key = Key(20);

    pub const ROW_BRIGHTNESS: Key = Key(30);
    pub const LBL_BRIGHTNESS: Key = Key(31);
    pub const VAL_BRIGHTNESS: Key = Key(32);
    pub const SLIDER_BRIGHTNESS: Key = Key(33);

    pub const ROW_SOUND: Key = Key(40);
    pub const LBL_SOUND: Key = Key(41);
    pub const STATE_SOUND: Key = Key(42);
    pub const TOGGLE_SOUND: Key = Key(43);

    pub const ROW_THEME: Key = Key(50);
    pub const LBL_THEME: Key = Key(51);
    pub const STATE_THEME: Key = Key(52);
    pub const TOGGLE_THEME: Key = Key(53);

    pub const ROW_MONITOR: Key = Key(60);
    pub const LBL_MONITOR: Key = Key(61);
    pub const STATE_MONITOR: Key = Key(62);
    pub const TOGGLE_MONITOR: Key = Key(63);

    pub const ROW_TEMP: Key = Key(70);
    pub const LBL_TEMP: Key = Key(71);
    pub const VAL_TEMP: Key = Key(72);

    pub const ROW_RESET: Key = Key(80);
    pub const LBL_RESET: Key = Key(81);
    pub const BTN_RESET: Key = Key(82);

    pub const FOOTER: Key = Key(90);
    pub const FOOTER_TEXT: Key = Key(91);

    pub const MODAL: Key = Key(100);
    pub const MODAL_CARD: Key = Key(101);
    pub const MODAL_TITLE: Key = Key(102);
    pub const MODAL_BODY: Key = Key(103);
    pub const MODAL_BTNS: Key = Key(104);
    pub const BTN_CANCEL: Key = Key(105);
    pub const BTN_CONFIRM: Key = Key(106);
}

pub struct SettingsApp;

impl SettingsApp {
    pub fn theme(model: &SettingsModel) -> &'static Theme {
        if model.dark_theme {
            &DARK_THEME
        } else {
            &LIGHT_THEME
        }
    }
}

impl App for SettingsApp {
    type Model = SettingsModel;
    type Msg = SettingsMsg;

    fn update(model: &mut Self::Model, msg: Self::Msg, now: InstantMs) -> Change {
        match msg {
            SettingsMsg::SetBrightness(v) => {
                let clamped = ((v + 2) / 5 * 5).clamp(0, 100);
                if model.brightness != clamped {
                    model.brightness = clamped;
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
            SettingsMsg::ToggleSound => {
                model.sound_enabled = !model.sound_enabled;
                Change::View
            }
            SettingsMsg::ToggleTheme => {
                model.dark_theme = !model.dark_theme;
                Change::View
            }
            SettingsMsg::ToggleMonitoring => {
                model.live_monitoring = !model.live_monitoring;
                if model.live_monitoring {
                    model.sample_origin_ms = now;
                    model.next_sample_ms = Some(now.saturating_add(1000));
                } else {
                    model.next_sample_ms = None;
                }
                Change::View
            }
            SettingsMsg::OpenResetModal => {
                if !model.reset_modal_open {
                    model.reset_modal_open = true;
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
            SettingsMsg::CancelReset => {
                if model.reset_modal_open {
                    model.reset_modal_open = false;
                    Change::View
                } else {
                    Change::Unchanged
                }
            }
            SettingsMsg::ConfirmReset => {
                model.restore_defaults(now);
                Change::View
            }
        }
    }

    fn on_action(_model: &Self::Model, event: UiAction) -> Option<Self::Msg> {
        match (event.key, event.kind) {
            (keys::SLIDER_BRIGHTNESS, ActionKind::SetValue(v)) => {
                Some(SettingsMsg::SetBrightness(v))
            }
            (keys::TOGGLE_SOUND | keys::ROW_SOUND, ActionKind::Toggle | ActionKind::Activate) => {
                Some(SettingsMsg::ToggleSound)
            }
            (keys::TOGGLE_THEME | keys::ROW_THEME, ActionKind::Toggle | ActionKind::Activate) => {
                Some(SettingsMsg::ToggleTheme)
            }
            (
                keys::TOGGLE_MONITOR | keys::ROW_MONITOR,
                ActionKind::Toggle | ActionKind::Activate,
            ) => Some(SettingsMsg::ToggleMonitoring),
            (keys::BTN_RESET | keys::ROW_RESET, ActionKind::Activate) => {
                Some(SettingsMsg::OpenResetModal)
            }
            (keys::BTN_CANCEL, ActionKind::Activate) | (keys::MODAL, ActionKind::Dismiss) => {
                Some(SettingsMsg::CancelReset)
            }
            (keys::BTN_CONFIRM, ActionKind::Activate) => Some(SettingsMsg::ConfirmReset),
            _ => None,
        }
    }

    fn next_deadline(model: &Self::Model, _now: InstantMs) -> Option<InstantMs> {
        if model.live_monitoring {
            model.next_sample_ms
        } else {
            None
        }
    }

    fn on_timer(model: &mut Self::Model, now: InstantMs) -> Change {
        if model.sample_temperature(now) {
            Change::View
        } else {
            Change::Unchanged
        }
    }

    fn view(model: &Self::Model) -> impl Scene {
        let t = Self::theme(model);

        let mut root_p = NodeProps::new();
        root_p.width = Length::Fill(1);
        root_p.height = Length::Fill(1);
        root_p.bg = Some(t.bg);
        root_p.theme_id = t.id;

        layout(keys::ROOT, root_p, move |ui| {
            let mut stack_p = NodeProps::new();
            stack_p.width = Length::Fill(1);
            stack_p.height = Length::Fill(1);

            ui.stack(keys::STACK, stack_p, |ui| {
                let mut col_p = NodeProps::new();
                col_p.width = Length::Fill(1);
                col_p.height = Length::Fill(1);
                col_p.align_cross = Align::Stretch;

                ui.column(keys::MAIN_COL, col_p, |ui| {
                    // 1. Header (32 px)
                    let mut header_p = NodeProps::new();
                    header_p.width = Length::Fill(1);
                    header_p.height = Length::Px(32);
                    header_p.padding = EdgeInsets::symmetric(12, 6);
                    header_p.bg = Some(t.header_bg);
                    header_p.border_color = Some(t.border);
                    header_p.border_width = 1;
                    header_p.align_cross = Align::Center;
                    ui.row(keys::HEADER, header_p, |ui| {
                        let mut h_lbl = NodeProps::new();
                        h_lbl.width = Length::Fill(1);
                        h_lbl.height = Length::Content;
                        h_lbl.fg = t.text;
                        h_lbl.text = TextRef::Static("设备设置");
                        h_lbl.max_lines = 1;
                        ui.label(keys::HEADER_TITLE, h_lbl)
                    })?;

                    // 2. Scrollable list viewport (184 px)
                    let mut scroll_p = NodeProps::new();
                    scroll_p.width = Length::Fill(1);
                    scroll_p.height = Length::Px(184);
                    scroll_p.padding = EdgeInsets {
                        top: 10,
                        right: 8,
                        bottom: 10,
                        left: 6,
                    };
                    scroll_p.gap = 4;
                    scroll_p.accent = t.accent;
                    ui.scroll_y(keys::SCROLL, scroll_p, |ui| {
                        // Row 0: 屏幕亮度
                        setting_row(ui, keys::ROW_BRIGHTNESS, t, false, |ui| {
                            add_row_label(ui, keys::LBL_BRIGHTNESS, "屏幕亮度", t)?;
                            let bright_txt =
                                ui.text_fmt(format_args!("{:>3}%", model.brightness))?;
                            add_value_label(ui, keys::VAL_BRIGHTNESS, bright_txt, 48, t)?;
                            let mut sl_p = t.control();
                            sl_p.width = Length::Px(96);
                            sl_p.height = Length::Px(24);
                            sl_p.bg = Some(t.track_off);
                            sl_p.accent = t.accent;
                            sl_p.fg = t.knob;
                            sl_p.value = model.brightness;
                            sl_p.min = 0;
                            sl_p.max = 100;
                            sl_p.step = 5;
                            sl_p.focusable = true;
                            ui.slider(keys::SLIDER_BRIGHTNESS, sl_p)
                        })?;

                        // Row 1: 提示音
                        setting_row(ui, keys::ROW_SOUND, t, true, |ui| {
                            add_row_label(ui, keys::LBL_SOUND, "提示音", t)?;
                            let sound_str = if model.sound_enabled {
                                "开启"
                            } else {
                                "关闭"
                            };
                            add_value_label(
                                ui,
                                keys::STATE_SOUND,
                                TextRef::Static(sound_str),
                                44,
                                t,
                            )?;
                            add_toggle(ui, keys::TOGGLE_SOUND, model.sound_enabled, t)
                        })?;

                        // Row 2: 深色主题
                        setting_row(ui, keys::ROW_THEME, t, true, |ui| {
                            add_row_label(ui, keys::LBL_THEME, "深色主题", t)?;
                            let theme_str = if model.dark_theme { "开启" } else { "关闭" };
                            add_value_label(
                                ui,
                                keys::STATE_THEME,
                                TextRef::Static(theme_str),
                                44,
                                t,
                            )?;
                            add_toggle(ui, keys::TOGGLE_THEME, model.dark_theme, t)
                        })?;

                        // Row 3: 实时监测
                        setting_row(ui, keys::ROW_MONITOR, t, true, |ui| {
                            add_row_label(ui, keys::LBL_MONITOR, "实时监测", t)?;
                            let mon_str = if model.live_monitoring {
                                "开启"
                            } else {
                                "关闭"
                            };
                            add_value_label(
                                ui,
                                keys::STATE_MONITOR,
                                TextRef::Static(mon_str),
                                44,
                                t,
                            )?;
                            add_toggle(ui, keys::TOGGLE_MONITOR, model.live_monitoring, t)
                        })?;

                        // Row 4: 设备温度 (只读，不可聚焦，固定宽度文本字段)
                        setting_row(ui, keys::ROW_TEMP, t, false, |ui| {
                            add_row_label(ui, keys::LBL_TEMP, "设备温度", t)?;
                            let whole = model.temp_deci_c / 10;
                            let frac = (model.temp_deci_c % 10).abs();
                            let temp_txt = ui.text_fmt(format_args!("{whole}.{frac} °C"))?;
                            add_value_label(ui, keys::VAL_TEMP, temp_txt, 80, t)
                        })?;

                        // Row 5: 恢复默认
                        setting_row(ui, keys::ROW_RESET, t, true, |ui| {
                            add_row_label(ui, keys::LBL_RESET, "恢复默认", t)?;
                            let mut reset_btn = t.control();
                            reset_btn.width = Length::Px(76);
                            reset_btn.height = Length::Px(26);
                            reset_btn.padding = EdgeInsets::symmetric(8, 3);
                            reset_btn.bg = Some(t.surface_alt);
                            reset_btn.border_color = Some(t.border);
                            reset_btn.border_width = 1;
                            reset_btn.radius = 6;
                            reset_btn.fg = t.danger_text;
                            reset_btn.accent = t.surface_alt;
                            reset_btn.text = TextRef::Static("重设");
                            reset_btn.max_lines = 1;
                            reset_btn.focusable = true;
                            ui.button(keys::BTN_RESET, reset_btn)
                        })
                    })?;

                    // 3. Footer (24 px)
                    let mut footer_p = NodeProps::new();
                    footer_p.width = Length::Fill(1);
                    footer_p.height = Length::Px(24);
                    footer_p.padding = EdgeInsets::symmetric(10, 3);
                    footer_p.bg = Some(t.header_bg);
                    footer_p.border_color = Some(t.border);
                    footer_p.border_width = 1;
                    footer_p.align_cross = Align::Center;
                    ui.row(keys::FOOTER, footer_p, |ui| {
                        let mut f_lbl = NodeProps::new();
                        f_lbl.width = Length::Fill(1);
                        f_lbl.height = Length::Content;
                        f_lbl.fg = t.text_muted;
                        f_lbl.text = TextRef::Static("旋钮切换 · 确认编辑 · 返回");
                        f_lbl.max_lines = 1;
                        ui.label(keys::FOOTER_TEXT, f_lbl)
                    })
                })?;

                // 4. Optional Modal
                if ui.optional(model.reset_modal_open) {
                    let mut modal_p = NodeProps::new();
                    modal_p.width = Length::Fill(1);
                    modal_p.height = Length::Fill(1);
                    modal_p.overlay_color = t.overlay;
                    modal_p.overlay_alpha = 9;
                    ui.modal(keys::MODAL, modal_p, |ui| {
                        let mut card_p = NodeProps::new();
                        card_p.width = Length::Px(252);
                        card_p.height = Length::Content;
                        card_p.padding = EdgeInsets::all(14);
                        card_p.gap = 8;
                        card_p.align_cross = Align::Stretch;
                        card_p.bg = Some(t.surface);
                        card_p.border_color = Some(t.border);
                        card_p.border_width = 1;
                        card_p.radius = 10;
                        ui.column(keys::MODAL_CARD, card_p, |ui| {
                            let mut mt_p = NodeProps::new();
                            mt_p.width = Length::Fill(1);
                            mt_p.height = Length::Content;
                            mt_p.fg = t.text;
                            mt_p.text = TextRef::Static("恢复默认设置？");
                            mt_p.max_lines = 1;
                            ui.label(keys::MODAL_TITLE, mt_p)?;

                            let mut mb_p = NodeProps::new();
                            mb_p.width = Length::Fill(1);
                            mb_p.height = Length::Content;
                            mb_p.fg = t.text_muted;
                            mb_p.text = TextRef::Static("将恢复出厂默认值。");
                            mb_p.max_lines = 2;
                            ui.label(keys::MODAL_BODY, mb_p)?;

                            let mut btn_row = NodeProps::new();
                            btn_row.width = Length::Fill(1);
                            btn_row.height = Length::Px(40);
                            btn_row.padding = EdgeInsets::all(4);
                            btn_row.gap = 10;
                            btn_row.align_cross = Align::Stretch;
                            ui.row(keys::MODAL_BTNS, btn_row, |ui| {
                                let mut cancel_p = t.control();
                                cancel_p.width = Length::Fill(1);
                                cancel_p.height = Length::Px(32);
                                cancel_p.padding = EdgeInsets::symmetric(8, 4);
                                cancel_p.bg = Some(t.surface_alt);
                                cancel_p.border_color = Some(t.border);
                                cancel_p.border_width = 1;
                                cancel_p.radius = 6;
                                cancel_p.fg = t.text;
                                cancel_p.accent = t.surface_alt;
                                cancel_p.text = TextRef::Static("取消");
                                cancel_p.max_lines = 1;
                                cancel_p.focusable = true;
                                ui.button(keys::BTN_CANCEL, cancel_p)?;

                                let mut confirm_p = t.control();
                                confirm_p.width = Length::Fill(1);
                                confirm_p.height = Length::Px(32);
                                confirm_p.padding = EdgeInsets::symmetric(8, 4);
                                confirm_p.bg = Some(t.danger);
                                confirm_p.radius = 6;
                                confirm_p.fg = t.on_accent;
                                confirm_p.accent = t.danger;
                                confirm_p.text = TextRef::Static("恢复");
                                confirm_p.max_lines = 1;
                                confirm_p.focusable = true;
                                ui.button(keys::BTN_CONFIRM, confirm_p)
                            })
                        })
                    })?;
                }

                Ok(())
            })
        })
    }
}

fn setting_row<F>(
    ui: &mut Children<'_, '_>,
    key: Key,
    t: &Theme,
    clickable: bool,
    f: F,
) -> Result<(), BuildError>
where
    F: FnOnce(&mut Children<'_, '_>) -> Result<(), BuildError>,
{
    let mut row_p = NodeProps::new();
    row_p.width = Length::Fill(1);
    row_p.height = Length::Px(40);
    row_p.padding = EdgeInsets::symmetric(10, 6);
    row_p.gap = 8;
    row_p.align_cross = Align::Center;
    row_p.bg = Some(t.surface);
    row_p.border_color = Some(t.border);
    row_p.border_width = 1;
    row_p.radius = 6;
    row_p.clickable = clickable;
    ui.row(key, row_p, f)
}

fn add_row_label(
    ui: &mut Children<'_, '_>,
    key: Key,
    text: &'static str,
    t: &Theme,
) -> Result<(), BuildError> {
    let mut p = NodeProps::new();
    p.width = Length::Fill(1);
    p.height = Length::Content;
    p.fg = t.text;
    p.text = TextRef::Static(text);
    p.max_lines = 1;
    ui.label(key, p)
}

fn add_value_label(
    ui: &mut Children<'_, '_>,
    key: Key,
    text: TextRef,
    width: u32,
    t: &Theme,
) -> Result<(), BuildError> {
    let mut p = NodeProps::new();
    p.width = Length::Px(width);
    p.height = Length::Content;
    p.fg = t.text_muted;
    p.text = text;
    p.max_lines = 1;
    ui.label(key, p)
}

fn add_toggle(
    ui: &mut Children<'_, '_>,
    key: Key,
    checked: bool,
    t: &Theme,
) -> Result<(), BuildError> {
    let mut p = t.control();
    p.width = Length::Px(40);
    p.height = Length::Px(24);
    p.bg = Some(t.track_off);
    p.accent = t.accent;
    p.fg = t.knob;
    p.checked = checked;
    p.focusable = true;
    ui.toggle(key, p)
}
