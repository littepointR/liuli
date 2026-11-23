#![no_std]
#![forbid(unsafe_code)]

pub mod font;
pub mod model;
pub mod pages;
pub mod terminal;
pub mod theme;
pub mod view;

pub use font::{
    SETTINGS_CHARSET, SETTINGS_LF4_BYTES, SMALL12_CHARSET, SMALL12_LF4_BYTES, app_fonts,
    settings_font, small12_font,
};
pub use model::{SettingsModel, SettingsMsg};
pub use terminal::{
    DiagnosticsSnapshot, PageId, PageMemory, SAMPLE_INTERVAL_MS, TOAST_DURATION_MS, TerminalApp,
    TerminalModel, TerminalMsg, ToastState, terminal_keys,
};
pub use theme::{DARK_THEME, LIGHT_THEME, Theme};
pub use view::{SettingsApp, keys};

#[cfg(test)]
mod tests {
    use crate::font::{
        SETTINGS_LF4_BYTES, SMALL12_LF4_BYTES, app_fonts, settings_font, small12_font,
    };

    #[test]
    fn all_settings_ui_strings_present_without_fallback() {
        let font = settings_font();
        let small = small12_font();
        let fonts = app_fonts();
        assert!(
            SETTINGS_LF4_BYTES.len() + SMALL12_LF4_BYTES.len() <= 32_768,
            "combined LF4 assets exceeded 32 KiB: {} B",
            SETTINGS_LF4_BYTES.len() + SMALL12_LF4_BYTES.len()
        );
        assert_eq!(fonts.body16().line_height(), 18);
        assert_eq!(fonts.small12().line_height(), 14);

        let body_strings = [
            "设备设置",
            "屏幕亮度",
            "提示音",
            "深色主题",
            "实时监测",
            "设备温度",
            "恢复默认",
            "重设",
            "开启",
            "关闭",
            "旋钮切换 · 确认编辑 · 返回",
            "恢复默认设置？",
            "将恢复出厂默认值。",
            "取消",
            "恢复",
            "0123456789.% °C",
            "仪表盘",
            "设置",
            "诊断",
            "暂停采样",
            "开启采样",
            "刷新",
            "暂停监测",
            "开启监测",
            "已恢复默认",
            "诊断已刷新",
            "-+",
        ];
        for s in body_strings {
            for ch in s.chars() {
                assert!(
                    font.lookup_exact(ch).is_some(),
                    "character '{}' (U+{:04X}) missing from settings.lf4",
                    ch,
                    ch as u32
                );
            }
        }

        let small_strings = [
            "设备温度",
            "系统负载",
            "实时波形",
            "32/32 点",
            "采样正常",
            "采样暂停",
            "实时监测 · 温度负载波形",
            "采样状态",
            "帧传输",
            "帧总计",
            "存储状态",
            "温度负载",
            "当前时间",
            "正常 · 32/32点",
            "暂停 · 0/32点",
            "30 条带 / 76800 像素",
            "12 帧 · CMD 99",
            "内存正常 · <80KB",
            "23.6 °C · 59%",
            "3000 ms · MEAS 0",
            "页签 · 监测设置诊断 · 状态正常",
            "0123456789.%/: -ms px KB",
        ];
        for s in small_strings {
            for ch in s.chars() {
                assert!(
                    small.lookup_exact(ch).is_some(),
                    "character '{}' (U+{:04X}) missing from small12.lf4",
                    ch,
                    ch as u32
                );
            }
        }
    }
}
