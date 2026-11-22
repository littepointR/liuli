use liuli::prelude::Lf4Font;

pub const SETTINGS_LF4_BYTES: &[u8] = include_bytes!("../assets/settings.lf4");
pub const SETTINGS_CHARSET: &str = include_str!("../assets/charset.txt");

pub fn settings_font() -> Lf4Font<'static> {
    Lf4Font::from_bytes(SETTINGS_LF4_BYTES).expect("embedded settings.lf4 must be valid")
}
