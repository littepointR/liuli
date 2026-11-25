use liuli::prelude::*;

use crate::terminal::{TerminalModel, terminal_keys as tk};
use crate::theme::Theme;

pub fn build_settings(
    model: &TerminalModel,
    ui: &mut Children<'_, '_>,
    t: &Theme,
) -> Result<(), BuildError> {
    let mut scroll_p = NodeProps::new();
    scroll_p.width = Length::Fill(1);
    scroll_p.height = Length::Fill(1);
    scroll_p.padding = EdgeInsets {
        top: 6,
        right: 8,
        bottom: 6,
        left: 6,
    };
    scroll_p.gap = 4;
    scroll_p.accent = t.accent;

    ui.scroll_y(tk::SET_SCROLL, scroll_p, |ui| {
        // Row 0: 屏幕亮度 (repeatable - / + + Slider)
        setting_row(ui, tk::SET_ROW_BRIGHTNESS, t, false, |ui| {
            add_row_label(ui, tk::SET_LBL_BRIGHTNESS, "屏幕亮度", t)?;
            let bright_txt = ui.text_fmt(format_args!("{:>3}%", model.settings.brightness))?;
            add_value_label(ui, tk::SET_VAL_BRIGHTNESS, bright_txt, 38, t)?;

            let mut step_btn = t.control();
            step_btn.width = Length::Px(24);
            step_btn.height = Length::Px(24);
            step_btn.padding = EdgeInsets::symmetric(4, 2);
            step_btn.bg = Some(t.surface_alt);
            step_btn.border_color = Some(t.border);
            step_btn.border_width = 1;
            step_btn.radius = 5;
            step_btn.fg = t.text;
            step_btn.accent = t.surface_alt;
            step_btn.text_align = TextAlign::Center;
            step_btn.max_lines = 1;
            step_btn.focusable = true;
            step_btn.repeatable = true;

            step_btn.text = TextRef::Static("-");
            ui.button(tk::SET_BTN_BRIGHT_DEC, step_btn)?;

            let mut sl_p = t.control();
            sl_p.width = Length::Px(74);
            sl_p.height = Length::Px(24);
            sl_p.bg = Some(t.track_off);
            sl_p.accent = t.accent;
            sl_p.fg = t.knob;
            sl_p.value = model.settings.brightness;
            sl_p.min = 0;
            sl_p.max = 100;
            sl_p.step = 5;
            sl_p.focusable = true;
            ui.slider(tk::SET_SLIDER_BRIGHTNESS, sl_p)?;

            step_btn.text = TextRef::Static("+");
            ui.button(tk::SET_BTN_BRIGHT_INC, step_btn)
        })?;

        // Row 1: 提示音
        setting_row(ui, tk::SET_ROW_SOUND, t, true, |ui| {
            add_row_label(ui, tk::SET_LBL_SOUND, "提示音", t)?;
            let s_str = if model.settings.sound_enabled {
                "开启"
            } else {
                "关闭"
            };
            add_value_label(ui, tk::SET_STATE_SOUND, TextRef::Static(s_str), 44, t)?;
            add_cubic_toggle(ui, tk::SET_TOGGLE_SOUND, model.settings.sound_enabled, t)
        })?;

        // Row 2: 深色主题
        setting_row(ui, tk::SET_ROW_THEME, t, true, |ui| {
            add_row_label(ui, tk::SET_LBL_THEME, "深色主题", t)?;
            let th_str = if model.settings.dark_theme {
                "开启"
            } else {
                "关闭"
            };
            add_value_label(ui, tk::SET_STATE_THEME, TextRef::Static(th_str), 44, t)?;
            add_cubic_toggle(ui, tk::SET_TOGGLE_THEME, model.settings.dark_theme, t)
        })?;

        // Row 3: 实时监测
        setting_row(ui, tk::SET_ROW_MONITOR, t, true, |ui| {
            add_row_label(ui, tk::SET_LBL_MONITOR, "实时监测", t)?;
            let mon_str = if model.settings.live_monitoring {
                "开启"
            } else {
                "关闭"
            };
            add_value_label(ui, tk::SET_STATE_MONITOR, TextRef::Static(mon_str), 44, t)?;
            add_cubic_toggle(
                ui,
                tk::SET_TOGGLE_MONITOR,
                model.settings.live_monitoring,
                t,
            )
        })?;

        // Row 4: 设备温度
        setting_row(ui, tk::SET_ROW_TEMP, t, false, |ui| {
            add_row_label(ui, tk::SET_LBL_TEMP, "设备温度", t)?;
            let whole = model.settings.temp_deci_c / 10;
            let frac = (model.settings.temp_deci_c % 10).abs();
            let temp_txt = ui.text_fmt(format_args!("{whole}.{frac} °C"))?;
            add_value_label(ui, tk::SET_VAL_TEMP, temp_txt, 80, t)
        })?;

        // Row 5: 恢复默认
        setting_row(ui, tk::SET_ROW_RESET, t, true, |ui| {
            add_row_label(ui, tk::SET_LBL_RESET, "恢复默认", t)?;
            let mut reset_btn = t.control();
            reset_btn.width = Length::Px(84);
            reset_btn.height = Length::Px(26);
            reset_btn.padding = EdgeInsets::symmetric(8, 3);
            reset_btn.bg = Some(t.surface_alt);
            reset_btn.border_color = Some(t.border);
            reset_btn.border_width = 1;
            reset_btn.radius = 6;
            reset_btn.fg = t.danger_text;
            reset_btn.accent = t.surface_alt;
            reset_btn.icon = IconKind::Reset;
            reset_btn.text = TextRef::Static("重设");
            reset_btn.max_lines = 1;
            reset_btn.focusable = true;
            reset_btn.repeatable = false;
            ui.button(tk::SET_BTN_RESET, reset_btn)
        })
    })
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
    row_p.height = Length::Px(38);
    row_p.padding = EdgeInsets::symmetric(8, 5);
    row_p.gap = 5;
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
    let mut lp = NodeProps::new();
    lp.width = Length::Fill(1);
    lp.height = Length::Content;
    lp.fg = t.text;
    lp.text = TextRef::Static(text);
    lp.max_lines = 1;
    ui.label(key, lp)
}

fn add_value_label(
    ui: &mut Children<'_, '_>,
    key: Key,
    text: TextRef,
    width: u32,
    t: &Theme,
) -> Result<(), BuildError> {
    let mut vp = NodeProps::new();
    vp.width = Length::Px(width);
    vp.height = Length::Content;
    vp.fg = t.text_muted;
    vp.font = FontId::Small12;
    vp.text_align = TextAlign::End;
    vp.text = text;
    vp.max_lines = 1;
    ui.label(key, vp)
}

fn add_cubic_toggle(
    ui: &mut Children<'_, '_>,
    key: Key,
    checked: bool,
    t: &Theme,
) -> Result<(), BuildError> {
    let mut tp = t.control();
    tp.width = Length::Px(46);
    tp.height = Length::Px(24);
    tp.bg = Some(t.track_off);
    tp.accent = t.accent;
    tp.fg = t.knob;
    tp.checked = checked;
    tp.focusable = true;
    tp.anim_easing = Easing::EaseOutCubic;
    ui.toggle(key, tp)
}
