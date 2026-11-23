use liuli::prelude::*;

use crate::terminal::{TerminalModel, terminal_keys as tk};
use crate::theme::Theme;

pub fn build_dashboard(
    model: &TerminalModel,
    ui: &mut Children<'_, '_>,
    t: &Theme,
) -> Result<(), BuildError> {
    let mut scroll_p = NodeProps::new();
    scroll_p.width = Length::Fill(1);
    scroll_p.height = Length::Fill(1);
    scroll_p.padding = EdgeInsets::symmetric(8, 6);
    scroll_p.gap = 8;
    scroll_p.accent = t.accent;

    ui.scroll_y(tk::DASH_SCROLL, scroll_p, |ui| {
        // 1. Top KPI row: Temperature + System Load ProgressBar
        let mut kpi_p = NodeProps::new();
        kpi_p.width = Length::Fill(1);
        kpi_p.height = Length::Px(46);
        kpi_p.gap = 6;
        kpi_p.align_cross = Align::Stretch;
        ui.row(tk::DASH_KPI_ROW, kpi_p, |ui| {
            let mut card_p = NodeProps::new();
            card_p.width = Length::Fill(1);
            card_p.height = Length::Fill(1);
            card_p.padding = EdgeInsets::symmetric(8, 4);
            card_p.gap = 2;
            card_p.bg = Some(t.surface);
            card_p.border_color = Some(t.border);
            card_p.border_width = 1;
            card_p.radius = 6;
            ui.column(tk::DASH_TEMP_CARD, card_p, |ui| {
                let mut lbl = NodeProps::new();
                lbl.width = Length::Fill(1);
                lbl.height = Length::Content;
                lbl.fg = t.text_muted;
                lbl.font = FontId::Small12;
                lbl.text = TextRef::Static("设备温度");
                lbl.max_lines = 1;
                ui.label(tk::DASH_TEMP_LBL, lbl)?;

                let whole = model.settings.temp_deci_c / 10;
                let frac = (model.settings.temp_deci_c % 10).abs();
                let temp_txt = ui.text_fmt(format_args!("{whole}.{frac} °C"))?;
                let mut val = NodeProps::new();
                val.width = Length::Fill(1);
                val.height = Length::Content;
                val.fg = t.text;
                val.font = FontId::Body16;
                val.text = temp_txt;
                val.max_lines = 1;
                ui.label(tk::DASH_TEMP_VAL, val)
            })?;

            ui.column(tk::DASH_LOAD_CARD, card_p, |ui| {
                let mut hdr = NodeProps::new();
                hdr.width = Length::Fill(1);
                hdr.height = Length::Px(16);
                hdr.align_cross = Align::Center;
                ui.row(tk::DASH_LOAD_HDR, hdr, |ui| {
                    let mut lbl = NodeProps::new();
                    lbl.width = Length::Fill(1);
                    lbl.height = Length::Content;
                    lbl.fg = t.text_muted;
                    lbl.font = FontId::Small12;
                    lbl.text = TextRef::Static("系统负载");
                    lbl.max_lines = 1;
                    ui.label(tk::DASH_LOAD_LBL, lbl)?;

                    let load_txt = ui.text_fmt(format_args!("{:>3}%", model.load_pct))?;
                    let mut val = NodeProps::new();
                    val.width = Length::Px(42);
                    val.height = Length::Content;
                    val.fg = t.text;
                    val.font = FontId::Small12;
                    val.text_align = TextAlign::End;
                    val.text = load_txt;
                    val.max_lines = 1;
                    ui.label(tk::DASH_LOAD_VAL, val)
                })?;

                let mut bar = NodeProps::new();
                bar.width = Length::Fill(1);
                bar.height = Length::Px(10);
                bar.bg = Some(t.track_off);
                bar.accent = t.accent;
                bar.radius = 4;
                bar.min = 0;
                bar.max = 100;
                bar.value = model.load_pct;
                ui.progress_bar(tk::DASH_LOAD_BAR, bar)
            })
        })?;

        // 2. Sparkline History Card
        let mut wave_card = NodeProps::new();
        wave_card.width = Length::Fill(1);
        wave_card.height = Length::Px(80);
        wave_card.padding = EdgeInsets::symmetric(8, 5);
        wave_card.gap = 4;
        wave_card.align_cross = Align::Stretch;
        wave_card.bg = Some(t.surface);
        wave_card.border_color = Some(t.border);
        wave_card.border_width = 1;
        wave_card.radius = 6;
        ui.column(tk::DASH_WAVE_CARD, wave_card, |ui| {
            let mut hdr = NodeProps::new();
            hdr.width = Length::Fill(1);
            hdr.height = Length::Px(16);
            hdr.align_cross = Align::Center;
            ui.row(tk::DASH_WAVE_HDR, hdr, |ui| {
                let mut title = NodeProps::new();
                title.width = Length::Fill(1);
                title.height = Length::Content;
                title.fg = t.text;
                title.font = FontId::Small12;
                title.text = TextRef::Static("实时波形");
                title.max_lines = 1;
                ui.label(tk::DASH_WAVE_TITLE, title)?;

                let meta_txt =
                    ui.text_fmt(format_args!("{:>2}/32 点", model.temp_history.len()))?;
                let mut meta = NodeProps::new();
                meta.width = Length::Px(64);
                meta.height = Length::Content;
                meta.fg = t.text_muted;
                meta.font = FontId::Small12;
                meta.text_align = TextAlign::End;
                meta.text = meta_txt;
                meta.max_lines = 1;
                ui.label(tk::DASH_WAVE_META, meta)
            })?;

            let mut spark = NodeProps::new();
            spark.width = Length::Fill(1);
            spark.height = Length::Px(50);
            spark.bg = Some(t.surface_alt);
            spark.border_color = Some(t.border);
            spark.border_width = 1;
            spark.radius = 4;
            spark.accent = t.accent;
            spark.min = 220;
            spark.max = 260;
            ui.sparkline_ring(tk::DASH_SPARKLINE, &model.temp_history, spark)
        })?;

        // 3. Action Controls Row (padding gives 3px clearance for unclipped 2px focus rings)
        let mut act = NodeProps::new();
        act.width = Length::Fill(1);
        act.height = Length::Px(34);
        act.padding = EdgeInsets::symmetric(4, 3);
        act.gap = 6;
        act.align_cross = Align::Center;
        ui.row(tk::DASH_ACT_ROW, act, |ui| {
            let mut pause_p = NodeProps::new();
            pause_p.width = Length::Px(108);
            pause_p.height = Length::Px(28);
            pause_p.padding = EdgeInsets::symmetric(8, 3);
            pause_p.bg = Some(t.surface);
            pause_p.border_color = Some(t.border);
            pause_p.border_width = 1;
            pause_p.radius = 6;
            pause_p.fg = t.text;
            pause_p.accent = t.surface_alt;
            pause_p.focus_color = t.focus;
            pause_p.icon = IconKind::Waveform;
            pause_p.text = TextRef::Static(if model.settings.live_monitoring {
                "暂停采样"
            } else {
                "开启采样"
            });
            pause_p.max_lines = 1;
            pause_p.focusable = true;
            ui.button(tk::DASH_BTN_PAUSE, pause_p)?;

            let mut adj_p = NodeProps::new();
            adj_p.width = Length::Px(34);
            adj_p.height = Length::Px(28);
            adj_p.padding = EdgeInsets::symmetric(6, 3);
            adj_p.bg = Some(t.surface_alt);
            adj_p.border_color = Some(t.border);
            adj_p.border_width = 1;
            adj_p.radius = 6;
            adj_p.fg = t.text;
            adj_p.accent = t.surface_alt;
            adj_p.focus_color = t.focus;
            adj_p.text_align = TextAlign::Center;
            adj_p.max_lines = 1;
            adj_p.focusable = true;
            adj_p.repeatable = true;

            adj_p.text = TextRef::Static("-");
            ui.button(tk::DASH_BTN_DEC, adj_p)?;

            adj_p.text = TextRef::Static("+");
            ui.button(tk::DASH_BTN_INC, adj_p)?;

            let mut status = NodeProps::new();
            status.width = Length::Fill(1);
            status.height = Length::Content;
            status.fg = t.text_muted;
            status.font = FontId::Small12;
            status.text_align = TextAlign::End;
            status.text = TextRef::Static(if model.settings.live_monitoring {
                "采样正常"
            } else {
                "采样暂停"
            });
            status.max_lines = 1;
            ui.label(tk::DASH_STATUS_LBL, status)
        })?;

        // 4. Below-the-fold status summary row (allows Dashboard independent scroll testing)
        let mut note_p = NodeProps::new();
        note_p.width = Length::Fill(1);
        note_p.height = Length::Px(28);
        note_p.padding = EdgeInsets::symmetric(8, 4);
        note_p.align_cross = Align::Center;
        note_p.bg = Some(t.surface);
        note_p.border_color = Some(t.border);
        note_p.border_width = 1;
        note_p.radius = 6;
        ui.row(tk::DASH_NOTE_ROW, note_p, |ui| {
            let mut n_lbl = NodeProps::new();
            n_lbl.width = Length::Fill(1);
            n_lbl.height = Length::Content;
            n_lbl.fg = t.text_muted;
            n_lbl.font = FontId::Small12;
            n_lbl.text = TextRef::Static("实时监测 · 温度负载波形");
            n_lbl.max_lines = 1;
            ui.label(tk::DASH_NOTE_LBL, n_lbl)
        })
    })
}
