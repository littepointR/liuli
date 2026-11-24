use liuli_core::geometry::{Rect, Size};
use liuli_core::layout::{TextMeasureOut, TextMeasurer};
use liuli_core::view::{FontId, TextAlign, WrapMode};

use crate::font::{
    FontProvider, FontSet, GlyphBitmap, GlyphEncoding, GlyphFormat, GlyphMetrics, Lf4Font,
    ResolvedGlyph,
};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TextLayoutOptions {
    pub max_width: u32,
    pub max_lines: u8,
    pub ellipsis: bool,
    pub align: TextAlign,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct TextLayoutStyle<'a> {
    pub wrap: WrapMode,
    pub letter_spacing: i8,
    pub line_spacing: i8,
    /// Optional relative `[y_min, y_max)` band clip window for early skip/stop.
    pub vis_y: Option<(i32, i32)>,
    pub fallback: Option<&'a Lf4Font<'a>>,
}

#[derive(Copy, Clone)]
pub struct PositionedGlyph<'a> {
    pub x: i32,
    pub y: i32,
    pub line: u16,
    pub glyph: GlyphBitmap<'a>,
}

#[derive(Copy, Clone)]
pub struct PositionedGlyphEx<'a> {
    pub x: i32,
    pub y: i32,
    pub line: u16,
    pub phase: u8,
    pub format: GlyphFormat,
    pub encoding: GlyphEncoding,
    pub from_fallback: bool,
    pub glyph: GlyphBitmap<'a>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct LineDecor {
    pub line: u16,
    pub x_start: i32,
    pub x_end: i32,
    pub baseline_y: i32,
    pub ascent: i16,
}

#[derive(Copy, Clone)]
pub struct PositionedGlyphMetrics {
    pub x: i32,
    pub y: i32,
    pub line: u16,
    pub phase: u8,
    pub format: GlyphFormat,
    pub encoding: GlyphEncoding,
    pub from_fallback: bool,
    pub metrics: GlyphMetrics,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct ProviderLayoutStyle {
    pub wrap: WrapMode,
    pub letter_spacing: i8,
    pub line_spacing: i8,
    pub vis_y: Option<(i32, i32)>,
}

#[derive(Copy, Clone)]
struct ResolvedChar<'a> {
    bitmap: GlyphBitmap<'a>,
    format: GlyphFormat,
    encoding: GlyphEncoding,
    phases: u8,
    is_q2: bool,
    from_fallback: bool,
}

#[inline]
fn resolve_char<'a>(
    font: &Lf4Font<'a>,
    fallback: Option<&Lf4Font<'a>>,
    ch: char,
) -> ResolvedChar<'a> {
    if let Some(bitmap) = font.lookup_exact(ch) {
        return ResolvedChar {
            bitmap,
            format: font.format(),
            encoding: font.encoding(),
            phases: font.subpixel_phases(),
            is_q2: font.advance_is_q2(),
            from_fallback: false,
        };
    }
    if let Some(fb) = fallback
        && let Some(bitmap) = fb.lookup_exact(ch)
    {
        return ResolvedChar {
            bitmap,
            format: fb.format(),
            encoding: fb.encoding(),
            phases: fb.subpixel_phases(),
            is_q2: fb.advance_is_q2(),
            from_fallback: true,
        };
    }
    let bitmap = font.fallback_glyph();
    ResolvedChar {
        bitmap,
        format: font.format(),
        encoding: font.encoding(),
        phases: font.subpixel_phases(),
        is_q2: font.advance_is_q2(),
        from_fallback: false,
    }
}

#[inline]
fn adv_q2(
    ch: char,
    prev: Option<char>,
    res: &ResolvedChar<'_>,
    font: &Lf4Font<'_>,
    letter_spacing: i8,
) -> i64 {
    let base = if res.is_q2 {
        i64::from(res.bitmap.metrics.advance)
    } else {
        i64::from(res.bitmap.metrics.advance) * 4
    };
    let kern = if let Some(p) = prev
        && !res.from_fallback
        && font.has_kerning()
    {
        let k = i64::from(font.kerning(p, ch));
        if font.advance_is_q2() { k } else { k * 4 }
    } else {
        0
    };
    let ls = if prev.is_some() {
        i64::from(letter_spacing) * 4
    } else {
        0
    };
    (base + kern + ls).max(0)
}

#[inline]
fn clamp_i64_to_i32(v: i64) -> i32 {
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

#[inline]
fn emit_glyph_ex<'a>(
    res: &ResolvedChar<'a>,
    pen_x_q2: i64,
    baseline: i64,
    line: u16,
    ink: &mut Rect,
    on_glyph: &mut impl FnMut(PositionedGlyphEx<'a>),
) {
    let (px, phase) = if res.phases <= 1 {
        if res.is_q2 {
            ((pen_x_q2 + 2) >> 2, 0)
        } else {
            (pen_x_q2 >> 2, 0)
        }
    } else {
        let sub = (pen_x_q2 & 3) as u8;
        let phase = (sub * res.phases) >> 2;
        (pen_x_q2 >> 2, phase.min(res.phases - 1))
    };
    let gx = clamp_i64_to_i32(px + i64::from(res.bitmap.metrics.bearing_x));
    let gy = clamp_i64_to_i32(baseline - i64::from(res.bitmap.metrics.bearing_y));
    let g_rect = Rect::new(
        gx,
        gy,
        u32::from(res.bitmap.metrics.width),
        u32::from(res.bitmap.metrics.height),
    );
    *ink = ink.union(g_rect);
    on_glyph(PositionedGlyphEx {
        x: gx,
        y: gy,
        line,
        phase,
        format: res.format,
        encoding: res.encoding,
        from_fallback: res.from_fallback,
        glyph: res.bitmap,
    });
}

#[inline]
fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3040..=0x30FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF
            | 0xFF01..=0xFF60
    )
}

#[inline]
fn no_line_start(ch: char) -> bool {
    matches!(
        ch,
        '，' | '。'
            | '、'
            | '！'
            | '？'
            | '；'
            | '：'
            | '”'
            | '’'
            | '）'
            | '】'
            | '》'
            | '」'
            | '』'
            | '〉'
            | ','
            | '.'
            | '!'
            | '?'
            | ';'
            | ':'
            | ')'
            | ']'
            | '}'
    )
}

#[inline]
fn no_line_end(ch: char) -> bool {
    matches!(
        ch,
        '（' | '【' | '《' | '「' | '『' | '〈' | '“' | '‘' | '(' | '[' | '{'
    )
}

pub fn visit_layout_glyphs<'a>(
    font: &Lf4Font<'a>,
    text: &str,
    opts: TextLayoutOptions,
    mut on_glyph: impl FnMut(PositionedGlyph<'a>),
) -> TextMeasureOut {
    visit_layout_glyphs_styled(
        font,
        text,
        opts,
        TextLayoutStyle::default(),
        |pg| {
            on_glyph(PositionedGlyph {
                x: pg.x,
                y: pg.y,
                line: pg.line,
                glyph: pg.glyph,
            });
        },
        |_| {},
    )
}

pub fn visit_layout_glyphs_styled<'a>(
    font: &Lf4Font<'a>,
    text: &str,
    opts: TextLayoutOptions,
    style: TextLayoutStyle<'a>,
    mut on_glyph: impl FnMut(PositionedGlyphEx<'a>),
    mut on_line: impl FnMut(LineDecor),
) -> TextMeasureOut {
    if text.is_empty() || opts.max_width == 0 {
        return TextMeasureOut {
            size: Size::new(0, 0),
            ink: Rect::ZERO,
            lines: 0,
        };
    }

    let lh = font.line_height();
    let line_step = (lh as i64 + i64::from(style.line_spacing)).max(1);
    let ascent = i64::from(font.ascent());
    let max_lines = if opts.max_lines == 0 {
        u16::MAX
    } else {
        u16::from(opts.max_lines)
    };
    let max_width_q2 = u64::from(opts.max_width) * 4;
    let bytes = text.as_bytes();

    // Single-pass fast path: 1 line, Start-aligned, Char wrap, no ellipsis, no kerning.
    if max_lines == 1
        && opts.align == TextAlign::Start
        && !opts.ellipsis
        && style.wrap == WrapMode::Char
    {
        if let Some((vy0, vy1)) = style.vis_y
            && (0 >= i64::from(vy1) || line_step <= i64::from(vy0))
        {
            return TextMeasureOut {
                size: Size::new(0, line_step as u32),
                ink: Rect::ZERO,
                lines: 1,
            };
        }
        let baseline = ascent;
        let mut pen_x_q2 = 0i64;
        let mut ink = Rect::ZERO;
        let mut prev: Option<char> = None;
        let mut idx = 0usize;
        while idx < bytes.len() {
            let ch = next_char(text, &mut idx);
            if ch == '\r' || ch == '\n' {
                break;
            }
            let res = resolve_char(font, style.fallback, ch);
            let adv = adv_q2(ch, prev, &res, font, style.letter_spacing) as u64;
            if prev.is_some() && (pen_x_q2 as u64).saturating_add(adv) > max_width_q2 {
                break;
            }
            if prev.is_some() && (font.has_kerning() || style.letter_spacing != 0) {
                let extra = adv as i64
                    - if res.is_q2 {
                        i64::from(res.bitmap.metrics.advance)
                    } else {
                        i64::from(res.bitmap.metrics.advance) * 4
                    };
                pen_x_q2 += extra;
                let g_adv = adv as i64 - extra;
                emit_glyph_ex(&res, pen_x_q2, baseline, 0, &mut ink, &mut on_glyph);
                pen_x_q2 += g_adv;
            } else {
                emit_glyph_ex(&res, pen_x_q2, baseline, 0, &mut ink, &mut on_glyph);
                pen_x_q2 += adv as i64;
            }
            prev = Some(ch);
        }
        let end_px = ((pen_x_q2 + 2) >> 2).min(i64::from(opts.max_width)) as u32;
        on_line(LineDecor {
            line: 0,
            x_start: 0,
            x_end: end_px as i32,
            baseline_y: baseline as i32,
            ascent: font.ascent(),
        });
        return TextMeasureOut {
            size: Size::new(end_px, line_step as u32),
            ink,
            lines: 1,
        };
    }

    let mut line_idx = 0u16;
    let mut max_line_w = 0u32;
    let mut ink = Rect::ZERO;
    let mut byte_idx = 0usize;

    loop {
        if line_idx >= max_lines {
            break;
        }
        let line_top = i64::from(line_idx) * line_step;
        if let Some((_, vy1)) = style.vis_y
            && line_top >= i64::from(vy1)
        {
            break;
        }
        let skip_emit = style
            .vis_y
            .is_some_and(|(vy0, _)| line_top + line_step <= i64::from(vy0));
        let baseline = line_top + ascent;
        let is_last_allowed_line = line_idx + 1 == max_lines;

        if is_last_allowed_line
            && opts.ellipsis
            && will_overflow_after(font, &text[byte_idx..], opts.max_width, style)
        {
            let ell_res = resolve_char(font, style.fallback, '…');
            let ell_adv_q2 = if ell_res.is_q2 {
                u64::from(ell_res.bitmap.metrics.advance)
            } else {
                u64::from(ell_res.bitmap.metrics.advance) * 4
            };
            if ell_adv_q2 > max_width_q2 {
                line_idx += 1;
                break;
            }
            let budget_q2 = max_width_q2 - ell_adv_q2;
            let span_start = byte_idx;
            let mut span_end = byte_idx;
            let mut adv_sum_q2 = 0u64;
            let mut prev: Option<char> = None;
            while span_end < bytes.len() {
                let saved = span_end;
                let ch = next_char(text, &mut span_end);
                if ch == '\n' || ch == '\r' {
                    span_end = saved;
                    break;
                }
                let res = resolve_char(font, style.fallback, ch);
                let adv = adv_q2(ch, prev, &res, font, style.letter_spacing) as u64;
                if adv_sum_q2.saturating_add(adv) > budget_q2 {
                    span_end = saved;
                    break;
                }
                adv_sum_q2 += adv;
                prev = Some(ch);
            }
            let total_adv_q2 = adv_sum_q2 + ell_adv_q2;
            let total_px = ((total_adv_q2 + 2) >> 2).min(u64::from(opts.max_width)) as u32;
            if !skip_emit {
                let slack_px = opts.max_width.saturating_sub(total_px);
                let align_x: i64 = match opts.align {
                    TextAlign::Start => 0,
                    TextAlign::Center => i64::from(slack_px / 2),
                    TextAlign::End => i64::from(slack_px),
                };
                let mut pen_x_q2 = align_x * 4;
                let mut cur = span_start;
                let mut prev_emit: Option<char> = None;
                while cur < span_end {
                    let ch = next_char(text, &mut cur);
                    let res = resolve_char(font, style.fallback, ch);
                    let adv = adv_q2(ch, prev_emit, &res, font, style.letter_spacing);
                    let g_adv = if res.is_q2 {
                        i64::from(res.bitmap.metrics.advance)
                    } else {
                        i64::from(res.bitmap.metrics.advance) * 4
                    };
                    pen_x_q2 += adv - g_adv;
                    emit_glyph_ex(&res, pen_x_q2, baseline, line_idx, &mut ink, &mut on_glyph);
                    pen_x_q2 += g_adv;
                    prev_emit = Some(ch);
                }
                emit_glyph_ex(
                    &ell_res,
                    pen_x_q2,
                    baseline,
                    line_idx,
                    &mut ink,
                    &mut on_glyph,
                );
                on_line(LineDecor {
                    line: line_idx,
                    x_start: align_x as i32,
                    x_end: (align_x + i64::from(total_px)) as i32,
                    baseline_y: baseline as i32,
                    ascent: font.ascent(),
                });
            }
            max_line_w = max_line_w.max(total_px);
            line_idx += 1;
            break;
        }

        // Pass 1: measure greedy line advance and byte boundaries (supports Char & Word+CJK wrapping).
        let span_start = byte_idx;
        let mut span_end = byte_idx;
        let mut line_adv_q2 = 0u64;
        let mut line_has_char = false;
        let mut ended_by_newline = false;
        let mut prev: Option<char> = None;
        // (next_start_byte, trim_span_end_byte, trim_adv_q2)
        let mut last_break: Option<(usize, usize, u64)> = None;

        while byte_idx < bytes.len() {
            let saved_idx = byte_idx;
            let ch = next_char(text, &mut byte_idx);

            if ch == '\r' {
                if byte_idx < bytes.len() && bytes[byte_idx] == b'\n' {
                    byte_idx += 1;
                }
                ended_by_newline = true;
                break;
            }
            if ch == '\n' {
                ended_by_newline = true;
                break;
            }

            let res = resolve_char(font, style.fallback, ch);
            let adv = adv_q2(ch, prev, &res, font, style.letter_spacing) as u64;

            if style.wrap == WrapMode::Word && line_has_char {
                if ch == ' ' || ch == '\t' {
                    // Break before trailing spaces, resuming after contiguous spaces.
                    let mut next_after = byte_idx;
                    while next_after < bytes.len() && matches!(bytes[next_after], b' ' | b'\t') {
                        next_after += 1;
                    }
                    last_break = Some((next_after, span_end, line_adv_q2));
                } else if is_cjk_char(ch)
                    && !no_line_start(ch)
                    && prev.is_some_and(|p| !no_line_end(p) && p != ' ' && p != '\t')
                {
                    last_break = Some((saved_idx, span_end, line_adv_q2));
                }
            }

            if line_has_char && line_adv_q2.saturating_add(adv) > max_width_q2 {
                if style.wrap == WrapMode::Word
                    && let Some((brk_next, brk_end, brk_adv)) = last_break
                    && brk_end > span_start
                {
                    byte_idx = brk_next;
                    span_end = brk_end;
                    line_adv_q2 = brk_adv;
                } else {
                    byte_idx = saved_idx;
                }
                break;
            }

            line_has_char = true;
            line_adv_q2 = line_adv_q2.saturating_add(adv);
            span_end = byte_idx;
            prev = Some(ch);

            if style.wrap == WrapMode::Word && matches!(ch, '-' | '/') {
                last_break = Some((byte_idx, span_end, line_adv_q2));
            }
        }

        let line_px = ((line_adv_q2 + 2) >> 2).min(u64::from(opts.max_width)) as u32;
        if !skip_emit {
            let slack_px = opts.max_width.saturating_sub(line_px);
            let align_x: i64 = match opts.align {
                TextAlign::Start => 0,
                TextAlign::Center => i64::from(slack_px / 2),
                TextAlign::End => i64::from(slack_px),
            };
            let mut pen_x_q2 = align_x * 4;
            let mut cur = span_start;
            let mut prev_emit: Option<char> = None;
            while cur < span_end {
                let ch = next_char(text, &mut cur);
                let res = resolve_char(font, style.fallback, ch);
                let adv = adv_q2(ch, prev_emit, &res, font, style.letter_spacing);
                let g_adv = if res.is_q2 {
                    i64::from(res.bitmap.metrics.advance)
                } else {
                    i64::from(res.bitmap.metrics.advance) * 4
                };
                pen_x_q2 += adv - g_adv;
                emit_glyph_ex(&res, pen_x_q2, baseline, line_idx, &mut ink, &mut on_glyph);
                pen_x_q2 += g_adv;
                prev_emit = Some(ch);
            }
            if line_px > 0 {
                on_line(LineDecor {
                    line: line_idx,
                    x_start: align_x as i32,
                    x_end: (align_x + i64::from(line_px)) as i32,
                    baseline_y: baseline as i32,
                    ascent: font.ascent(),
                });
            }
        }

        max_line_w = max_line_w.max(line_px);
        line_idx += 1;

        if byte_idx >= bytes.len() {
            if ended_by_newline && line_idx < max_lines {
                line_idx += 1;
            }
            break;
        }
    }

    let total_h = (i64::from(line_idx) * line_step).max(0) as u32;
    TextMeasureOut {
        size: Size::new(max_line_w, total_h),
        ink,
        lines: line_idx,
    }
}

fn will_overflow_after(
    font: &Lf4Font<'_>,
    rest: &str,
    max_width: u32,
    style: TextLayoutStyle<'_>,
) -> bool {
    let mut pen_x_q2 = 0u64;
    let max_width_q2 = u64::from(max_width) * 4;
    let mut has_char = false;
    let mut idx = 0usize;
    let mut prev: Option<char> = None;
    let bytes = rest.as_bytes();
    while idx < bytes.len() {
        let ch = next_char(rest, &mut idx);
        if ch == '\r' {
            if idx < bytes.len() && bytes[idx] == b'\n' {
                idx += 1;
            }
            return idx <= bytes.len();
        }
        if ch == '\n' {
            return true;
        }
        let res = resolve_char(font, style.fallback, ch);
        let adv = adv_q2(ch, prev, &res, font, style.letter_spacing) as u64;
        if has_char && pen_x_q2.saturating_add(adv) > max_width_q2 {
            return true;
        }
        has_char = true;
        prev = Some(ch);
        pen_x_q2 = pen_x_q2.saturating_add(adv);
        if pen_x_q2 > max_width_q2 && idx < bytes.len() {
            return true;
        }
    }
    false
}

#[inline]
fn next_char(s: &str, idx: &mut usize) -> char {
    let ch = s[*idx..].chars().next().unwrap();
    *idx += ch.len_utf8();
    ch
}

impl TextMeasurer for Lf4Font<'_> {
    fn line_height(&self, _font: FontId) -> u32 {
        Lf4Font::line_height(self)
    }

    fn measure_text(
        &self,
        _font: FontId,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
        align: TextAlign,
    ) -> TextMeasureOut {
        visit_layout_glyphs(
            self,
            text,
            TextLayoutOptions {
                max_width,
                max_lines,
                ellipsis,
                align,
            },
            |_| {},
        )
    }

    fn measure_text_styled(
        &self,
        _font: FontId,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
        align: TextAlign,
        wrap: WrapMode,
        letter_spacing: i8,
        line_spacing: i8,
    ) -> TextMeasureOut {
        visit_layout_glyphs_styled(
            self,
            text,
            TextLayoutOptions {
                max_width,
                max_lines,
                ellipsis,
                align,
            },
            TextLayoutStyle {
                wrap,
                letter_spacing,
                line_spacing,
                vis_y: None,
                fallback: None,
            },
            |_| {},
            |_| {},
        )
    }
}

impl TextMeasurer for FontSet<'_> {
    #[inline]
    fn line_height(&self, font: FontId) -> u32 {
        self.get(font).line_height()
    }

    #[inline]
    fn measure_text(
        &self,
        font: FontId,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
        align: TextAlign,
    ) -> TextMeasureOut {
        visit_layout_glyphs_styled(
            self.get(font),
            text,
            TextLayoutOptions {
                max_width,
                max_lines,
                ellipsis,
                align,
            },
            TextLayoutStyle {
                fallback: self.fallback(),
                ..TextLayoutStyle::default()
            },
            |_| {},
            |_| {},
        )
    }

    #[inline]
    fn measure_text_styled(
        &self,
        font: FontId,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
        align: TextAlign,
        wrap: WrapMode,
        letter_spacing: i8,
        line_spacing: i8,
    ) -> TextMeasureOut {
        visit_layout_glyphs_styled(
            self.get(font),
            text,
            TextLayoutOptions {
                max_width,
                max_lines,
                ellipsis,
                align,
            },
            TextLayoutStyle {
                wrap,
                letter_spacing,
                line_spacing,
                vis_y: None,
                fallback: self.fallback(),
            },
            |_| {},
            |_| {},
        )
    }
}

#[inline]
fn adv_q2_provider<P: FontProvider>(
    provider: &P,
    slot: FontId,
    ch: char,
    prev: Option<char>,
    res: &ResolvedGlyph,
    letter_spacing: i8,
) -> Result<i64, P::Error> {
    let base = if res.info.advance_q2 {
        i64::from(res.metrics.advance)
    } else {
        i64::from(res.metrics.advance) * 4
    };
    let kern = if let Some(p) = prev
        && !res.from_fallback
        && provider.has_kerning(slot)
    {
        let k = i64::from(provider.kerning(slot, p, ch)?);
        if res.info.advance_q2 { k } else { k * 4 }
    } else {
        0
    };
    let ls = if prev.is_some() {
        i64::from(letter_spacing) * 4
    } else {
        0
    };
    Ok((base + kern + ls).max(0))
}

#[inline]
fn emit_glyph_provider(
    res: &ResolvedGlyph,
    pen_x_q2: i64,
    baseline: i64,
    line: u16,
    ink: &mut Rect,
    on_glyph: &mut impl FnMut(PositionedGlyphMetrics),
) {
    let (px, phase) = if res.info.phases <= 1 {
        if res.info.advance_q2 {
            ((pen_x_q2 + 2) >> 2, 0)
        } else {
            (pen_x_q2 >> 2, 0)
        }
    } else {
        let sub = (pen_x_q2 & 3) as u8;
        let phase = (sub * res.info.phases) >> 2;
        (pen_x_q2 >> 2, phase.min(res.info.phases - 1))
    };
    let gx = clamp_i64_to_i32(px + i64::from(res.metrics.bearing_x));
    let gy = clamp_i64_to_i32(baseline - i64::from(res.metrics.bearing_y));
    let g_rect = Rect::new(
        gx,
        gy,
        u32::from(res.metrics.width),
        u32::from(res.metrics.height),
    );
    *ink = ink.union(g_rect);
    on_glyph(PositionedGlyphMetrics {
        x: gx,
        y: gy,
        line,
        phase,
        format: res.info.format,
        encoding: res.info.encoding,
        from_fallback: res.from_fallback,
        metrics: res.metrics,
    });
}

fn will_overflow_after_provider<P: FontProvider>(
    provider: &P,
    slot: FontId,
    rest: &str,
    max_width: u32,
    letter_spacing: i8,
) -> Result<bool, P::Error> {
    let mut pen_x_q2 = 0u64;
    let max_width_q2 = u64::from(max_width) * 4;
    let mut has_char = false;
    let mut idx = 0usize;
    let mut prev: Option<char> = None;
    let bytes = rest.as_bytes();
    while idx < bytes.len() {
        let ch = next_char(rest, &mut idx);
        if ch == '\r' {
            if idx < bytes.len() && bytes[idx] == b'\n' {
                idx += 1;
            }
            return Ok(idx <= bytes.len());
        }
        if ch == '\n' {
            return Ok(true);
        }
        let res = provider.resolve_char(slot, ch)?;
        let adv = adv_q2_provider(provider, slot, ch, prev, &res, letter_spacing)? as u64;
        if has_char && pen_x_q2.saturating_add(adv) > max_width_q2 {
            return Ok(true);
        }
        has_char = true;
        prev = Some(ch);
        pen_x_q2 = pen_x_q2.saturating_add(adv);
        if pen_x_q2 > max_width_q2 && idx < bytes.len() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Lays out text against any unified [`FontProvider`] (XIP, non-XIP streaming, or hybrid),
/// returning glyph coordinates and metrics without requiring upfront bitmap reads.
pub fn visit_layout_glyphs_provider<P: FontProvider>(
    provider: &P,
    slot: FontId,
    text: &str,
    opts: TextLayoutOptions,
    style: ProviderLayoutStyle,
    mut on_glyph: impl FnMut(PositionedGlyphMetrics),
    mut on_line: impl FnMut(LineDecor),
) -> Result<TextMeasureOut, P::Error> {
    if text.is_empty() || opts.max_width == 0 {
        return Ok(TextMeasureOut {
            size: Size::new(0, 0),
            ink: Rect::ZERO,
            lines: 0,
        });
    }

    let lh = provider.line_height(slot);
    let line_step = (lh as i64 + i64::from(style.line_spacing)).max(1);
    let font_ascent = provider.ascent(slot);
    let ascent = if font_ascent > 0 {
        i64::from(font_ascent)
    } else {
        0
    };
    let max_lines = if opts.max_lines == 0 {
        u16::MAX
    } else {
        u16::from(opts.max_lines)
    };
    let max_width_q2 = u64::from(opts.max_width) * 4;
    let bytes = text.as_bytes();

    if max_lines == 1
        && opts.align == TextAlign::Start
        && !opts.ellipsis
        && style.wrap == WrapMode::Char
    {
        if let Some((vy0, vy1)) = style.vis_y
            && (0 >= i64::from(vy1) || line_step <= i64::from(vy0))
        {
            return Ok(TextMeasureOut {
                size: Size::new(0, line_step as u32),
                ink: Rect::ZERO,
                lines: 1,
            });
        }
        let baseline = ascent;
        let mut pen_x_q2 = 0i64;
        let mut ink = Rect::ZERO;
        let mut prev: Option<char> = None;
        let mut idx = 0usize;
        while idx < bytes.len() {
            let ch = next_char(text, &mut idx);
            if ch == '\r' || ch == '\n' {
                break;
            }
            let res = provider.resolve_char(slot, ch)?;
            let adv = adv_q2_provider(provider, slot, ch, prev, &res, style.letter_spacing)? as u64;
            if prev.is_some() && (pen_x_q2 as u64).saturating_add(adv) > max_width_q2 {
                break;
            }
            let g_adv = if res.info.advance_q2 {
                i64::from(res.metrics.advance)
            } else {
                i64::from(res.metrics.advance) * 4
            };
            if prev.is_some() && (provider.has_kerning(slot) || style.letter_spacing != 0) {
                pen_x_q2 += adv as i64 - g_adv;
            }
            emit_glyph_provider(&res, pen_x_q2, baseline, 0, &mut ink, &mut on_glyph);
            pen_x_q2 += g_adv;
            prev = Some(ch);
        }
        let end_px = ((pen_x_q2 + 2) >> 2).min(i64::from(opts.max_width)) as u32;
        on_line(LineDecor {
            line: 0,
            x_start: 0,
            x_end: end_px as i32,
            baseline_y: baseline as i32,
            ascent: font_ascent,
        });
        return Ok(TextMeasureOut {
            size: Size::new(end_px, line_step as u32),
            ink,
            lines: 1,
        });
    }

    let mut line_idx = 0u16;
    let mut max_line_w = 0u32;
    let mut ink = Rect::ZERO;
    let mut byte_idx = 0usize;

    loop {
        if line_idx >= max_lines {
            break;
        }
        let line_top = i64::from(line_idx) * line_step;
        if let Some((_, vy1)) = style.vis_y
            && line_top >= i64::from(vy1)
        {
            break;
        }
        let skip_emit = style
            .vis_y
            .is_some_and(|(vy0, _)| line_top + line_step <= i64::from(vy0));
        let baseline = line_top + ascent;
        let is_last_allowed_line = line_idx + 1 == max_lines;

        if is_last_allowed_line
            && opts.ellipsis
            && will_overflow_after_provider(
                provider,
                slot,
                &text[byte_idx..],
                opts.max_width,
                style.letter_spacing,
            )?
        {
            let ell_res = provider.resolve_char(slot, '…')?;
            let ell_adv_q2 = if ell_res.info.advance_q2 {
                u64::from(ell_res.metrics.advance)
            } else {
                u64::from(ell_res.metrics.advance) * 4
            };
            if ell_adv_q2 > max_width_q2 {
                line_idx += 1;
                break;
            }
            let budget_q2 = max_width_q2 - ell_adv_q2;
            let span_start = byte_idx;
            let mut span_end = byte_idx;
            let mut adv_sum_q2 = 0u64;
            let mut prev: Option<char> = None;
            while span_end < bytes.len() {
                let saved = span_end;
                let ch = next_char(text, &mut span_end);
                if ch == '\n' || ch == '\r' {
                    span_end = saved;
                    break;
                }
                let res = provider.resolve_char(slot, ch)?;
                let adv =
                    adv_q2_provider(provider, slot, ch, prev, &res, style.letter_spacing)? as u64;
                if adv_sum_q2.saturating_add(adv) > budget_q2 {
                    span_end = saved;
                    break;
                }
                adv_sum_q2 += adv;
                prev = Some(ch);
            }
            let total_adv_q2 = adv_sum_q2 + ell_adv_q2;
            let total_px = ((total_adv_q2 + 2) >> 2).min(u64::from(opts.max_width)) as u32;
            if !skip_emit {
                let slack_px = opts.max_width.saturating_sub(total_px);
                let align_x: i64 = match opts.align {
                    TextAlign::Start => 0,
                    TextAlign::Center => i64::from(slack_px / 2),
                    TextAlign::End => i64::from(slack_px),
                };
                let mut pen_x_q2 = align_x * 4;
                let mut cur = span_start;
                let mut prev_emit: Option<char> = None;
                while cur < span_end {
                    let ch = next_char(text, &mut cur);
                    let res = provider.resolve_char(slot, ch)?;
                    let adv =
                        adv_q2_provider(provider, slot, ch, prev_emit, &res, style.letter_spacing)?;
                    let g_adv = if res.info.advance_q2 {
                        i64::from(res.metrics.advance)
                    } else {
                        i64::from(res.metrics.advance) * 4
                    };
                    pen_x_q2 += adv - g_adv;
                    emit_glyph_provider(
                        &res,
                        pen_x_q2,
                        baseline,
                        line_idx,
                        &mut ink,
                        &mut on_glyph,
                    );
                    pen_x_q2 += g_adv;
                    prev_emit = Some(ch);
                }
                emit_glyph_provider(
                    &ell_res,
                    pen_x_q2,
                    baseline,
                    line_idx,
                    &mut ink,
                    &mut on_glyph,
                );
                on_line(LineDecor {
                    line: line_idx,
                    x_start: align_x as i32,
                    x_end: (align_x + i64::from(total_px)) as i32,
                    baseline_y: baseline as i32,
                    ascent: font_ascent,
                });
            }
            max_line_w = max_line_w.max(total_px);
            line_idx += 1;
            break;
        }

        let span_start = byte_idx;
        let mut span_end = byte_idx;
        let mut line_adv_q2 = 0u64;
        let mut line_has_char = false;
        let mut ended_by_newline = false;
        let mut prev: Option<char> = None;
        let mut last_break: Option<(usize, usize, u64)> = None;

        while byte_idx < bytes.len() {
            let saved_idx = byte_idx;
            let ch = next_char(text, &mut byte_idx);

            if ch == '\r' {
                if byte_idx < bytes.len() && bytes[byte_idx] == b'\n' {
                    byte_idx += 1;
                }
                ended_by_newline = true;
                break;
            }
            if ch == '\n' {
                ended_by_newline = true;
                break;
            }

            let res = provider.resolve_char(slot, ch)?;
            let adv = adv_q2_provider(provider, slot, ch, prev, &res, style.letter_spacing)? as u64;

            if style.wrap == WrapMode::Word && line_has_char {
                if ch == ' ' || ch == '\t' {
                    let mut next_after = byte_idx;
                    while next_after < bytes.len() && matches!(bytes[next_after], b' ' | b'\t') {
                        next_after += 1;
                    }
                    last_break = Some((next_after, span_end, line_adv_q2));
                } else if is_cjk_char(ch)
                    && !no_line_start(ch)
                    && prev.is_some_and(|p| !no_line_end(p) && p != ' ' && p != '\t')
                {
                    last_break = Some((saved_idx, span_end, line_adv_q2));
                }
            }

            if line_has_char && line_adv_q2.saturating_add(adv) > max_width_q2 {
                if style.wrap == WrapMode::Word
                    && let Some((brk_next, brk_end, brk_adv)) = last_break
                    && brk_end > span_start
                {
                    byte_idx = brk_next;
                    span_end = brk_end;
                    line_adv_q2 = brk_adv;
                } else {
                    byte_idx = saved_idx;
                }
                break;
            }

            line_has_char = true;
            line_adv_q2 = line_adv_q2.saturating_add(adv);
            span_end = byte_idx;
            prev = Some(ch);

            if style.wrap == WrapMode::Word && matches!(ch, '-' | '/') {
                last_break = Some((byte_idx, span_end, line_adv_q2));
            }
        }

        let line_px = ((line_adv_q2 + 2) >> 2).min(u64::from(opts.max_width)) as u32;
        if !skip_emit {
            let slack_px = opts.max_width.saturating_sub(line_px);
            let align_x: i64 = match opts.align {
                TextAlign::Start => 0,
                TextAlign::Center => i64::from(slack_px / 2),
                TextAlign::End => i64::from(slack_px),
            };
            let mut pen_x_q2 = align_x * 4;
            let mut cur = span_start;
            let mut prev_emit: Option<char> = None;
            while cur < span_end {
                let ch = next_char(text, &mut cur);
                let res = provider.resolve_char(slot, ch)?;
                let adv =
                    adv_q2_provider(provider, slot, ch, prev_emit, &res, style.letter_spacing)?;
                let g_adv = if res.info.advance_q2 {
                    i64::from(res.metrics.advance)
                } else {
                    i64::from(res.metrics.advance) * 4
                };
                pen_x_q2 += adv - g_adv;
                emit_glyph_provider(&res, pen_x_q2, baseline, line_idx, &mut ink, &mut on_glyph);
                pen_x_q2 += g_adv;
                prev_emit = Some(ch);
            }
            if line_px > 0 {
                on_line(LineDecor {
                    line: line_idx,
                    x_start: align_x as i32,
                    x_end: (align_x + i64::from(line_px)) as i32,
                    baseline_y: baseline as i32,
                    ascent: font_ascent,
                });
            }
        }

        max_line_w = max_line_w.max(line_px);
        line_idx += 1;

        if byte_idx >= bytes.len() {
            if ended_by_newline && line_idx < max_lines {
                line_idx += 1;
            }
            break;
        }
    }

    let total_h = (i64::from(line_idx) * line_step).max(0) as u32;
    Ok(TextMeasureOut {
        size: Size::new(max_line_w, total_h),
        ink,
        lines: line_idx,
    })
}
