use liuli_core::geometry::{Rect, Size};
use liuli_core::layout::{TextMeasureOut, TextMeasurer};

use crate::font::{GlyphBitmap, Lf4Font};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TextLayoutOptions {
    pub max_width: u32,
    pub max_lines: u8,
    pub ellipsis: bool,
}

#[derive(Copy, Clone)]
pub struct PositionedGlyph<'a> {
    pub x: i32,
    pub y: i32,
    pub line: u16,
    pub glyph: GlyphBitmap<'a>,
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
    let line_step = lh;
    let ascent = font.ascent() as i32;
    let max_lines = if opts.max_lines == 0 {
        u16::MAX
    } else {
        opts.max_lines as u16
    };

    let mut line_idx = 0u16;
    let mut max_line_w = 0u32;
    let mut ink = Rect::ZERO;
    let mut byte_idx = 0usize;
    let bytes = text.as_bytes();

    loop {
        if line_idx >= max_lines {
            break;
        }
        let line_top = (line_idx as i32) * (line_step as i32);
        let baseline = line_top + ascent;

        let is_last_allowed_line = line_idx + 1 == max_lines;

        if is_last_allowed_line
            && opts.ellipsis
            && will_overflow_after(font, &text[byte_idx..], opts.max_width)
        {
            // Ellipsis rollback on last allowed line
            let (ell_glyph, _) = font.lookup_or_fallback('…');
            let ell_adv = ell_glyph.metrics.advance as u32;
            if ell_adv > opts.max_width {
                // Ellipsis itself doesn't fit: output empty ink for last line
                line_idx += 1;
                break;
            }
            let budget = opts.max_width - ell_adv;
            let mut pen_x = 0i32;
            let mut cur = byte_idx;
            while cur < bytes.len() {
                let ch = next_char(text, &mut cur);
                if ch == '\n' || ch == '\r' {
                    break;
                }
                let (g, _) = font.lookup_or_fallback(ch);
                let adv = g.metrics.advance as u32;
                if (pen_x as u32).saturating_add(adv) > budget {
                    break;
                }
                let gx = pen_x + g.metrics.bearing_x as i32;
                let gy = baseline - g.metrics.bearing_y as i32;
                let g_rect = Rect::new(gx, gy, g.metrics.width as u32, g.metrics.height as u32);
                ink = ink.union(g_rect);
                on_glyph(PositionedGlyph {
                    x: gx,
                    y: gy,
                    line: line_idx,
                    glyph: g,
                });
                pen_x += adv as i32;
            }
            let ex = pen_x + ell_glyph.metrics.bearing_x as i32;
            let ey = baseline - ell_glyph.metrics.bearing_y as i32;
            let e_rect = Rect::new(
                ex,
                ey,
                ell_glyph.metrics.width as u32,
                ell_glyph.metrics.height as u32,
            );
            ink = ink.union(e_rect);
            on_glyph(PositionedGlyph {
                x: ex,
                y: ey,
                line: line_idx,
                glyph: ell_glyph,
            });
            pen_x += ell_adv as i32;
            max_line_w = max_line_w.max((pen_x as u32).min(opts.max_width));
            line_idx += 1;
            break;
        }

        // Normal greedy line layout
        let mut pen_x = 0i32;
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
            let adv = g.metrics.advance as u32;
            if line_has_char && (pen_x as u32).saturating_add(adv) > opts.max_width {
                // Wrap before this scalar
                byte_idx = saved_idx;
                break;
            }

            line_has_char = true;
            let gx = pen_x + g.metrics.bearing_x as i32;
            let gy = baseline - g.metrics.bearing_y as i32;
            let g_rect = Rect::new(gx, gy, g.metrics.width as u32, g.metrics.height as u32);
            ink = ink.union(g_rect);
            on_glyph(PositionedGlyph {
                x: gx,
                y: gy,
                line: line_idx,
                glyph: g,
            });
            pen_x += adv as i32;
        }

        max_line_w = max_line_w.max((pen_x as u32).min(opts.max_width));
        line_idx += 1;

        if byte_idx >= bytes.len() {
            // Trailing explicit newline produces one final empty line if within max_lines
            if ended_by_newline && line_idx < max_lines {
                line_idx += 1;
            }
            break;
        }
    }

    let total_h = (line_idx as u32).saturating_mul(line_step);
    TextMeasureOut {
        size: Size::new(max_line_w, total_h),
        ink,
        lines: line_idx,
    }
}

fn will_overflow_after(font: &Lf4Font<'_>, rest: &str, max_width: u32) -> bool {
    let mut pen_x = 0u32;
    let mut has_char = false;
    let mut idx = 0usize;
    let bytes = rest.as_bytes();
    while idx < bytes.len() {
        let ch = next_char(rest, &mut idx);
        if ch == '\r' {
            if idx < bytes.len() && bytes[idx] == b'\n' {
                idx += 1;
            }
            return idx <= bytes.len(); // explicit newline means another line follows (even if empty, trailing newline counts as a 2nd line)
        }
        if ch == '\n' {
            return true;
        }
        let (g, _) = font.lookup_or_fallback(ch);
        let adv = g.metrics.advance as u32;
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
    fn line_height(&self) -> u32 {
        Lf4Font::line_height(self)
    }

    fn measure_text(
        &self,
        text: &str,
        max_width: u32,
        max_lines: u8,
        ellipsis: bool,
    ) -> TextMeasureOut {
        visit_layout_glyphs(
            self,
            text,
            TextLayoutOptions {
                max_width,
                max_lines,
                ellipsis,
            },
            |_| {},
        )
    }
}
