//! A single-line text field with immutable frame images and a bounded text budget.
use crate::{
    Editor, Glyph, ShapingFont,
    glyphs::{GlyphError, raster_glyph},
    shape_paragraph,
};
use alloc::{string::String, sync::Arc, vec, vec::Vec};
use liuli_core::{
    bitmap::OwnedBitmap,
    capacity::BuildError,
    geometry::{IntoStorage, Rect, Rgb565},
    input::TextInput,
    paint::WidgetCanvas,
};
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
pub struct FieldStyle {
    pub width: u16,
    pub height: u16,
    pub px: u16,
    pub foreground: Rgb565,
    pub background: Rgb565,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldError {
    Size,
    TextBudget,
    Glyph(GlyphError),
    NoFonts,
    Multiline,
}
pub struct TextField<'a> {
    fonts: Vec<ShapingFont<'a>>,
    editor: Editor,
    style: FieldStyle,
    max_bytes: usize,
    image: Arc<OwnedBitmap>,
    revision: u16,
}
impl<'a> TextField<'a> {
    pub fn new(
        text: String,
        fonts: Vec<ShapingFont<'a>>,
        style: FieldStyle,
        max_bytes: usize,
    ) -> Result<Self, FieldError> {
        if fonts.is_empty() {
            return Err(FieldError::NoFonts);
        }
        if text.len() > max_bytes {
            return Err(FieldError::TextBudget);
        }
        if text.contains(['\r', '\n']) {
            return Err(FieldError::Multiline);
        }
        let editor = Editor::new(text);
        let image = render(&editor, &fonts, style)?;
        Ok(Self {
            fonts,
            editor,
            style,
            max_bytes,
            image,
            revision: 0,
        })
    }
    pub fn editor(&self) -> &Editor {
        &self.editor
    }
    pub fn revision(&self) -> u16 {
        self.revision
    }
    pub fn set_colors(&mut self, foreground: Rgb565, background: Rgb565) -> Result<(), FieldError> {
        if self.style.foreground == foreground && self.style.background == background {
            return Ok(());
        }
        let next_style = FieldStyle {
            foreground,
            background,
            ..self.style
        };
        let image = render(&self.editor, &self.fonts, next_style)?;
        self.style = next_style;
        self.image = image;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
    pub fn paint(&self, canvas: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
        canvas.surface(
            Rect::new(0, 0, self.style.width.into(), self.style.height.into()),
            self.image.clone(),
            0,
            15,
        )
    }
    pub fn apply(&mut self, input: TextInput<'_>) -> Result<(), FieldError> {
        let mut next = self.editor.clone();
        match input {
            TextInput::Compose(s) | TextInput::Commit(s) => {
                if s.contains(['\r', '\n']) {
                    return Err(FieldError::Multiline);
                }
                if next.text().len().saturating_add(s.len()) > self.max_bytes {
                    return Err(FieldError::TextBudget);
                }
                if matches!(input, TextInput::Compose(_)) {
                    next.compose(s);
                } else {
                    next.commit(s);
                }
            }
            TextInput::Cancel => next.cancel(),
            TextInput::Backspace => next.backspace(),
            TextInput::Left => next.move_visual(false),
            TextInput::Right => next.move_visual(true),
        }
        let image = render(&next, &self.fonts, self.style)?;
        self.editor = next;
        self.image = image;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}
fn render(
    editor: &Editor,
    fonts: &[ShapingFont<'_>],
    style: FieldStyle,
) -> Result<Arc<OwnedBitmap>, FieldError> {
    let (w, h) = (usize::from(style.width), usize::from(style.height));
    if w == 0
        || h == 0
        || style.px == 0
        || w.checked_mul(h).is_none_or(|pixels| pixels > 1024 * 1024)
    {
        return Err(FieldError::Size);
    }
    let mut text = String::from(editor.text());
    let composition = editor.preedit().unwrap_or("");
    text.insert_str(editor.cursor(), composition);
    let cursor = editor.cursor() + composition.len();
    let mut glyphs: Vec<(usize, Glyph, i32)> = Vec::new();
    let mut pen = 0i32;
    shape_paragraph(&text, fonts, i32::from(style.px), |f, g| {
        let x = pen;
        pen = pen.saturating_add(g.advance);
        glyphs.push((f, g, x));
    });
    let bidi = BidiInfo::new(&text, None);
    let mut clusters: Vec<_> = glyphs.iter().map(|(_, g, _)| g.cluster).collect();
    clusters.push(text.len());
    clusters.sort_unstable();
    clusters.dedup();
    let mut caret = 0;
    for span in clusters.windows(2) {
        if cursor < span[0] || cursor > span[1] {
            continue;
        }
        let mut run = glyphs.iter().filter(|(_, g, _)| g.cluster == span[0]);
        let Some((_, first, x)) = run.next() else {
            continue;
        };
        let advance = run.fold(first.advance, |a, (_, g, _)| a.saturating_add(g.advance));
        let count = text[span[0]..span[1]].graphemes(true).count().max(1);
        // A shaper may join multiple graphemes into one cluster.
        let elapsed = text[span[0]..cursor].graphemes(true).count().min(count);
        let n = if bidi.levels[span[0]].is_rtl() {
            count - elapsed
        } else {
            elapsed
        };
        caret = x.saturating_add((i64::from(advance) * n as i64 / count as i64) as i32);
        break;
    }
    let scroll = (caret - (w as i32 - 2)).max(0);
    let bg = style.background.into_storage();
    let fg = style.foreground.into_storage();
    let mut pixels = vec![bg; w * h];
    let baseline = (i32::from(style.height) + i32::from(style.px)) / 2 - 2;
    for (f, g, x) in glyphs {
        raster_glyph(
            &fonts[f],
            g.id,
            style.px,
            (
                x.saturating_add(g.x_offset).saturating_sub(scroll),
                baseline.saturating_sub(g.y_offset),
            ),
            65536,
            |x, y, a| {
                if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                    let dst = &mut pixels[y as usize * w + x as usize];
                    let background = Rgb565::from(liuli_core::geometry::RawU16::new(*dst));
                    *dst =
                        liuli_text::blend_rgb565_a4(style.foreground, background, a).into_storage();
                }
            },
        )
        .map_err(FieldError::Glyph)?;
    }
    let col = (caret - scroll).clamp(0, w as i32 - 1) as usize;
    for row in pixels.chunks_exact_mut(w) {
        row[col] = fg;
    }
    Ok(Arc::new(
        OwnedBitmap::new(style.width, style.height, pixels.into_boxed_slice())
            .ok_or(FieldError::Size)?,
    ))
}
