use liuli_core::geometry::{Rect, Size};
use liuli_core::layout::{TextMeasureOut, TextMeasurer};
use liuli_core::view::{FontId, TextAlign};

use crate::font::{FontSet, GlyphBitmap, Lf4Font};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TextLayoutOptions {
    pub max_width: u32,
    pub max_lines: u8,
    pub ellipsis: bool,
    pub align: TextAlign,
}

#[derive(Copy, Clone)]
pub struct PositionedGlyph<'a> {
    pub x: i32,
    pub y: i32,
    pub line: u16,
    pub glyph: GlyphBitmap<'a>,
}

#[inline]
fn clamp_i64_to_i32(v: i64) -> i32 {
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

#[inline]
fn emit_glyph<'a>(
    g: GlyphBitmap<'a>,
    pen_x: i64,
    baseline: i64,
    line: u16,
    ink: &mut Rect,
    on_glyph: &mut impl FnMut(PositionedGlyph<'a>),
) {
    let gx = clamp_i64_to_i32(pen_x + g.metrics.bearing_x as i64);
    let gy = clamp_i64_to_i32(baseline - g.metrics.bearing_y as i64);
    let g_rect = Rect::new(gx, gy, g.metrics.width as u32, g.metrics.height as u32);
    *ink = ink.union(g_rect);
    on_glyph(PositionedGlyph {
        x: gx,
        y: gy,
        line,
        glyph: g,
    });
}

pub fn visit_layout_glyphs<'a>(
    font: &Lf4Font<'a>,
    text: &str,
    opts: TextLayoutOptions,
    mut on_glyph: impl FnMut(PositionedGlyph<'a>),
) -> TextMeasureOut {
    if text.is_empty() || opts.max_width == 0 {
        return TextMeasureOut {
            size: Size::new(0, 0),
            ink: Rect::ZERO,
            lines: 0,
        };
    }

    let lh = font.line_height();
    let line_step = lh as i64;
    let ascent = font.ascent() as i64;
    let max_lines = if opts.max_lines == 0 {
        u16::MAX
    } else {
        opts.max_lines as u16
    };
    let max_width = opts.max_width as u64;

    let mut line_idx = 0u16;
    let mut max_line_w = 0u32;
    let mut ink = Rect::ZERO;
    let mut byte_idx = 0usize;
    let bytes = text.as_bytes();

    loop {
        if line_idx >= max_lines {
            break;
        }
        let line_top = (line_idx as i64) * line_step;
        let baseline = line_top + ascent;
        let is_last_allowed_line = line_idx + 1 == max_lines;

        if is_last_allowed_line
            && opts.ellipsis
            && will_overflow_after(font, &text[byte_idx..], opts.max_width)
        {
            let (ell_glyph, _) = font.lookup_or_fallback('…');
            let ell_adv = ell_glyph.metrics.advance as u64;
            if ell_adv > max_width {
                line_idx += 1;
                break;
            }
            let budget = max_width - ell_adv;
            let span_start = byte_idx;
            let mut span_end = byte_idx;
            let mut adv_sum = 0u64;
            while span_end < bytes.len() {
                let saved = span_end;
                let ch = next_char(text, &mut span_end);
                if ch == '\n' || ch == '\r' {
                    span_end = saved;
                    break;
                }
                let (g, _) = font.lookup_or_fallback(ch);
                let adv = g.metrics.advance as u64;
                if adv_sum.saturating_add(adv) > budget {
                    span_end = saved;
                    break;
                }
                adv_sum += adv;
            }
            let total_adv = adv_sum + ell_adv;
            let slack = max_width.saturating_sub(total_adv);
            let align_x: i64 = match opts.align {
                TextAlign::Start => 0,
                TextAlign::Center => (slack / 2) as i64,
                TextAlign::End => slack as i64,
            };

            let mut pen_x = align_x;
            let mut cur = span_start;
            while cur < span_end {
                let ch = next_char(text, &mut cur);
                let (g, _) = font.lookup_or_fallback(ch);
                emit_glyph(g, pen_x, baseline, line_idx, &mut ink, &mut on_glyph);
                pen_x += g.metrics.advance as i64;
            }
            emit_glyph(
                ell_glyph,
                pen_x,
                baseline,
                line_idx,
                &mut ink,
                &mut on_glyph,
            );
            max_line_w = max_line_w.max(total_adv.min(max_width) as u32);
            line_idx += 1;
            break;
        }

        // Pass 1: measure greedy line advance and byte boundaries
        let span_start = byte_idx;
        let mut span_end = byte_idx;
        let mut line_adv = 0u64;
        let mut line_has_char = false;
        let mut ended_by_newline = false;

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

            let (g, _) = font.lookup_or_fallback(ch);
            let adv = g.metrics.advance as u64;
            if line_has_char && line_adv.saturating_add(adv) > max_width {
                byte_idx = saved_idx;
                break;
            }

            line_has_char = true;
            line_adv = line_adv.saturating_add(adv);
            span_end = byte_idx;
        }

        // Pass 2: emit aligned glyphs
        let slack = max_width.saturating_sub(line_adv);
        let align_x: i64 = match opts.align {
            TextAlign::Start => 0,
            TextAlign::Center => (slack / 2) as i64,
            TextAlign::End => slack as i64,
        };
        let mut pen_x = align_x;
        let mut cur = span_start;
        while cur < span_end {
            let ch = next_char(text, &mut cur);
            let (g, _) = font.lookup_or_fallback(ch);
            emit_glyph(g, pen_x, baseline, line_idx, &mut ink, &mut on_glyph);
            pen_x += g.metrics.advance as i64;
        }

        max_line_w = max_line_w.max(line_adv.min(max_width) as u32);
        line_idx += 1;

        if byte_idx >= bytes.len() {
            if ended_by_newline && line_idx < max_lines {
                line_idx += 1;
            }
            break;
        }
    }

    let total_h = (line_idx as u32).saturating_mul(lh);
    TextMeasureOut {
        size: Size::new(max_line_w, total_h),
        ink,
        lines: line_idx,
    }
}

fn will_overflow_after(font: &Lf4Font<'_>, rest: &str, max_width: u32) -> bool {
    let mut pen_x = 0u64;
    let max_width = max_width as u64;
    let mut has_char = false;
    let mut idx = 0usize;
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
        let (g, _) = font.lookup_or_fallback(ch);
        let adv = g.metrics.advance as u64;
        if has_char && pen_x.saturating_add(adv) > max_width {
            return true;
        }
        has_char = true;
        pen_x = pen_x.saturating_add(adv);
        if pen_x > max_width && idx < bytes.len() {
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
        visit_layout_glyphs(
            self.get(font),
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
}
