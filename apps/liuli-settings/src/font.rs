use liuli::prelude::{FontSet, Lf4Font};

pub const SETTINGS_LF4_BYTES: &[u8] = include_bytes!("../assets/settings.lf4");
pub const SETTINGS_CHARSET: &str = include_str!("../assets/charset.txt");
pub const SMALL12_LF4_BYTES: &[u8] = include_bytes!("../assets/small12.lf4");
pub const SMALL12_CHARSET: &str = include_str!("../assets/charset_small12.txt");

pub fn settings_font() -> Lf4Font<'static> {
    Lf4Font::from_bytes(SETTINGS_LF4_BYTES).expect("embedded settings.lf4 must be valid")
}

pub fn small12_font() -> Lf4Font<'static> {
    Lf4Font::from_bytes(SMALL12_LF4_BYTES).expect("embedded small12.lf4 must be valid")
}

pub fn app_fonts() -> FontSet<'static> {
    FontSet::new(settings_font(), small12_font())
}
