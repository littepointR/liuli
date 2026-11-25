/// Converts an RGB565 word into RGB888 using standard bit replication.
#[inline]
pub fn rgb565_to_rgb8(word: u16) -> [u8; 3] {
    let r5 = ((word >> 11) & 0x1F) as u8;
    let g6 = ((word >> 5) & 0x3F) as u8;
    let b5 = (word & 0x1F) as u8;
    let r8 = (r5 << 3) | (r5 >> 2);
    let g8 = (g6 << 2) | (g6 >> 4);
    let b8 = (b5 << 3) | (b5 >> 2);
    [r8, g8, b8]
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
    let row_bytes = (width as usize) * 3;
    assert_eq!(rgb8.len(), row_bytes * (height as usize));

    let mut raw_scanlines = Vec::with_capacity((row_bytes + 1) * (height as usize));
    for y in 0..height as usize {
        raw_scanlines.push(0u8); // filter type 0 (None)
        raw_scanlines.extend_from_slice(&rgb8[y * row_bytes..(y + 1) * row_bytes]);
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
    ihdr[9] = 2; // color type: Truecolor RGB
    ihdr[10] = 0; // compression
    ihdr[11] = 0; // filter
    ihdr[12] = 0; // interlace
    write_png_chunk(&mut out, b"IHDR", &ihdr);
    write_png_chunk(&mut out, b"IDAT", &zlib);
    write_png_chunk(&mut out, b"IEND", &[]);
    out
}

/// Decodes an RGB8 filter-0 stored-zlib PNG and verifies signature, chunk CRCs, and Adler-32.
pub fn decode_png_rgb8(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if bytes.len() < 8 || bytes[..8] != PNG_SIG {
        return Err("invalid PNG signature".into());
    }
    let mut pos = 8usize;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut saw_ihdr = false;
    let mut saw_iend = false;
    let mut idat = Vec::new();

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
                if data[8] != 8 || data[9] != 2 || data[10] != 0 || data[11] != 0 || data[12] != 0 {
                    return Err("unsupported PNG IHDR format".into());
                }
                saw_ihdr = true;
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

    let row_bytes = (width as usize) * 3;
    if raw.len() != (row_bytes + 1) * (height as usize) {
        return Err("scanline byte length mismatch".into());
    }
    let mut rgb8 = Vec::with_capacity(row_bytes * (height as usize));
    for y in 0..height as usize {
        let row = &raw[y * (row_bytes + 1)..(y + 1) * (row_bytes + 1)];
        if row[0] != 0 {
            return Err(format!("unsupported filter type {} at row {y}", row[0]));
        }
        rgb8.extend_from_slice(&row[1..]);
    }
    Ok((width, height, rgb8))
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
}
