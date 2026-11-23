use liuli::prelude::*;

use crate::terminal::{TerminalModel, terminal_keys as tk};
use crate::theme::Theme;

pub fn build_diagnostics(
    model: &TerminalModel,
    ui: &mut Children<'_, '_>,
    t: &Theme,
) -> Result<(), BuildError> {
    let mut scroll_p = NodeProps::new();
    scroll_p.width = Length::Fill(1);
    scroll_p.height = Length::Fill(1);
    scroll_p.padding = EdgeInsets::symmetric(8, 3);
    scroll_p.gap = 3;
    scroll_p.accent = t.accent;

    let d = model.displayed_diag;

    ui.scroll_y(tk::DIAG_SCROLL, scroll_p, |ui| {
        let state_txt = ui.text_fmt(format_args!(
            "{} · {}/32点",
            if model.settings.live_monitoring {
                "正常"
            } else {
                "暂停"
            },
            model.temp_history.len()
        ))?;
        diag_row(
            ui,
            tk::DIAG_ROW_SAMPLE,
            tk::DIAG_LBL_SAMPLE,
            tk::DIAG_VAL_SAMPLE,
            "采样状态",
            state_txt,
            t,
        )?;

        let xfer_txt = ui.text_fmt(format_args!(
            "{} 条带 / {} 像素",
            d.last_bands, d.last_pixels
        ))?;
        diag_row(
            ui,
            tk::DIAG_ROW_XFER,
            tk::DIAG_LBL_XFER,
            tk::DIAG_VAL_XFER,
            "帧传输",
            xfer_txt,
            t,
        )?;

        let frames_txt = ui.text_fmt(format_args!(
            "{} 帧 · CMD {}",
            d.total_frames, d.last_commands
        ))?;
        diag_row(
            ui,
            tk::DIAG_ROW_FRAMES,
            tk::DIAG_LBL_FRAMES,
            tk::DIAG_VAL_FRAMES,
            "帧总计",
            frames_txt,
            t,
        )?;

        diag_row(
            ui,
            tk::DIAG_ROW_STORAGE,
            tk::DIAG_LBL_STORAGE,
            tk::DIAG_VAL_STORAGE,
            "存储状态",
            TextRef::Static("内存正常 · <80KB"),
            t,
        )?;

        let whole = model.settings.temp_deci_c / 10;
        let frac = (model.settings.temp_deci_c % 10).abs();
        let tv_txt = ui.text_fmt(format_args!("{whole}.{frac} °C · {}%", model.load_pct))?;
        diag_row(
            ui,
            tk::DIAG_ROW_TEMP,
            tk::DIAG_LBL_TEMP,
            tk::DIAG_VAL_TEMP,
            "温度负载",
            tv_txt,
            t,
        )?;

        let time_txt = ui.text_fmt(format_args!(
            "{} ms · MEAS {}",
            d.captured_at_ms, d.last_measured
        ))?;
        diag_row(
            ui,
            tk::DIAG_ROW_TIME,
            tk::DIAG_LBL_TIME,
            tk::DIAG_VAL_TIME,
            "当前时间",
            time_txt,
            t,
        )?;

        // Action row (3px vertical padding ensures unclipped 2px focus rings)
        let mut act = NodeProps::new();
        act.width = Length::Fill(1);
        act.height = Length::Px(34);
        act.padding = EdgeInsets::symmetric(4, 3);
        act.gap = 8;
        act.align_cross = Align::Center;
        ui.row(tk::DIAG_ACT_ROW, act, |ui| {
            let mut ref_p = NodeProps::new();
            ref_p.width = Length::Fill(1);
            ref_p.height = Length::Px(28);
            ref_p.padding = EdgeInsets::symmetric(10, 3);
            ref_p.bg = Some(t.accent);
            ref_p.radius = 6;
            ref_p.fg = t.on_accent;
            ref_p.accent = t.accent;
            ref_p.focus_color = t.focus;
            ref_p.icon = IconKind::Diagnostics;
            ref_p.text = TextRef::Static("刷新");
            ref_p.max_lines = 1;
            ref_p.focusable = true;
            ui.button(tk::DIAG_BTN_REFRESH, ref_p)?;

            let mut mon_p = NodeProps::new();
            mon_p.width = Length::Fill(1);
            mon_p.height = Length::Px(28);
            mon_p.padding = EdgeInsets::symmetric(10, 3);
            mon_p.bg = Some(t.surface);
            mon_p.border_color = Some(t.border);
            mon_p.border_width = 1;
            mon_p.radius = 6;
            mon_p.fg = t.text;
            mon_p.accent = t.surface_alt;
            mon_p.focus_color = t.focus;
            mon_p.icon = IconKind::Waveform;
            mon_p.text = TextRef::Static(if model.settings.live_monitoring {
                "暂停监测"
            } else {
                "开启监测"
            });
            mon_p.max_lines = 1;
            mon_p.focusable = true;
            ui.button(tk::DIAG_BTN_MONITOR, mon_p)
        })
    })
}

fn diag_row(
    ui: &mut Children<'_, '_>,
    row_key: Key,
    lbl_key: Key,
    val_key: Key,
    label: &'static str,
    value: TextRef,
    t: &Theme,
) -> Result<(), BuildError> {
    let mut row_p = NodeProps::new();
    row_p.width = Length::Fill(1);
    row_p.height = Length::Px(21);
    row_p.padding = EdgeInsets::symmetric(8, 2);
    row_p.gap = 6;
    row_p.align_cross = Align::Center;
    row_p.bg = Some(t.surface);
    row_p.border_color = Some(t.border);
    row_p.border_width = 1;
    row_p.radius = 6;
    ui.row(row_key, row_p, |ui| {
        let mut lbl = NodeProps::new();
        lbl.width = Length::Px(84);
        lbl.height = Length::Content;
        lbl.fg = t.text_muted;
        lbl.font = FontId::Small12;
        lbl.text = TextRef::Static(label);
        lbl.max_lines = 1;
        ui.label(lbl_key, lbl)?;

        let mut val = NodeProps::new();
        val.width = Length::Fill(1);
        val.height = Length::Content;
        val.fg = t.text;
        val.font = FontId::Small12;
        val.text_align = TextAlign::End;
        val.text = value;
        val.max_lines = 1;
        ui.label(val_key, val)
    })
}
