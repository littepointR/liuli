use alloc::{string::String, vec::Vec};
use core::ops::Range;
use harfrust::{Direction, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use unicode_bidi::BidiInfo;
use unicode_script::{Script, UnicodeScript};
use unicode_segmentation::UnicodeSegmentation;

/// Committed UTF-8 text and independent IME preedit. Cursor positions are byte offsets.
#[derive(Default, Clone, Debug)]
pub struct Editor {
    text: String,
    cursor: usize,
    preedit: Option<String>,
    visual_stop: Option<usize>,
}
impl Editor {
    pub fn new(text: String) -> Self {
        let cursor = text.len();
        Self {
            text,
            cursor,
            preedit: None,
            visual_stop: None,
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn preedit(&self) -> Option<&str> {
        self.preedit.as_deref()
    }
    pub fn set_cursor(&mut self, pos: usize) -> bool {
        if pos != self.text.len() && !self.text.grapheme_indices(true).any(|(i, _)| i == pos) {
            return false;
        }
        self.visual_stop = None;
        self.cursor = pos;
        self.preedit = None;
        true
    }
    pub fn previous(&mut self) {
        self.visual_stop = None;
        self.preedit = None;
        self.cursor = self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i);
    }
    pub fn next(&mut self) {
        self.visual_stop = None;
        self.preedit = None;
        if let Some(g) = self.text[self.cursor..].graphemes(true).next() {
            self.cursor += g.len();
        }
    }
    /// Move one grapheme boundary in on-screen order, retaining the caret's affinity at run edges.
    pub fn move_visual(&mut self, right: bool) {
        let stops = visual_carets(&self.text);
        let Some(i) = self
            .visual_stop
            .filter(|&i| stops.get(i) == Some(&self.cursor))
            .or_else(|| stops.iter().position(|&pos| pos == self.cursor))
        else {
            return;
        };
        let next = if right {
            (i + 1).min(stops.len() - 1)
        } else {
            i.saturating_sub(1)
        };
        self.visual_stop = Some(next);
        self.cursor = stops[next];
        self.preedit = None;
    }
    pub fn backspace(&mut self) {
        if self.preedit.take().is_some() {
            return;
        }
        let end = self.cursor;
        self.previous();
        self.text.replace_range(self.cursor..end, "");
        self.snap_forward();
    }
    pub fn compose(&mut self, s: &str) {
        self.preedit = Some(String::from(s));
    }
    pub fn cancel(&mut self) {
        self.preedit = None;
    }
    pub fn commit(&mut self, s: &str) {
        self.visual_stop = None;
        self.preedit = None;
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
        self.snap_forward();
    }
    fn snap_forward(&mut self) {
        self.cursor = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|&i| i >= self.cursor)
            .unwrap_or(self.text.len());
    }
}
/// UTF-8 caret offsets in visual order, with an entry for each edge of a directional run.
pub fn visual_carets(text: &str) -> Vec<usize> {
    let bidi = BidiInfo::new(text, None);
    let boundaries: Vec<_> = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(core::iter::once(text.len()))
        .collect();
    let mut result = Vec::new();
    for p in &bidi.paragraphs {
        let (levels, runs) = bidi.visual_runs(p, p.range.clone());
        for run in runs {
            if run.is_empty() {
                continue;
            }
            let lo = boundaries.partition_point(|&i| i < run.start);
            let hi = boundaries.partition_point(|&i| i <= run.end);
            let edges = &boundaries[lo..hi];
            if levels[run.start].is_rtl() {
                result.extend(edges.iter().rev().copied());
            } else {
                result.extend_from_slice(edges);
            }
        }
    }
    if result.is_empty() {
        result.push(0);
    }
    result.dedup();
    result
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    pub id: u16,
    pub cluster: usize,
    pub advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
}
pub struct ShapingFont<'a> {
    font: FontRef<'a>,
    data: ShaperData,
    pub(crate) cmap: ttf_parser::Face<'a>,
}
impl<'a> ShapingFont<'a> {
    pub fn from_bytes(bytes: &'a [u8], index: u32) -> Option<Self> {
        let font = FontRef::from_index(bytes, index).ok()?;
        let data = ShaperData::new(&font);
        Some(Self {
            font,
            data,
            cmap: ttf_parser::Face::parse(bytes, index).ok()?,
        })
    }
}
/// Shape one paragraph in visual order. Positions are in caller-selected units per em.
/// Each callback receives the font index needed for glyph-ID raster lookup.
pub fn shape_paragraph(
    text: &str,
    fonts: &[ShapingFont<'_>],
    scale: i32,
    mut emit: impl FnMut(usize, Glyph),
) {
    let Some(first) = fonts.first() else { return };
    let bidi = BidiInfo::new(text, None);
    for paragraph in &bidi.paragraphs {
        let (levels, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
        for range in runs {
            if range.is_empty() {
                continue;
            }
            let rtl = levels[range.start].is_rtl();
            let mut spans: Vec<(Range<usize>, Script, usize)> = Vec::new();
            let mut previous = Script::Common;
            for (offset, cluster) in text[range.clone()].grapheme_indices(true) {
                let script = cluster
                    .chars()
                    .map(|c| c.script())
                    .find(|s| !matches!(s, Script::Common | Script::Inherited))
                    .unwrap_or(previous);
                previous = script;
                let start = range.start + offset;
                let font = fonts
                    .iter()
                    .position(|f| {
                        cluster
                            .chars()
                            .filter(|c| !matches!(*c, '\u{200C}' | '\u{200D}' | '\u{FE0F}'))
                            .all(|c| f.cmap.glyph_index(c).is_some())
                    })
                    .unwrap_or(0);
                if let Some((span, last_script, last_font)) = spans.last_mut()
                    && *last_font == font
                    && (*last_script == script || *last_script == Script::Common)
                {
                    *last_script = script;
                    span.end = start + cluster.len();
                } else {
                    spans.push((start..start + cluster.len(), script, font));
                }
            }
            if rtl {
                spans.reverse();
            }
            for (span, _, font_index) in spans {
                let f = fonts.get(font_index).unwrap_or(first);
                let shaper = f.data.shaper(&f.font).build();
                let mut buffer = UnicodeBuffer::new();
                buffer.push_str(&text[span.clone()]);
                buffer.set_direction(if rtl {
                    Direction::RightToLeft
                } else {
                    Direction::LeftToRight
                });
                buffer.guess_segment_properties();
                let shaped = shaper.shape(buffer, ShapeOptions::new().scale(Some(scale)));
                for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
                    emit(
                        font_index,
                        Glyph {
                            id: info.glyph_id as u16,
                            cluster: span.start + info.cluster as usize,
                            advance: pos.x_advance,
                            x_offset: pos.x_offset,
                            y_offset: pos.y_offset,
                        },
                    );
                }
            }
        }
    }
}
