//! Pack a binary PPM (RGB8) as an MCU-native L565 asset.
use std::{env, fs, path::Path};
fn pack(source: &Path, output: &Path) -> Result<(), String> {
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    let (width, height, rgb) = liuli_sim::image::decode_ppm(&bytes)?;
    let w = u16::try_from(width).map_err(|_| "width exceeds 65535")?;
    let h = u16::try_from(height).map_err(|_| "height exceeds 65535")?;
    if w == 0 || h == 0 {
        return Err("image dimensions must be positive".into());
    }
    let mut asset = b"L565\x01\x00".to_vec();
    asset.extend(w.to_le_bytes());
    asset.extend(h.to_le_bytes());
    for px in rgb.as_chunks::<3>().0 {
        let word =
            (u16::from(px[0] >> 3) << 11) | (u16::from(px[1] >> 2) << 5) | u16::from(px[2] >> 3);
        asset.extend(word.to_le_bytes());
    }
    fs::write(output, asset).map_err(|e| e.to_string())
}
fn main() -> Result<(), String> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [source, output] = args.as_slice() else {
        return Err("usage: liuli-image INPUT.ppm OUTPUT.l565".into());
    };
    pack(Path::new(source), Path::new(output))
}
