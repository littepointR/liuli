#![no_std]
#![forbid(unsafe_code)]

pub mod font;
pub mod model;
pub mod theme;
pub mod view;

pub use font::{SETTINGS_CHARSET, SETTINGS_LF4_BYTES, settings_font};
pub use model::{SettingsModel, SettingsMsg};
pub use theme::{DARK_THEME, LIGHT_THEME, Theme};
pub use view::{SettingsApp, keys};

#[cfg(test)]
mod tests {
    use crate::font::settings_font;

    #[test]
    fn all_settings_ui_strings_present_without_fallback() {
        let font = settings_font();
        let strings = [
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
        ];
        for s in strings {
            for ch in s.chars() {
                assert!(
                    font.lookup_exact(ch).is_some(),
                    "character '{}' (U+{:04X}) missing from settings.lf4",
                    ch,
                    ch as u32
                );
            }
        }
    }
}
