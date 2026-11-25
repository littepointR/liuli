/// Converts an RGB565 word into RGB888 using standard bit replication.
#[inline]
pub fn rgb565_to_rgb8(word: u16) -> [u8; 3] {
    liuli::core::color::rgb565_u16_to_rgb888(word).to_bytes()
}

pub fn encode_ppm(width: u32, height: u32, rgb8: &[u8]) -> Vec<u8> {
    assert_eq!(rgb8.len(), (width as usize) * (height as usize) * 3);
    let header = format!("P6\n{width} {height}\n255\n");
    let mut out = Vec::with_capacity(header.len() + rgb8.len());
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(rgb8);
    out
}

pub fn decode_ppm(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut pos = 0usize;
    let magic = next_token(bytes, &mut pos)?;
    if magic != "P6" {
        return Err(format!("expected P6 PPM, got '{magic}'"));
    }
    let width: u32 = next_token(bytes, &mut pos)?
        .parse()
        .map_err(|e| format!("bad PPM width: {e}"))?;
    let height: u32 = next_token(bytes, &mut pos)?
        .parse()
        .map_err(|e| format!("bad PPM height: {e}"))?;
    let maxval: u32 = next_token(bytes, &mut pos)?
        .parse()
        .map_err(|e| format!("bad PPM maxval: {e}"))?;
    if maxval != 255 {
        return Err(format!("expected PPM maxval 255, got {maxval}"));
    }
    if pos >= bytes.len() || !bytes[pos].is_ascii_whitespace() {
        return Err("missing whitespace after PPM header".into());
    }
    pos += 1;
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|p| p.checked_mul(3))
        .ok_or_else(|| "PPM dimensions overflow".to_string())?;
    if bytes.len() - pos != expected {
        return Err(format!(
            "PPM payload length mismatch: expected {expected}, got {}",
            bytes.len() - pos
        ));
    }
    Ok((width, height, bytes[pos..].to_vec()))
}

fn next_token<'a>(bytes: &'a [u8], pos: &mut usize) -> Result<&'a str, String> {
    while *pos < bytes.len() {
        if bytes[*pos] == b'#' {
            while *pos < bytes.len() && bytes[*pos] != b'\n' {
                *pos += 1;
            }
        } else if bytes[*pos].is_ascii_whitespace() {
            *pos += 1;
        } else {
            break;
        }
    }
    let start = *pos;
    while *pos < bytes.len() && !bytes[*pos].is_ascii_whitespace() && bytes[*pos] != b'#' {
        *pos += 1;
    }
    if start == *pos {
        return Err("unexpected EOF in PPM header".into());
    }
    std::str::from_utf8(&bytes[start..*pos]).map_err(|e| format!("PPM header UTF-8 error: {e}"))
}

const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// Encodes RGB888 pixels into a valid PNG using filter 0 and uncompressed zlib stored blocks (`<= 65,535` B).
pub fn encode_png_rgb8(width: u32, height: u32, rgb8: &[u8]) -> Vec<u8> {
    encode_png_with_type(width, height, 2, 3, rgb8)
}

/// Encodes RGBA8888 pixels into a valid PNG using filter 0 and uncompressed zlib stored blocks.
pub fn encode_png_rgba8(width: u32, height: u32, rgba8: &[u8]) -> Vec<u8> {
    encode_png_with_type(width, height, 6, 4, rgba8)
}

fn encode_png_with_type(
    width: u32,
    height: u32,
    color_type: u8,
    channels: usize,
    pixels: &[u8],
) -> Vec<u8> {
    let row_bytes = (width as usize) * channels;
    assert_eq!(pixels.len(), row_bytes * (height as usize));

    let mut raw_scanlines = Vec::with_capacity((row_bytes + 1) * (height as usize));
    for y in 0..height as usize {
        raw_scanlines.push(0u8); // filter type 0 (None)
        raw_scanlines.extend_from_slice(&pixels[y * row_bytes..(y + 1) * row_bytes]);
    }

    let mut zlib = Vec::with_capacity(raw_scanlines.len() + 64);
    zlib.push(0x78); // CMF: deflate, 32K window
    zlib.push(0x01); // FLG: no preset dict, FCHECK valid for 0x7801 (0x7801 % 31 == 0)

    if raw_scanlines.is_empty() {
        zlib.push(0x01); // BFINAL=1, BTYPE=00
        zlib.extend_from_slice(&0u16.to_le_bytes());
        zlib.extend_from_slice(&(!0u16).to_le_bytes());
    } else {
        let chunks: Vec<&[u8]> = raw_scanlines.chunks(65_535).collect();
        for (idx, chunk) in chunks.iter().enumerate() {
            let is_last = idx + 1 == chunks.len();
            zlib.push(if is_last { 0x01 } else { 0x00 });
            let len = chunk.len() as u16;
            let nlen = !len;
            zlib.extend_from_slice(&len.to_le_bytes());
            zlib.extend_from_slice(&nlen.to_le_bytes());
            zlib.extend_from_slice(chunk);
        }
    }
    zlib.extend_from_slice(&adler32(&raw_scanlines).to_be_bytes());

    let mut out = Vec::with_capacity(64 + zlib.len());
    out.extend_from_slice(&PNG_SIG);

    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = color_type;
    ihdr[10] = 0; // compression
    ihdr[11] = 0; // filter
    ihdr[12] = 0; // interlace
    write_png_chunk(&mut out, b"IHDR", &ihdr);
    write_png_chunk(&mut out, b"IDAT", &zlib);
    write_png_chunk(&mut out, b"IEND", &[]);
    out
}

struct ParsedPng {
    width: u32,
    height: u32,
    color_type: u8,
    palette: Vec<[u8; 3]>,
    trns: Vec<u8>,
    raw: Vec<u8>,
}

fn parse_png_stored(bytes: &[u8]) -> Result<ParsedPng, String> {
    if bytes.len() < 8 || bytes[..8] != PNG_SIG {
        return Err("invalid PNG signature".into());
    }
    let mut pos = 8usize;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut color_type = 0u8;
    let mut saw_ihdr = false;
    let mut saw_iend = false;
    let mut idat = Vec::new();
    let mut palette = Vec::new();
    let mut trns = Vec::new();

    while pos < bytes.len() {
        if pos + 12 > bytes.len() {
            return Err("truncated PNG chunk header".into());
        }
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        let ctype = &bytes[pos + 4..pos + 8];
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(len)
            .ok_or_else(|| "PNG chunk length overflow".to_string())?;
        if data_end + 4 > bytes.len() {
            return Err("truncated PNG chunk payload".into());
        }
        let data = &bytes[data_start..data_end];
        let expected_crc = u32::from_be_bytes(bytes[data_end..data_end + 4].try_into().unwrap());
        let actual_crc = png_chunk_crc(ctype, data);
        if expected_crc != actual_crc {
            return Err(format!(
                "PNG CRC mismatch in chunk {:?}: expected 0x{expected_crc:08X}, got 0x{actual_crc:08X}",
                std::str::from_utf8(ctype)
            ));
        }

        match ctype {
            b"IHDR" => {
                if data.len() != 13 {
                    return Err("invalid IHDR length".into());
                }
                width = u32::from_be_bytes(data[0..4].try_into().unwrap());
                height = u32::from_be_bytes(data[4..8].try_into().unwrap());
                color_type = data[9];
                let supported_ct = matches!(color_type, 0 | 2 | 3 | 4 | 6);
                if data[8] != 8
                    || !supported_ct
                    || data[10] != 0
                    || data[11] != 0
                    || data[12] != 0
                {
                    return Err("unsupported PNG IHDR format".into());
                }
                saw_ihdr = true;
            }
            b"PLTE" => {
                if !data.len().is_multiple_of(3) {
                    return Err("invalid PNG PLTE length".into());
                }
                palette = data.as_chunks::<3>().0.to_vec();
            }
            b"tRNS" => {
                trns = data.to_vec();
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => {
                saw_iend = true;
                break;
            }
            _ => {}
        }
        pos = data_end + 4;
    }

    if !saw_ihdr || !saw_iend {
        return Err("missing IHDR or IEND chunk".into());
    }
    if idat.len() < 6 {
        return Err("zlib stream too short".into());
    }
    let cmf = idat[0] as u16;
    let flg = idat[1] as u16;
    if !(cmf * 256 + flg).is_multiple_of(31) || (idat[0] & 0x0F) != 8 {
        return Err("invalid zlib header".into());
    }

    let mut zpos = 2usize;
    let mut raw = Vec::new();
    loop {
        if zpos + 5 > idat.len() {
            return Err("truncated deflate stored block".into());
        }
        let hdr = idat[zpos];
        zpos += 1;
        if (hdr & 0x06) != 0 {
            return Err(
                "unsupported compressed deflate block (only stored supported in decoder)".into(),
            );
        }
        let len = u16::from_le_bytes([idat[zpos], idat[zpos + 1]]);
        let nlen = u16::from_le_bytes([idat[zpos + 2], idat[zpos + 3]]);
        zpos += 4;
        if len != !nlen {
            return Err("deflate LEN/NLEN mismatch".into());
        }
        if zpos + (len as usize) > idat.len() {
            return Err("deflate block past end".into());
        }
        raw.extend_from_slice(&idat[zpos..zpos + (len as usize)]);
        zpos += len as usize;
        if (hdr & 0x01) != 0 {
            break;
        }
    }
    if zpos + 4 != idat.len() {
        return Err("trailing bytes or missing Adler32".into());
    }
    let expected_adler = u32::from_be_bytes(idat[zpos..zpos + 4].try_into().unwrap());
    let actual_adler = adler32(&raw);
    if expected_adler != actual_adler {
        return Err(format!(
            "Adler32 mismatch: expected 0x{expected_adler:08X}, got 0x{actual_adler:08X}"
        ));
    }

    Ok(ParsedPng {
        width,
        height,
        color_type,
        palette,
        trns,
        raw,
    })
}

fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let a = i32::from(a);
    let b = i32::from(b);
    let c = i32::from(c);
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

fn unfilter_png_scanlines(
    raw: &[u8],
    width: u32,
    height: u32,
    bpp: usize,
) -> Result<Vec<u8>, String> {
    let row_bytes = (width as usize)
        .checked_mul(bpp)
        .ok_or_else(|| "PNG row length overflow".to_string())?;
    let expected = row_bytes
        .checked_add(1)
        .and_then(|stride| stride.checked_mul(height as usize))
        .ok_or_else(|| "PNG payload length overflow".to_string())?;
    if raw.len() != expected {
        return Err("scanline byte length mismatch".into());
    }

    let mut out = vec![0u8; row_bytes * (height as usize)];
    for y in 0..height as usize {
        let filter = raw[y * (row_bytes + 1)];
        let src = &raw[y * (row_bytes + 1) + 1..(y + 1) * (row_bytes + 1)];
        let (prev_all, curr_all) = out.split_at_mut(y * row_bytes);
        let prev: &[u8] = if y == 0 {
            &[]
        } else {
            &prev_all[(y - 1) * row_bytes..y * row_bytes]
        };
        let dst = &mut curr_all[..row_bytes];
        for i in 0..row_bytes {
            let a = if i >= bpp { dst[i - bpp] } else { 0 };
            let b = if y > 0 { prev[i] } else { 0 };
            let c = if y > 0 && i >= bpp { prev[i - bpp] } else { 0 };
            dst[i] = match filter {
                0 => src[i],
                1 => src[i].wrapping_add(a),
                2 => src[i].wrapping_add(b),
                3 => src[i].wrapping_add(((u16::from(a) + u16::from(b)) / 2) as u8),
                4 => src[i].wrapping_add(paeth_predictor(a, b, c)),
                _ => return Err(format!("unsupported filter type {filter} at row {y}")),
            };
        }
    }
    Ok(out)
}

/// Decodes an RGB8 stored-zlib PNG (filters 0..=4) and verifies signature, chunk CRCs, and Adler-32.
pub fn decode_png_rgb8(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let parsed = parse_png_stored(bytes)?;
    if parsed.color_type != 2 {
        return Err("unsupported PNG IHDR format".into());
    }
    let rgb8 = unfilter_png_scanlines(&parsed.raw, parsed.width, parsed.height, 3)?;
    Ok((parsed.width, parsed.height, rgb8))
}

/// Decodes a stored-zlib PNG (`Gray8`/`RGB8`/`Indexed8`/`GrayA8`/`RGBA8`, filters `0..=4`) to `RGBA8`.
pub fn decode_png_rgba8(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let parsed = parse_png_stored(bytes)?;
    let channels = match parsed.color_type {
        0 => 1usize,
        2 => 3usize,
        3 => 1usize,
        4 => 2usize,
        6 => 4usize,
        ct => return Err(format!("unsupported PNG color type {ct}")),
    };
    let pixels = unfilter_png_scanlines(&parsed.raw, parsed.width, parsed.height, channels)?;
    let px_count = (parsed.width as usize) * (parsed.height as usize);
    let mut rgba = Vec::with_capacity(px_count * 4);
    match parsed.color_type {
        0 => {
            for &g in &pixels {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        2 => {
            for px in pixels.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        3 => {
            for &idx in &pixels {
                let rgb = *parsed
                    .palette
                    .get(usize::from(idx))
                    .ok_or_else(|| format!("PNG palette index {idx} out of bounds"))?;
                let a = parsed.trns.get(usize::from(idx)).copied().unwrap_or(255);
                rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
            }
        }
        4 => {
            for px in pixels.as_chunks::<2>().0 {
                rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
        }
        6 => rgba = pixels,
        _ => unreachable!(),
    }
    Ok((parsed.width, parsed.height, rgba))
}

/// Decodes a Netpbm `P7` PAM image (`DEPTH` 1..=4, `MAXVAL 255`) into tight `RGBA8` bytes.
pub fn decode_pam_rgba8(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let first_nl = bytes
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| "missing PAM P7 magic header".to_string())?;
    let magic = std::str::from_utf8(&bytes[..first_nl])
        .map_err(|e| format!("PAM header UTF-8 error: {e}"))?
        .trim();
    if magic != "P7" {
        return Err(format!("expected P7 PAM, got '{magic}'"));
    }
    let mut pos = first_nl + 1;

    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let mut depth: Option<u32> = None;
    let mut maxval: Option<u32> = None;
    let mut saw_endhdr = false;

    while pos < bytes.len() {
        let rel_nl = bytes[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or_else(|| "unterminated PAM header before ENDHDR".to_string())?;
        let line = std::str::from_utf8(&bytes[pos..pos + rel_nl])
            .map_err(|e| format!("PAM header UTF-8 error: {e}"))?
            .trim();
        pos += rel_nl + 1;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "ENDHDR" {
            saw_endhdr = true;
            break;
        }
        let mut parts = line.split_ascii_whitespace();
        let key = parts.next().unwrap_or("");
        let val = parts
            .next()
            .ok_or_else(|| format!("missing value for PAM header key '{key}'"))?;
        match key {
            "WIDTH" => {
                width = Some(
                    val.parse()
                        .map_err(|e| format!("bad PAM WIDTH: {e}"))?,
                )
            }
            "HEIGHT" => {
                height = Some(
                    val.parse()
                        .map_err(|e| format!("bad PAM HEIGHT: {e}"))?,
                )
            }
            "DEPTH" => {
                depth = Some(
                    val.parse()
                        .map_err(|e| format!("bad PAM DEPTH: {e}"))?,
                )
            }
            "MAXVAL" => {
                maxval = Some(
                    val.parse()
                        .map_err(|e| format!("bad PAM MAXVAL: {e}"))?,
                )
            }
            "TUPLTYPE" => {}
            _ => return Err(format!("unsupported PAM header key '{key}'")),
        }
    }

    if !saw_endhdr {
        return Err("missing ENDHDR in PAM header".into());
    }
    let w = width.ok_or("missing PAM WIDTH")?;
    let h = height.ok_or("missing PAM HEIGHT")?;
    let d = depth.ok_or("missing PAM DEPTH")?;
    let mv = maxval.ok_or("missing PAM MAXVAL")?;
    if mv != 255 {
        return Err(format!("expected PAM MAXVAL 255, got {mv}"));
    }
    if !(1..=4).contains(&d) {
        return Err(format!("unsupported PAM DEPTH {d} (expected 1..=4)"));
    }
    let expected = (w as usize)
        .checked_mul(h as usize)
        .and_then(|p| p.checked_mul(d as usize))
        .ok_or_else(|| "PAM dimensions overflow".to_string())?;
    let payload = &bytes[pos..];
    if payload.len() != expected {
        return Err(format!(
            "PAM payload length mismatch: expected {expected}, got {}",
            payload.len()
        ));
    }

    let mut rgba = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for px in payload.chunks_exact(d as usize) {
        match d {
            1 => rgba.extend_from_slice(&[px[0], px[0], px[0], 255]),
            2 => rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]),
            3 => rgba.extend_from_slice(&[px[0], px[1], px[2], 255]),
            4 => rgba.extend_from_slice(&[px[0], px[1], px[2], px[3]]),
            _ => unreachable!(),
        }
    }
    Ok((w, h, rgba))
}

/// Decodes a `P6` PPM image into tight `RGBA8` bytes (`A = 255`).
pub fn decode_ppm_rgba8(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let (w, h, rgb) = decode_ppm(bytes)?;
    let mut rgba = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for px in rgb.as_chunks::<3>().0 {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    Ok((w, h, rgba))
}

/// Decodes a PNG, PAM (`P7`), or PPM (`P6`) byte buffer by magic header into tight `RGBA8`.
pub fn decode_image_rgba8(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if bytes.starts_with(&PNG_SIG) {
        decode_png_rgba8(bytes)
    } else if bytes.starts_with(b"P7") {
        decode_pam_rgba8(bytes)
    } else {
        decode_ppm_rgba8(bytes)
    }
}

fn write_png_chunk(out: &mut Vec<u8>, ctype: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(ctype);
    out.extend_from_slice(data);
    out.extend_from_slice(&png_chunk_crc(ctype, data).to_be_bytes());
}

fn png_chunk_crc(ctype: &[u8], data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in ctype.iter().chain(data.iter()) {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

pub fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65521;
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + byte as u32) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_known_vector_and_multiblock_roundtrip() {
        // 1x1 pure red RGB8 PNG: verify exact Adler32 (a=1+255=0x0100, b=1+256+256+256=0x0301) and roundtrip decode
        let png_1x1 = encode_png_rgb8(1, 1, &[255, 0, 0]);
        assert_eq!(adler32(&[0, 255, 0, 0]), 0x0301_0100);
        let (w, h, px) = decode_png_rgb8(&png_1x1).unwrap();
        assert_eq!((w, h), (1, 1));
        assert_eq!(px, vec![255, 0, 0]);

        // 320x240 payload = 230,640 raw scanline bytes => 4 zlib stored blocks (> 65,535 B)
        let mut big = vec![0u8; 320 * 240 * 3];
        for (i, chunk) in big.as_chunks_mut::<3>().0.iter_mut().enumerate() {
            chunk[0] = (i % 251) as u8;
            chunk[1] = ((i / 3) % 241) as u8;
            chunk[2] = ((i / 7) % 233) as u8;
        }
        let png = encode_png_rgb8(320, 240, &big);
        let (dw, dh, decoded) = decode_png_rgb8(&png).unwrap();
        assert_eq!((dw, dh), (320, 240));
        assert_eq!(decoded, big);

        // PPM roundtrip matches
        let ppm = encode_ppm(320, 240, &big);
        let (pw, ph, pdec) = decode_ppm(&ppm).unwrap();
        assert_eq!((pw, ph), (320, 240));
        assert_eq!(pdec, decoded);
    }

    fn build_custom_filter_png_rgba8(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        // Construct a stored-zlib RGBA8 PNG cycling through all 5 filter types (0..=4) across rows.
        let bpp = 4usize;
        let row_bytes = width as usize * bpp;
        let mut raw_scanlines = Vec::with_capacity((row_bytes + 1) * height as usize);
        for y in 0..height as usize {
            let filter = (y % 5) as u8;
            raw_scanlines.push(filter);
            let curr = &rgba[y * row_bytes..(y + 1) * row_bytes];
            let prev: &[u8] = if y == 0 {
                &[]
            } else {
                &rgba[(y - 1) * row_bytes..y * row_bytes]
            };
            for i in 0..row_bytes {
                let a = if i >= bpp { curr[i - bpp] } else { 0 };
                let b = if y > 0 { prev[i] } else { 0 };
                let c = if y > 0 && i >= bpp { prev[i - bpp] } else { 0 };
                let encoded = match filter {
                    0 => curr[i],
                    1 => curr[i].wrapping_sub(a),
                    2 => curr[i].wrapping_sub(b),
                    3 => curr[i].wrapping_sub(((u16::from(a) + u16::from(b)) / 2) as u8),
                    4 => curr[i].wrapping_sub(paeth_predictor(a, b, c)),
                    _ => unreachable!(),
                };
                raw_scanlines.push(encoded);
            }
        }
        let mut zlib = vec![0x78, 0x01, 0x01];
        let len = raw_scanlines.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(&raw_scanlines);
        zlib.extend_from_slice(&adler32(&raw_scanlines).to_be_bytes());
        let mut out = PNG_SIG.to_vec();
        let mut ihdr = [0u8; 13];
        ihdr[0..4].copy_from_slice(&width.to_be_bytes());
        ihdr[4..8].copy_from_slice(&height.to_be_bytes());
        ihdr[8] = 8;
        ihdr[9] = 6;
        write_png_chunk(&mut out, b"IHDR", &ihdr);
        write_png_chunk(&mut out, b"IDAT", &zlib);
        write_png_chunk(&mut out, b"IEND", &[]);
        out
    }

    #[test]
    fn png_rgba8_filters_and_pam_roundtrip() {
        let mut rgba = Vec::new();
        for y in 0..6u8 {
            for x in 0..5u8 {
                rgba.extend_from_slice(&[
                    x.wrapping_mul(41).wrapping_add(y * 17),
                    y.wrapping_mul(37),
                    (x ^ y).wrapping_mul(53),
                    y * 40 + 15,
                ]);
            }
        }
        let encoded_f0 = encode_png_rgba8(5, 6, &rgba);
        let (w0, h0, d0) = decode_image_rgba8(&encoded_f0).unwrap();
        assert_eq!((w0, h0, d0), (5, 6, rgba.clone()));

        let filtered = build_custom_filter_png_rgba8(5, 6, &rgba);
        let (wf, hf, df) = decode_png_rgba8(&filtered).unwrap();
        assert_eq!((wf, hf, df), (5, 6, rgba.clone()));

        // P7 PAM RGB_ALPHA roundtrip via decode_image_rgba8
        let mut pam = b"P7\n# comment\nWIDTH 5\nHEIGHT 6\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n".to_vec();
        pam.extend_from_slice(&rgba);
        let (wp, hp, dp) = decode_image_rgba8(&pam).unwrap();
        assert_eq!((wp, hp, dp), (5, 6, rgba));
    }
}
