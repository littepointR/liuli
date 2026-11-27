mod selection;
use crate::capacity::{BuildError, MAX_COMMANDS, MAX_DEPTH, MAX_NODES};
use crate::geometry::{Point, Rect, Rgb565};
use crate::node::{RetainedNode, RetainedTree};
use crate::part_style::{IconPosition, VisualState, WidgetPartStyle};
use crate::time::{ANIM_SCALE, AnimPool, AnimProp};
use crate::view::{
    Align, BUTTON_ICON_GAP, BUTTON_ICON_SIZE, FontId, IconKind, Key, NodeKind, PaintMode, SlotLink,
    StoredProps, TextAlign, TextStyleFlags,
};
use selection::{paint_selection, paint_sliding_indicator, paint_tab, selection_progress};

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct TextId(pub u16);

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum PaintCmd {
    Empty = 0,
    Bitmap {
        angle: u8,
        alpha: u8,
        rect: Rect,
        clip: Rect,
        source: &'static crate::bitmap::Bitmap,
    } = 8,
    Surface {
        rect: Rect,
        clip: Rect,
        index: u16,
        angle: u8,
        alpha: u8,
    } = 9,
    Shadow {
        rect: Rect,
        clip: Rect,
        shadow: crate::effects::Shadow,
        projection: crate::effects::Projection,
        radius: u8,
    } = 10,
    Group {
        rect: Rect,
        clip: Rect,
        index: u8,
        end: u16,
    } = 11,
    FillRect {
        rect: Rect,
        clip: Rect,
        color: Rgb565,
    } = 1,
    StrokeRect {
        rect: Rect,
        clip: Rect,
        color: Rgb565,
        width: u8,
    } = 2,
    StrokeRoundedRect {
        rect: Rect,
        clip: Rect,
        color: Rgb565,
        width: u8,
        radius: u8,
    } = 12,
    FillRoundedRect {
        rect: Rect,
        clip: Rect,
        color: Rgb565,
        radius: u8,
        border_color: Option<Rgb565>,
        border_width: u8,
    } = 3,
    Text {
        pos: Point,
        clip: Rect,
        wrap_width: u32,
        text: TextId,
        color: Rgb565,
        font: FontId,
        align: TextAlign,
        max_lines: u8,
        ellipsis: bool,
        style: TextStyleFlags,
        letter_spacing: i8,
        line_spacing: i8,
    } = 4,
    RoundedOverlay {
        rect: Rect,
        clip: Rect,
        color: Rgb565,
        alpha: u8,
        radius: u8,
    } = 30,
    Overlay {
        rect: Rect,
        clip: Rect,
        color: Rgb565,
        alpha: u8,
    } = 5,
    Icon {
        rect: Rect,
        clip: Rect,
        kind: IconKind,
        color: Rgb565,
    } = 6,
    Sparkline {
        rect: Rect,
        clip: Rect,
        color: Rgb565,
        series: u8,
        min: i16,
        max: i16,
        stroke_width: u8,
    } = 7,
    Line {
        p0: Point,
        p1: Point,
        clip: Rect,
        color: Rgb565,
        width: u8,
        aa: bool,
    } = 13,
    Circle {
        center: Point,
        clip: Rect,
        radius: u16,
        fill: Option<Rgb565>,
        border_color: Option<Rgb565>,
        border_width: u8,
    } = 14,
    Arc {
        center: Point,
        clip: Rect,
        radius: u16,
        start_deg: i16,
        sweep_deg: i16,
        color: Rgb565,
        width: u8,
        cap_round: bool,
    } = 15,
    InlineText {
        pos: Point,
        clip: Rect,
        color: Rgb565,
        wrap_width: u16,
        font: FontId,
        align: TextAlign,
        len: u8,
        bytes: [u8; 12],
    } = 16,
    GradientRect {
        rect: Rect,
        clip: Rect,
        start: Rgb565,
        end: Rgb565,
        direction: u8,
        radius: u8,
    } = 17,
    StaticPath {
        paint: crate::path::PathPaint,
        color: Rgb565,
        rect: Rect,
        clip: Rect,
        path: &'static crate::path::Path<'static>,
    } = 19,
    PathRun {
        paint: crate::path::PathPaint,
        color: Rgb565,
        chunks: u16,
        rect: Rect,
        clip: Rect,
    } = 20,
    PathEdges {
        len: u8,
        edges: [crate::path::PathEdge; 4],
    } = 21,
    MaskGroup {
        slot: u8,
        radius: u8,
        end: u16,
        rect: Rect,
        clip: Rect,
        composition: &'static crate::composition::SurfaceComposition,
    } = 22,
    OutlineShadow {
        rule: crate::path::FillRule,
        shadow: crate::effects::Shadow,
        path: &'static crate::path::Path<'static>,
        rect: Rect,
        clip: Rect,
    } = 23,
    PathRing {
        rule: crate::path::FillRule,
        outer: i8,
        inner: i8,
        color: Rgb565,
        path: &'static crate::path::Path<'static>,
        rect: Rect,
        clip: Rect,
    } = 24,
    WideRound {
        rect: Rect,
        clip: Rect,
        radius: u32,
        color: Rgb565,
        width: u8,
    } = 25,
    WideRoundShadow {
        rect: Rect,
        clip: Rect,
        shadow: crate::effects::Shadow,
        radius: u32,
    } = 26,
    AlphaMask {
        color: Rgb565,
        alpha: u8,
        rect: Rect,
        clip: Rect,
        source: &'static crate::bitmap::Bitmap,
    } = 18,
}

pub const GRADIENT_HORIZONTAL: u8 = 0;
pub const GRADIENT_VERTICAL: u8 = 1;
pub const GRADIENT_DIAGONAL: u8 = 2;

const _: () = assert!(core::mem::size_of::<PaintCmd>() <= 48);

impl PaintCmd {
    pub const EMPTY: Self = Self::Empty;

    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub const fn text(
        pos: Point,
        clip: Rect,
        text: TextId,
        color: Rgb565,
        wrap_width: u32,
        max_lines: u8,
        ellipsis: bool,
        font: FontId,
        align: TextAlign,
    ) -> Self {
        Self::text_styled(
            pos,
            clip,
            text,
            color,
            wrap_width,
            max_lines,
            ellipsis,
            font,
            align,
            TextStyleFlags::DEFAULT,
            0,
            0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub const fn text_styled(
        pos: Point,
        clip: Rect,
        text: TextId,
        color: Rgb565,
        wrap_width: u32,
        max_lines: u8,
        ellipsis: bool,
        font: FontId,
        align: TextAlign,
        style: TextStyleFlags,
        letter_spacing: i8,
        line_spacing: i8,
    ) -> Self {
        Self::Text {
            pos,
            clip,
            wrap_width,
            text,
            color,
            font,
            align,
            max_lines,
            ellipsis,
            style,
            letter_spacing,
            line_spacing,
        }
    }

    #[inline]
    pub fn inline_text(
        pos: Point,
        clip: Rect,
        text: &str,
        color: Rgb565,
        wrap_width: u16,
        font: FontId,
        align: TextAlign,
    ) -> Option<Self> {
        let mut take = 0usize;
        for (idx, ch) in text.char_indices() {
            let end = idx + ch.len_utf8();
            if end <= 12 {
                take = end;
            } else {
                break;
            }
        }
        if take == 0 {
            return None;
        }
        let mut bytes = [0u8; 12];
        bytes[..take].copy_from_slice(&text.as_bytes()[..take]);
        Some(Self::InlineText {
            pos,
            clip,
            color,
            wrap_width,
            font,
            align,
            len: take as u8,
            bytes,
        })
    }

    pub fn bounds(&self) -> Rect {
        match *self {
            Self::Empty | Self::PathEdges { .. } => Rect::ZERO,
            Self::WideRoundShadow {
                rect, clip, shadow, ..
            }
            | Self::OutlineShadow {
                rect, clip, shadow, ..
            } => shadow.bounds(rect).intersect(clip),
            Self::Shadow {
                rect, clip, shadow, ..
            } => shadow.bounds(rect).intersect(clip),
            Self::PathRing {
                rect, clip, outer, ..
            } => rect.expand(outer.max(0) as u32).intersect(clip),
            Self::WideRound { rect, clip, .. }
            | Self::Group { rect, clip, .. }
            | Self::StaticPath { rect, clip, .. }
            | Self::PathRun { rect, clip, .. }
            | Self::MaskGroup { rect, clip, .. }
            | Self::Surface { rect, clip, .. }
            | Self::Bitmap { rect, clip, .. }
            | Self::FillRect { rect, clip, .. }
            | Self::StrokeRect { rect, clip, .. }
            | Self::StrokeRoundedRect { rect, clip, .. }
            | Self::FillRoundedRect { rect, clip, .. }
            | Self::RoundedOverlay { rect, clip, .. }
            | Self::Overlay { rect, clip, .. }
            | Self::Icon { rect, clip, .. }
            | Self::Sparkline { rect, clip, .. }
            | Self::GradientRect { rect, clip, .. }
            | Self::AlphaMask { rect, clip, .. } => rect.intersect(clip),
            Self::Text { clip, .. } | Self::InlineText { clip, .. } => clip,
            Self::Line {
                p0,
                p1,
                clip,
                width,
                ..
            } => {
                let pad = i32::from(width.max(1)) / 2 + 2;
                let x0 = p0.x.min(p1.x).saturating_sub(pad);
                let y0 = p0.y.min(p1.y).saturating_sub(pad);
                let x1 = p0.x.max(p1.x).saturating_add(pad + 1);
                let y1 = p0.y.max(p1.y).saturating_add(pad + 1);
                Rect::new(
                    x0,
                    y0,
                    x1.saturating_sub(x0).max(0) as u32,
                    y1.saturating_sub(y0).max(0) as u32,
                )
                .intersect(clip)
            }
            Self::Circle {
                center,
                clip,
                radius,
                border_width,
                ..
            } => {
                let r = i32::from(radius) + i32::from(border_width) + 2;
                let size = (r * 2 + 1).max(0) as u32;
                Rect::new(
                    center.x.saturating_sub(r),
                    center.y.saturating_sub(r),
                    size,
                    size,
                )
                .intersect(clip)
            }
            Self::Arc {
                center,
                clip,
                radius,
                width,
                ..
            } => {
                let r = i32::from(radius) + i32::from(width) + 2;
                let size = (r * 2 + 1).max(0) as u32;
                Rect::new(
                    center.x.saturating_sub(r),
                    center.y.saturating_sub(r),
                    size,
                    size,
                )
                .intersect(clip)
            }
        }
    }
}

pub struct CommandList {
    has_masks: bool,
    pub effect_error_key: Option<crate::view::Key>,
    #[cfg(feature = "layers")]
    pub layers: Option<alloc::vec::Vec<crate::layers::LayerCache>>,
    #[cfg(feature = "layers")]
    pub layer_budget: Option<usize>,
    #[cfg(feature = "layers")]
    pub(crate) used_layers: u8,

    #[cfg(feature = "owned-images")]
    surfaces: Option<alloc::vec::Vec<alloc::sync::Arc<crate::bitmap::OwnedBitmap>>>,
    pub cmds: [PaintCmd; MAX_COMMANDS],
    pub count: usize,
    pub root_bg: Rgb565,
    pub screen: Rect,
}

impl CommandList {
    pub const ZERO: Self = Self {
        has_masks: false,
        effect_error_key: None,
        #[cfg(feature = "owned-images")]
        surfaces: None,
        #[cfg(feature = "layers")]
        layers: None,
        #[cfg(feature = "layers")]
        layer_budget: None,
        #[cfg(feature = "layers")]
        used_layers: 0,
        cmds: [PaintCmd::EMPTY; MAX_COMMANDS],
        count: 0,
        root_bg: Rgb565::new(0, 0, 0),
        screen: Rect::ZERO,
    };

    pub const fn new() -> Self {
        Self::ZERO
    }

    pub fn has_mask_groups(&self) -> bool {
        self.has_masks
    }
    pub fn reset(&mut self) {
        self.has_masks = false;
        self.count = 0;
        self.effect_error_key = None;
        #[cfg(feature = "layers")]
        {
            self.used_layers = 0;
        }
        #[cfg(feature = "owned-images")]
        if let Some(v) = &mut self.surfaces {
            v.clear();
        }
    }

    pub fn image(&self, cmd: PaintCmd) -> Option<crate::bitmap::BitmapView<'_>> {
        match cmd {
            PaintCmd::Bitmap { source, .. } => Some(*source),
            #[cfg(feature = "owned-images")]
            PaintCmd::Surface { index, .. } => {
                Some(self.surfaces.as_ref()?.get(usize::from(index))?.view())
            }
            _ => None,
        }
    }
    /// Number of isolated subtrees that reused an earlier frame's pixel cache.
    pub fn reused_groups(&self) -> usize {
        #[cfg(feature = "layers")]
        {
            self.layers
                .iter()
                .flatten()
                .filter(|l| l.group.borrow().reused)
                .count()
        }
        #[cfg(not(feature = "layers"))]
        {
            0
        }
    }
    pub fn cached_layer_bytes(&self) -> usize {
        #[cfg(feature = "layers")]
        {
            self.layer_bytes()
        }
        #[cfg(not(feature = "layers"))]
        {
            0
        }
    }
    pub fn group_style(&self, index: u8) -> Option<(crate::effects::Layer, u8)> {
        #[cfg(feature = "layers")]
        {
            let l = self.layers.as_ref()?.get(usize::from(index))?;
            Some((l.style, l.radius))
        }
        #[cfg(not(feature = "layers"))]
        {
            let _ = index;
            None
        }
    }
    pub fn group_composition(
        &self,
        index: u8,
    ) -> Option<(
        Rect,
        Option<crate::composition::SurfaceComposition>,
        Option<crate::effects::Shadow>,
    )> {
        #[cfg(feature = "layers")]
        {
            let cache = self.layers.as_ref()?.get(usize::from(index))?;
            Some((cache.layout, cache.composition, cache.shadow))
        }
        #[cfg(not(feature = "layers"))]
        {
            let _ = index;
            None
        }
    }
    pub fn group_source(&self, index: u8) -> Option<Rect> {
        #[cfg(feature = "layers")]
        {
            Some(self.layers.as_ref()?.get(usize::from(index))?.source)
        }
        #[cfg(not(feature = "layers"))]
        {
            let _ = index;
            None
        }
    }
    pub fn group_backdrop_end(&self, index: u8) -> Option<usize> {
        #[cfg(feature = "layers")]
        {
            let l = self.layers.as_ref()?.get(usize::from(index))?;
            Some(l.backdrop_end)
        }
        #[cfg(not(feature = "layers"))]
        {
            let _ = index;
            None
        }
    }
    pub fn as_slice(&self) -> &[PaintCmd] {
        &self.cmds[..self.count]
    }

    pub fn push(&mut self, cmd: PaintCmd) -> Result<(), BuildError> {
        if self.count >= MAX_COMMANDS {
            return Err(BuildError::Commands);
        }
        self.has_masks |= matches!(cmd, PaintCmd::MaskGroup { .. });
        self.cmds[self.count] = cmd;
        self.count += 1;
        Ok(())
    }
}

impl Default for CommandList {
    fn default() -> Self {
        Self::new()
    }
}

pub trait SceneResources {
    fn geometry(&self, _slot: u8) -> Option<&'static crate::composition::WidgetGeometry> {
        None
    }
    fn text(&self, id: TextId) -> &str;
    fn sparkline(&self, series: u8) -> &[i16];
}

impl SceneResources for RetainedTree {
    fn geometry(&self, slot: u8) -> Option<&'static crate::composition::WidgetGeometry> {
        self.nodes
            .get(usize::from(slot))
            .and_then(|n| n.props.geometry)
    }
    #[inline]
    fn text(&self, id: TextId) -> &str {
        let idx = id.0 as usize;
        if idx < MAX_NODES && self.nodes[idx].occupied {
            self.text_str(self.nodes[idx].props.text())
        } else {
            ""
        }
    }

    #[inline]
    fn sparkline(&self, series: u8) -> &[i16] {
        self.sparklines.slice(series)
    }
}

pub struct SliceSceneResources<'a> {
    pub texts: &'a [&'a str],
    pub sparklines: &'a [&'a [i16]],
}

impl<'a> SliceSceneResources<'a> {
    pub const EMPTY: Self = Self {
        texts: &[],
        sparklines: &[],
    };
}

impl SceneResources for SliceSceneResources<'_> {
    #[inline]
    fn text(&self, id: TextId) -> &str {
        self.texts.get(id.0 as usize).copied().unwrap_or("")
    }

    #[inline]
    fn sparkline(&self, series: u8) -> &[i16] {
        self.sparklines.get(series as usize).copied().unwrap_or(&[])
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PaintFocusInfo {
    pub focused_slot: Option<u16>,
    pub ring_visible: bool,
    pub editing: bool,
}

#[derive(Copy, Clone)]
struct PaintDepthFrame {
    slot: u16,
    stage: u8,
    next_child: SlotLink,
}

/// Current interaction and animation state delivered to `App::paint_override` and `paint_custom`.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct PaintState {
    pub pressed: bool,
    pub focused: bool,
    pub editing: bool,
    pub checked: bool,
    pub enabled: bool,
    /// Current normalized state transition, or the legacy custom channel when unbound.
    pub anim_progress: i32,
}

/// Result returned by `App::paint_override`.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum PaintOverride {
    /// Proceed with the widget's default painting after the hook returns.
    #[default]
    Default = 0,
    /// Skip default widget painting (override handled everything).
    Handled = 1,
}

#[derive(Copy, Clone)]
struct BuiltinPaintCtx<'a> {
    composition: Option<&'static crate::composition::SurfaceComposition>,
    node: &'a RetainedNode,
    anims: &'a AnimPool,
    focus: PaintFocusInfo,
    screen: Rect,
    part: &'a WidgetPartStyle,
    slot: u16,
    root_bg: Rgb565,
    in_tab_bar: bool,
}

const MAX_CLIP_DEPTH: usize = 4;

/// Coordinates supplied to drawing methods are relative to the widget's `bounds`.
pub struct WidgetCanvas<'a> {
    bounds: Rect,
    clip_stack: [Rect; MAX_CLIP_DEPTH],
    clip_depth: u8,
    commands: &'a mut CommandList,
    pub pressed: bool,
    pub state: PaintState,
    slot: u16,
    ctx: BuiltinPaintCtx<'a>,
}

impl WidgetCanvas<'_> {
    #[inline]
    pub fn size(&self) -> crate::geometry::Size {
        self.bounds.size()
    }

    /// The main silhouette's bounding box and outline in widget-local coordinates.
    pub fn body_outline(&self) -> (Rect, crate::composition::Outline) {
        let outline =
            crate::composition::body_outline(self.ctx.node.props.geometry, self.ctx.composition);
        let size = self.size();
        (
            outline
                .bounds_and_radius(
                    Rect::new(0, 0, size.width, size.height),
                    self.ctx.node.props.radius,
                )
                .0,
            outline,
        )
    }
    /// The same merged part styles and state colors used by built-in painting.
    pub fn parts(&self) -> &WidgetPartStyle {
        self.ctx.part
    }

    pub fn foreground(&self) -> Rgb565 {
        resolve_state_fg(self.ctx.node, self.ctx.part)
    }

    pub fn background(&self) -> Option<Rgb565> {
        resolve_state_bg(self.ctx.node, self.ctx.part, self.ctx.root_bg)
    }

    /// Draw the themed selection part in widget-local coordinates. A composite widget
    /// animating one shared marker can supply its interpolated anchor as `control`.
    pub fn selection_indicator(&mut self, control: Rect, content: Rect) -> Result<(), BuildError> {
        let control = self.to_global(control);
        let content = self.to_global(content);
        let clip = self.current_clip();
        paint_selection(
            self.commands,
            self.ctx.part,
            control,
            content,
            clip,
            self.ctx.node.props.accent,
            self.state.checked,
            selection_progress(self.ctx.node, self.ctx.anims),
        )
    }

    /// Local x coordinates of the thumb center at the minimum and maximum values.
    pub fn slider_track_span(&self) -> (i32, i32) {
        let size = self.size();
        let part = self.parts();
        let (w, _) = part.slider_knob_size(size);
        let left = (w / 2) as i32;
        (left, left + (size.width - w) as i32)
    }

    #[inline]
    fn current_clip(&self) -> Rect {
        let idx = (self.clip_depth as usize)
            .saturating_sub(1)
            .min(MAX_CLIP_DEPTH - 1);
        self.clip_stack[idx]
    }

    /// Push a widget-local sub-clip rectangle onto the 4-level clip stack.
    pub fn push_clip(&mut self, rect: Rect) -> bool {
        let depth = self.clip_depth as usize;
        if depth >= MAX_CLIP_DEPTH {
            return false;
        }
        let next = rect
            .translate(self.bounds.x, self.bounds.y)
            .intersect(self.current_clip());
        self.clip_stack[depth] = next;
        self.clip_depth += 1;
        true
    }

    /// Pop the top sub-clip pushed by `push_clip`.
    pub fn pop_clip(&mut self) {
        if self.clip_depth > 1 {
            self.clip_depth -= 1;
        }
    }

    #[inline]
    fn to_global(&self, rect: Rect) -> Rect {
        Rect::new(
            self.bounds.x.saturating_add(rect.x),
            self.bounds.y.saturating_add(rect.y),
            rect.w,
            rect.h,
        )
    }

    #[inline]
    fn point_global(&self, p: Point) -> Point {
        Point::new(
            self.bounds.x.saturating_add(p.x),
            self.bounds.y.saturating_add(p.y),
        )
    }

    #[cfg(feature = "owned-images")]
    pub fn surface(
        &mut self,
        rect: Rect,
        source: alloc::sync::Arc<crate::bitmap::OwnedBitmap>,
        angle: u8,
        alpha: u8,
    ) -> Result<(), BuildError> {
        let clip = self.current_clip();
        let rect = rect.translate(self.bounds.x, self.bounds.y);
        if alpha == 0 || !rect.intersects(clip) {
            return Ok(());
        }
        if self.commands.count >= MAX_COMMANDS {
            return Err(BuildError::Commands);
        }
        let surfaces = self
            .commands
            .surfaces
            .get_or_insert_with(alloc::vec::Vec::new);
        let index = surfaces.len() as u16;
        surfaces.push(source);
        self.commands.push(PaintCmd::Surface {
            rect,
            clip,
            index,
            alpha: alpha.min(15),
            angle,
        })
    }

    pub fn bitmap(
        &mut self,
        rect: Rect,
        source: &'static crate::bitmap::Bitmap,
        angle: u8,
        alpha: u8,
    ) -> Result<(), BuildError> {
        let clip = self.current_clip();
        let rect = self.to_global(rect);
        if alpha != 0 && rect.intersects(clip) {
            self.commands.push(PaintCmd::Bitmap {
                rect,
                clip,
                source,
                angle,
                alpha: alpha.min(15),
            })?;
        }
        Ok(())
    }

    pub fn alpha_mask(
        &mut self,
        rect: Rect,
        source: &'static crate::bitmap::Bitmap,
        color: Rgb565,
        alpha: u8,
    ) -> Result<(), BuildError> {
        let clip = self.current_clip();
        let rect = self.to_global(rect);
        if alpha != 0 && !rect.intersect(clip).is_empty() {
            self.commands.push(PaintCmd::AlphaMask {
                rect,
                clip,
                source,
                color,
                alpha: alpha.min(15),
            })?;
        }
        Ok(())
    }

    pub fn fill(&mut self, rect: Rect, color: Rgb565) -> Result<(), BuildError> {
        let clip = self.current_clip();
        let rect = self.to_global(rect).intersect(clip);
        if !rect.is_empty() {
            self.commands
                .push(PaintCmd::FillRect { rect, clip, color })?;
        }
        Ok(())
    }

    pub fn rounded_rect(
        &mut self,
        rect: Rect,
        color: Rgb565,
        radius: u8,
        border_color: Option<Rgb565>,
        border_width: u8,
    ) -> Result<(), BuildError> {
        let clip = self.current_clip();
        let g = self.to_global(rect);
        if !g.intersect(clip).is_empty() {
            self.commands.push(PaintCmd::FillRoundedRect {
                rect: g,
                clip,
                color,
                radius,
                border_color,
                border_width,
            })?;
        }
        Ok(())
    }

    pub fn gradient_rect(
        &mut self,
        rect: Rect,
        start: Rgb565,
        end: Rgb565,
        direction: u8,
        radius: u8,
    ) -> Result<(), BuildError> {
        let clip = self.current_clip();
        let g = self.to_global(rect);
        if !g.intersect(clip).is_empty() {
            self.commands.push(PaintCmd::GradientRect {
                rect: g,
                clip,
                start,
                end,
                direction,
                radius,
            })?;
        }
        Ok(())
    }

    pub fn stroke_rect(&mut self, rect: Rect, color: Rgb565, width: u8) -> Result<(), BuildError> {
        if width == 0 {
            return Ok(());
        }
        let clip = self.current_clip();
        let g = self.to_global(rect);
        if !g.intersect(clip).is_empty() {
            self.commands.push(PaintCmd::StrokeRect {
                rect: g,
                clip,
                color,
                width,
            })?;
        }
        Ok(())
    }

    pub fn stroke_rounded_rect(
        &mut self,
        rect: Rect,
        color: Rgb565,
        width: u8,
        radius: u8,
    ) -> Result<(), BuildError> {
        if width == 0 {
            return Ok(());
        }
        let clip = self.current_clip();
        let g = self.to_global(rect);
        if !g.intersect(clip).is_empty() {
            self.commands.push(PaintCmd::StrokeRoundedRect {
                rect: g,
                clip,
                color,
                width,
                radius,
            })?;
        }
        Ok(())
    }

    pub fn overlay(&mut self, rect: Rect, color: Rgb565, alpha: u8) -> Result<(), BuildError> {
        let clip = self.current_clip();
        let g = self.to_global(rect).intersect(clip);
        if alpha != 0 && !g.is_empty() {
            self.commands.push(PaintCmd::Overlay {
                rect: g,
                clip,
                color,
                alpha: alpha.min(15),
            })?;
        }
        Ok(())
    }

    pub fn icon(&mut self, rect: Rect, kind: IconKind, color: Rgb565) -> Result<(), BuildError> {
        if kind == IconKind::None {
            return Ok(());
        }
        let clip = self.current_clip();
        let g = self.to_global(rect);
        if !g.intersect(clip).is_empty() {
            self.commands.push(PaintCmd::Icon {
                rect: g,
                clip,
                kind,
                color,
            })?;
        }
        Ok(())
    }

    pub fn shadow(
        &mut self,
        rect: Rect,
        radius: u8,
        shadow: crate::effects::Shadow,
    ) -> Result<(), BuildError> {
        if shadow.opacity == 0 {
            return Ok(());
        }
        let clip = self.current_clip();
        let g = self.to_global(rect);
        if !g.is_empty() && shadow.bounds(g).intersects(clip) {
            self.commands.push(PaintCmd::Shadow {
                rect: g,
                clip,
                shadow,
                projection: crate::effects::Projection(0),
                radius: radius.into(),
            })?;
        }
        Ok(())
    }

    pub fn line(
        &mut self,
        p0: Point,
        p1: Point,
        color: Rgb565,
        width: u8,
        aa: bool,
    ) -> Result<(), BuildError> {
        if width == 0 {
            return Ok(());
        }
        let cmd = PaintCmd::Line {
            p0: self.point_global(p0),
            p1: self.point_global(p1),
            clip: self.current_clip(),
            color,
            width,
            aa,
        };
        if !cmd.bounds().is_empty() {
            self.commands.push(cmd)?;
        }
        Ok(())
    }

    /// Submit a prepared flash-resident outline as a single command.
    pub fn static_path(
        &mut self,
        rect: Rect,
        path: &'static crate::path::Path<'static>,
        paint: crate::path::PathPaint,
        color: Rgb565,
    ) -> Result<(), BuildError> {
        let rect = self.to_global(rect);
        let clip = self.current_clip();
        if !path.edges.is_empty() && rect.intersects(clip) {
            self.commands.push(PaintCmd::StaticPath {
                path,
                rect,
                clip,
                paint,
                color,
            })?
        }
        Ok(())
    }
    /// Snapshot a runtime-generated outline into the bounded command list.
    pub fn path(
        &mut self,
        rect: Rect,
        path: crate::path::Path<'_>,
        paint: crate::path::PathPaint,
        color: Rgb565,
    ) -> Result<(), BuildError> {
        let rect = self.to_global(rect);
        let clip = self.current_clip();
        if path.edges.is_empty() || !rect.intersects(clip) {
            return Ok(());
        }
        let chunks = path.edges.len().div_ceil(4);
        if chunks >= MAX_COMMANDS - self.commands.count {
            return Err(BuildError::Commands);
        }
        self.commands.push(PaintCmd::PathRun {
            rect,
            clip,
            paint,
            color,
            chunks: chunks as u16,
        })?;
        for slice in path.edges.chunks(4) {
            let mut edges = [crate::path::PathEdge::EMPTY; 4];
            edges[..slice.len()].copy_from_slice(slice);
            self.commands.push(PaintCmd::PathEdges {
                len: slice.len() as u8,
                edges,
            })?;
        }
        Ok(())
    }
    pub fn circle(
        &mut self,
        center: Point,
        radius: u16,
        fill: Option<Rgb565>,
        border_color: Option<Rgb565>,
        border_width: u8,
    ) -> Result<(), BuildError> {
        if fill.is_none() && (border_color.is_none() || border_width == 0) {
            return Ok(());
        }
        let cmd = PaintCmd::Circle {
            center: self.point_global(center),
            clip: self.current_clip(),
            radius,
            fill,
            border_color,
            border_width,
        };
        if !cmd.bounds().is_empty() {
            self.commands.push(cmd)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &mut self,
        center: Point,
        radius: u16,
        start_deg: i16,
        sweep_deg: i16,
        width: u8,
        cap_round: bool,
        color: Rgb565,
    ) -> Result<(), BuildError> {
        if width == 0 || sweep_deg == 0 {
            return Ok(());
        }
        let cmd = PaintCmd::Arc {
            center: self.point_global(center),
            clip: self.current_clip(),
            radius,
            start_deg,
            sweep_deg,
            color,
            width,
            cap_round,
        };
        if !cmd.bounds().is_empty() {
            self.commands.push(cmd)?;
        }
        Ok(())
    }

    /// Draw the node's own text (`props.text`) inside a widget-local `rect`.
    pub fn node_text(
        &mut self,
        rect: Rect,
        props: &StoredProps,
        color: Rgb565,
        align: TextAlign,
    ) -> Result<(), BuildError> {
        if matches!(props.text(), crate::view::TextRef::Static("")) {
            return Ok(());
        }
        let clip = self.current_clip();
        let g = self.to_global(rect);
        let eff_clip = g.intersect(clip);
        if eff_clip.is_empty() {
            return Ok(());
        }
        self.commands.push(PaintCmd::text_styled(
            Point::new(g.x, g.y),
            eff_clip,
            TextId(self.slot),
            color,
            g.w,
            props.max_lines,
            props.ellipsis(),
            props.font(),
            align,
            props.text_style(),
            props.letter_spacing,
            props.line_spacing,
        ))
    }

    /// Draw short inline UTF-8 text (up to 12 bytes) inside widget-local `rect`.
    pub fn text(
        &mut self,
        rect: Rect,
        text: &str,
        color: Rgb565,
        font: FontId,
        align: TextAlign,
    ) -> Result<(), BuildError> {
        let clip = self.current_clip();
        let g = self.to_global(rect);
        let eff_clip = g.intersect(clip);
        if eff_clip.is_empty() || g.w == 0 {
            return Ok(());
        }
        let wrap_w = g.w.min(u16::MAX as u32) as u16;
        if let Some(cmd) = PaintCmd::inline_text(
            Point::new(g.x, g.y),
            eff_clip,
            text,
            color,
            wrap_w,
            font,
            align,
        ) {
            self.commands.push(cmd)?;
        }
        Ok(())
    }

    /// Paint the full default appearance of the current built-in node (valid inside `paint_override`).
    pub fn paint_default(&mut self) -> Result<(), BuildError> {
        paint_builtin_node(self.ctx, self.commands)
    }

    pub fn paint_box_bg(&mut self) -> Result<(), BuildError> {
        let ctx = self.ctx;
        emit_box_bg_and_border(ctx, self.commands)
    }

    pub fn paint_slider_track(&mut self) -> Result<(), BuildError> {
        let ctx = self.ctx;
        if ctx.node.kind == NodeKind::Slider {
            paint_slider_parts(ctx, true, false, self.commands)?;
        }
        Ok(())
    }

    pub fn paint_slider_knob(&mut self) -> Result<(), BuildError> {
        let ctx = self.ctx;
        if ctx.node.kind == NodeKind::Slider {
            paint_slider_parts(ctx, false, true, self.commands)?;
        }
        Ok(())
    }

    pub fn paint_toggle_track(&mut self) -> Result<(), BuildError> {
        let ctx = self.ctx;
        if ctx.node.kind == NodeKind::Toggle {
            paint_toggle_parts(ctx, true, false, self.commands)?;
        }
        Ok(())
    }

    pub fn paint_toggle_knob(&mut self) -> Result<(), BuildError> {
        let ctx = self.ctx;
        if ctx.node.kind == NodeKind::Toggle {
            paint_toggle_parts(ctx, false, true, self.commands)?;
        }
        Ok(())
    }

    pub fn paint_progress_bar(&mut self) -> Result<(), BuildError> {
        let ctx = self.ctx;
        if ctx.node.kind == NodeKind::ProgressBar {
            paint_progress_bar_node(ctx, self.commands)?;
        }
        Ok(())
    }
}

pub fn emit_commands(
    tree: &RetainedTree,
    anims: &AnimPool,
    focus: PaintFocusInfo,
    screen: Rect,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    emit_commands_with(tree, anims, focus, screen, out, |_, _, _| Ok(()))
}

pub fn emit_commands_with(
    tree: &RetainedTree,
    anims: &AnimPool,
    focus: PaintFocusInfo,
    screen: Rect,
    out: &mut CommandList,
    custom: impl FnMut(Key, &StoredProps, &mut WidgetCanvas<'_>) -> Result<(), BuildError>,
) -> Result<(), BuildError> {
    emit_commands_with_override(tree, anims, focus, screen, out, custom, |_, _, _, _, _| {
        Ok(PaintOverride::Default)
    })
}

pub fn emit_commands_with_override(
    tree: &RetainedTree,
    anims: &AnimPool,
    focus: PaintFocusInfo,
    screen: Rect,
    out: &mut CommandList,
    mut custom: impl FnMut(Key, &StoredProps, &mut WidgetCanvas<'_>) -> Result<(), BuildError>,
    mut override_cb: impl FnMut(
        Key,
        NodeKind,
        &StoredProps,
        PaintState,
        &mut WidgetCanvas<'_>,
    ) -> Result<PaintOverride, BuildError>,
) -> Result<(), BuildError> {
    out.reset();
    out.screen = screen;

    let Some(root_idx) = tree.root.get() else {
        return Ok(());
    };

    let root = &tree.nodes[root_idx];
    out.root_bg = root.props.bg().unwrap_or(Rgb565::new(0, 0, 0));

    let mut stack = [PaintDepthFrame {
        slot: 0,
        stage: 0,
        next_child: SlotLink::NONE,
    }; MAX_DEPTH + 2];
    let mut sp = 0usize;
    stack[sp] = PaintDepthFrame {
        slot: root_idx as u16,
        stage: 0,
        next_child: SlotLink::NONE,
    };
    sp += 1;

    let mut outline_layer = None;
    let mut group_at = [0u16; MAX_DEPTH + 2];
    #[cfg(feature = "layers")]
    let mut caster_clips: [Option<Rect>; MAX_DEPTH + 2] = [None; MAX_DEPTH + 2];
    while sp > 0 {
        let top = sp - 1;
        let frame = stack[top];
        let node = &tree.nodes[frame.slot as usize];
        if !node.occupied || !node.props.visible() {
            sp -= 1;
            continue;
        }

        let effect = tree.effects.get(node.key);
        let outline = crate::composition::body_outline(node.props.geometry, effect.composition);
        let projected_outline = effect.layer().is_some_and(|l| l.projection.0 != 0)
            && matches!(
                outline,
                crate::composition::Outline::Path(..)
                    | crate::composition::Outline::Circle
                    | crate::composition::Outline::Capsule
            );
        let shadow_in_group = projected_outline
            || effect
                .composition
                .is_some_and(|c| c.shadow_source == crate::composition::ShadowSource::Content);
        let rect = node.layout_rect;
        #[cfg(feature = "layers")]
        let clip = caster_clips[top].map_or(node.clip_rect, |r| {
            if node.kind.is_scroll_view() {
                r.intersect(rect)
            } else {
                r
            }
        });
        #[cfg(not(feature = "layers"))]
        let clip = node.clip_rect;
        #[cfg(feature = "layers")]
        let source_clip = if let Some(shadow) = effect.shadow
            && shadow_in_group
            && shadow.bounds(rect).intersects(clip)
        {
            // Capture the halo on the far side of a scrolling ancestor before clipping the output.
            Some(if effect.layer().is_some_and(|l| l.projection.0 != 0) {
                rect
            } else {
                rect.intersect(
                    clip.union(
                        clip.translate(-i32::from(shadow.x), -i32::from(shadow.y))
                            .expand(u32::from(shadow.blur)),
                    ),
                )
            })
        } else {
            caster_clips[top].map(|_| clip)
        };
        #[cfg(feature = "layers")]
        let mut adjusted;
        #[cfg(feature = "layers")]
        let node = if let Some(c) = source_clip {
            adjusted = *node;
            adjusted.clip_rect = c;
            adjusted.visible_rect = node.ink_rect.intersect(c);
            &adjusted
        } else {
            node
        };
        if frame.stage == 0 {
            let visible = rect.intersects(node.clip_rect) && !node.visible_rect.is_empty();

            #[cfg(feature = "layers")]
            let backdrop_end = out.count;
            group_at[top] = 0;
            let mut composite_shadow = None;
            if let Some(shadow) = effect.shadow {
                let mut shadow = if effect.press_depth.is_enabled() {
                    let press = if effect.press_depth.duration_ms() == 0 {
                        if node.pressed { 15 } else { 0 }
                    } else {
                        anims
                            .cached_value(node.key, AnimProp::PressDepth)
                            .unwrap_or(if node.pressed { 15 } else { 0 })
                    };
                    shadow.pressed_scaled(press as u8, effect.press_depth.scale())
                } else {
                    shadow
                };
                let projection = effect.layer().map_or(crate::effects::Projection(0), |l| {
                    shadow.opacity = (u16::from(shadow.opacity) * u16::from(l.opacity) / 15) as u8;
                    l.projection
                });
                if !rect.is_empty() && shadow.bounds(rect).intersects(clip) && shadow.opacity > 0 {
                    if shadow_in_group {
                        composite_shadow = Some(shadow);
                    } else if let crate::composition::Outline::Path(path, rule) = outline {
                        out.push(PaintCmd::OutlineShadow {
                            rule,
                            path,
                            rect,
                            clip,
                            shadow,
                        })?;
                    } else {
                        let (rect, radius) = outline.bounds_and_radius(rect, node.props.radius);
                        if let Ok(radius) = u8::try_from(radius) {
                            out.push(PaintCmd::Shadow {
                                rect,
                                clip,
                                shadow,
                                projection,
                                radius,
                            })?;
                        } else {
                            // Projected circles and capsules use the layer compositor above.
                            out.push(PaintCmd::WideRoundShadow {
                                rect,
                                clip,
                                shadow,
                                radius,
                            })?;
                        }
                    }
                }
            }
            #[cfg(feature = "layers")]
            {
                let isolated = effect.layer.is_some()
                    || composite_shadow.is_some()
                    || effect
                        .composition
                        .is_some_and(|c| !matches!(c.mask, crate::composition::Mask::Opaque));
                if visible && isolated {
                    let layer = effect.layer.unwrap_or(crate::effects::Layer::OPAQUE);
                    out.effect_error_key = Some(node.key);
                    let start = out.begin_layer(
                        node.key,
                        rect,
                        clip,
                        rect.intersect(node.clip_rect),
                        layer,
                        node.props.radius,
                        backdrop_end,
                    )?;
                    let cache = &mut out.layers.as_mut().unwrap()[usize::from(out.used_layers) - 1];
                    cache.composition = effect
                        .composition
                        .copied()
                        .or_else(|| {
                            node.props
                                .geometry
                                .map(|_| crate::composition::SurfaceComposition::DEFAULT)
                        })
                        .map(|c| crate::composition::SurfaceComposition { outline, ..c });
                    cache.shadow = composite_shadow;
                    if let Some(shadow) = composite_shadow
                        && let PaintCmd::Group { rect: bounds, .. } = &mut out.cmds[start]
                    {
                        *bounds = rect.union(shadow.bounds(rect))
                    }
                    group_at[top] = (start + 1) as u16;
                    out.effect_error_key = None;
                }
            }
            if visible
                && group_at[top] == 0
                && let Some(composition) = effect.composition
                && composition.masks_content()
            {
                group_at[top] = (out.count + 1) as u16;
                out.push(PaintCmd::MaskGroup {
                    slot: frame.slot as u8,
                    rect,
                    clip,
                    composition,
                    radius: node.props.radius,
                    end: 0,
                })?;
            }
            let _ = composite_shadow;
            if visible || node.kind == NodeKind::Modal {
                let part = tree.part_styles.get(usize::from(frame.slot));
                let is_focused = focus.focused_slot == Some(frame.slot);
                let anim_progress = if node.motion.state() != crate::view::MotionState::None {
                    anims
                        .cached_value(node.key, AnimProp::Style)
                        .unwrap_or_else(|| node.motion_target(is_focused && focus.ring_visible))
                        .clamp(0, ANIM_SCALE)
                } else if node.kind == NodeKind::Toggle {
                    anims
                        .cached_value(node.key, AnimProp::ToggleKnob)
                        .unwrap_or(if node.props.checked() { ANIM_SCALE } else { 0 })
                        .clamp(0, ANIM_SCALE)
                } else {
                    anims.cached_value(node.key, AnimProp::Custom0).unwrap_or(0)
                };
                let state = PaintState {
                    pressed: node.pressed,
                    focused: is_focused && focus.ring_visible,
                    editing: is_focused && focus.editing,
                    checked: node.props.checked(),
                    enabled: node.props.enabled(),
                    anim_progress,
                };
                let root_bg = out.root_bg;
                let init_clip = node.clip_rect.intersect(rect);
                let clip_stack = [init_clip, Rect::ZERO, Rect::ZERO, Rect::ZERO];
                let ctx = BuiltinPaintCtx {
                    composition: effect.composition,
                    node,
                    anims,
                    focus,
                    screen,
                    part,
                    slot: frame.slot,
                    root_bg,
                    in_tab_bar: node.kind == NodeKind::Tab
                        && node
                            .parent
                            .get()
                            .is_some_and(|i| tree.nodes[i].kind == NodeKind::TabBar),
                };
                out.effect_error_key = Some(node.key);
                if node.kind == NodeKind::Custom || node.props.paint_mode() == PaintMode::Override {
                    let mut canvas = WidgetCanvas {
                        bounds: rect,
                        clip_stack,
                        clip_depth: 1,
                        commands: out,
                        pressed: node.pressed,
                        state,
                        slot: frame.slot,
                        ctx,
                    };
                    if node.kind == NodeKind::Custom {
                        custom(node.key, &node.props, &mut canvas)?;
                    } else if override_cb(node.key, node.kind, &node.props, state, &mut canvas)?
                        == PaintOverride::Default
                    {
                        paint_builtin_node(ctx, out)?;
                    }
                } else {
                    paint_builtin_node(ctx, out)?;
                }
                out.effect_error_key = None;
                if node.kind == NodeKind::TabBar {
                    paint_sliding_indicator(tree, node, anims, root_bg, out)?;
                }
            }

            stack[top].stage = 1;
            stack[top].next_child = node.first_child;
        } else {
            // Walk children in forward declaration order; emit post-children overlays when finished.
            let mut next_link = frame.next_child;
            let mut found = None;
            while let Some(ch_idx) = next_link.get() {
                let cn = &tree.nodes[ch_idx];
                if cn.occupied && cn.props.visible() {
                    found = Some((ch_idx, cn.next_sibling));
                    break;
                }
                next_link = cn.next_sibling;
            }

            if let Some((ch_idx, after_sib)) = found {
                stack[top].next_child = after_sib;
                if sp >= stack.len() {
                    return Err(BuildError::Depth);
                }
                #[cfg(feature = "layers")]
                {
                    caster_clips[sp] = source_clip.map(|c| {
                        let b = u32::from(node.props.border_width);
                        if node.kind.is_scroll_view() {
                            c.intersect(rect)
                        } else {
                            c.intersect(rect.inset(b, b, b, b))
                        }
                    });
                }
                stack[sp] = PaintDepthFrame {
                    slot: ch_idx as u16,
                    stage: 0,
                    next_child: SlotLink::NONE,
                };
                sp += 1;
                continue;
            }

            // Stage post-children: ScrollY/ScrollX scrollbar & focus ring, then pop frame
            let clip = node.clip_rect;
            let rect = node.layout_rect;

            if matches!(node.kind, NodeKind::ScrollY | NodeKind::ScrollX) {
                let sb_part = tree.part_styles.get(usize::from(frame.slot));
                if !sb_part.scrollbar_hidden() {
                    let is_x = node.kind == NodeKind::ScrollX;
                    let vp_extent = if is_x { rect.w } else { rect.h };
                    let cross_extent = if is_x { rect.h } else { rect.w };
                    if node.content_height > vp_extent && vp_extent > 8 && cross_extent > 6 {
                        let sb_thick = sb_part
                            .knob_size()
                            .map(|(w, _)| u32::from(w.max(1)))
                            .unwrap_or(3)
                            .min(cross_extent.saturating_sub(2));
                        let track_len = vp_extent.saturating_sub(4);
                        let min_thumb = 12.min(track_len) as u64;
                        let thumb_len = ((track_len as u64 * vp_extent as u64)
                            / node.content_height as u64)
                            .clamp(min_thumb, track_len as u64)
                            as u32;
                        let max_scroll = node.content_height.saturating_sub(vp_extent).max(1);
                        let thumb_travel = track_len.saturating_sub(thumb_len);
                        let thumb_pos = 2
                            + ((thumb_travel as u64 * (node.scroll_y.max(0) as u64))
                                / max_scroll as u64) as i32;
                        let thumb_rect = if is_x {
                            Rect::new(
                                rect.x + thumb_pos,
                                rect.y + rect.h as i32 - (sb_thick as i32 + 1),
                                thumb_len,
                                sb_thick,
                            )
                        } else {
                            Rect::new(
                                rect.x + rect.w as i32 - (sb_thick as i32 + 1),
                                rect.y + thumb_pos,
                                sb_thick,
                                thumb_len,
                            )
                        };
                        if !thumb_rect.intersect(clip).is_empty() {
                            out.push(PaintCmd::FillRoundedRect {
                                rect: thumb_rect,
                                clip,
                                color: sb_part.knob_color().unwrap_or(node.props.accent),
                                radius: sb_part.knob_radius().unwrap_or(1),
                                border_color: None,
                                border_width: 0,
                            })?;
                        }
                    }
                }
            }

            if focus.ring_visible && focus.focused_slot == Some(frame.slot) {
                // Paint above adjacent controls and scrollbars in this flow, but below
                // the next overlay sibling (for example a toast or a modal).
                let mut layer = frame.slot;
                while let Some(parent) = tree.nodes[usize::from(layer)].parent.get_u16() {
                    let boundary = tree.effects.get(tree.nodes[usize::from(layer)].key);
                    if boundary.layer().is_some()
                        || boundary.composition.is_some_and(|c| c.masks_content())
                        || matches!(
                            tree.nodes[usize::from(parent)].kind,
                            NodeKind::Stack | NodeKind::Root | NodeKind::Modal
                        )
                    {
                        break;
                    }
                    layer = parent;
                }
                outline_layer = Some(layer);
            }
            let children_end = out.count as u16;
            if outline_layer == Some(frame.slot)
                && let Some(slot) = focus.focused_slot
            {
                let focused = &tree.nodes[usize::from(slot)];
                let part = tree.part_styles.get(usize::from(slot));
                emit_focus_outline(
                    &tree.effects.get(focused.key),
                    focused,
                    part,
                    screen,
                    focus.editing,
                    out,
                )?;
                outline_layer = None;
            }
            if group_at[top] != 0 {
                let start = usize::from(group_at[top] - 1);
                // The control's own focus decoration is above its content mask. Descendant
                // decorations remain inside the ancestor group; rotating planes carry their ring.
                let own_focus = focus.focused_slot == Some(frame.slot);
                let end = if own_focus && !effect.layer().is_some_and(|l| l.projection.0 != 0) {
                    children_end
                } else {
                    out.count as u16
                };
                if let PaintCmd::Group { end: limit, .. } | PaintCmd::MaskGroup { end: limit, .. } =
                    &mut out.cmds[start]
                {
                    *limit = end;
                }
            }
            sp -= 1;
        }
    }
    #[cfg(feature = "layers")]
    if let Some(layers) = out.layers.as_mut() {
        layers.truncate(usize::from(out.used_layers));
    }
    Ok(())
}

fn color_state(node: &RetainedNode, pressed: bool) -> VisualState {
    VisualState(
        (u8::from(!node.props.enabled()) * VisualState::DISABLED.0)
            | (u8::from(pressed) * VisualState::PRESSED.0)
            | (u8::from(node.props.checked()) * VisualState::CHECKED.0),
    )
}

fn resolve_state_fg(node: &RetainedNode, part: &WidgetPartStyle) -> Rgb565 {
    part.foreground(color_state(node, node.pressed), node.props.fg)
}

fn resolve_state_bg(
    node: &RetainedNode,
    part: &WidgetPartStyle,
    root_bg: Rgb565,
) -> Option<Rgb565> {
    state_background(node, part, root_bg, node.pressed)
}

fn state_background(
    node: &RetainedNode,
    part: &WidgetPartStyle,
    root_bg: Rgb565,
    pressed: bool,
) -> Option<Rgb565> {
    if let Some(c) = part.background(color_state(node, pressed)) {
        return Some(c);
    }
    if pressed {
        match (node.props.bg(), node.props.press_feedback()) {
            (Some(c), fb) => Some(apply_press_feedback(c, node.props.accent, fb)),
            (None, crate::view::PressFeedback::None) => None,
            (None, fb) if matches!(node.kind, NodeKind::Button | NodeKind::Tab) => {
                Some(apply_press_feedback(root_bg, node.props.accent, fb))
            }
            (None, _) => None,
        }
    } else {
        node.props.bg()
    }
}

fn paint_builtin_node(ctx: BuiltinPaintCtx<'_>, out: &mut CommandList) -> Result<(), BuildError> {
    let node = ctx.node;
    let rect = node.layout_rect;
    let clip = node.clip_rect;
    match node.kind {
        NodeKind::Custom => Ok(()),
        NodeKind::Root => {
            if let Some(bg) = node.props.bg() {
                out.push(PaintCmd::FillRect {
                    rect,
                    clip,
                    color: bg,
                })?;
            }
            Ok(())
        }
        NodeKind::Row
        | NodeKind::Column
        | NodeKind::Stack
        | NodeKind::ScrollY
        | NodeKind::ScrollX
        | NodeKind::Grid
        | NodeKind::TabPanel
        | NodeKind::TabBar
        | NodeKind::Wrap
        | NodeKind::CustomContainer => emit_box_bg_and_border(ctx, out),
        NodeKind::Modal => {
            if node.props.overlay_alpha > 0 {
                out.push(PaintCmd::Overlay {
                    rect: ctx.screen,
                    clip: ctx.screen,
                    color: node.props.overlay_color,
                    alpha: node.props.overlay_alpha.min(15),
                })?;
            }
            emit_box_bg_and_border(ctx, out)
        }
        NodeKind::Label => {
            emit_box_bg_and_border(ctx, out)?;
            let inner = rect.inset(
                node.props.padding.left as u32,
                node.props.padding.top as u32,
                node.props.padding.right as u32,
                node.props.padding.bottom as u32,
            );
            let text_x = inner.x;
            let mut text_y = inner.y;
            let align = node.props.text_align();
            let font = node.props.font();
            let text_style = node.props.text_style();
            let mut text_clip = clip;
            if node.text_cache.valid {
                let text_size = node.text_cache.size;
                if node.props.align_cross == Align::Center && inner.h > text_size.height {
                    text_y += ((inner.h - text_size.height) / 2) as i32;
                } else if node.props.align_cross == Align::End && inner.h > text_size.height {
                    text_y += (inner.h - text_size.height) as i32;
                }
                if node.text_cache.align == align
                    && (align == TextAlign::Start || node.text_cache.wrap_width == inner.w)
                {
                    let mut rel_ink = node.text_cache.rel_ink;
                    if text_style.underline() || text_style.strikethrough() {
                        rel_ink = rel_ink.union(Rect::new(
                            0,
                            0,
                            text_size.width.max(1),
                            text_size.height.saturating_add(3),
                        ));
                    }
                    text_clip = clip.intersect(rel_ink.translate(text_x, text_y));
                }
            }
            if !text_clip.is_empty() {
                out.push(PaintCmd::text_styled(
                    Point::new(text_x, text_y),
                    text_clip,
                    TextId(ctx.slot),
                    resolve_state_fg(node, ctx.part),
                    inner.w,
                    node.props.max_lines,
                    node.props.ellipsis(),
                    font,
                    align,
                    text_style,
                    node.props.letter_spacing,
                    node.props.line_spacing,
                ))?;
            }
            Ok(())
        }
        NodeKind::Tab => paint_tab(ctx, out),
        NodeKind::Button => paint_button_node(ctx, out),
        NodeKind::Toggle => paint_toggle_parts(ctx, true, true, out),
        NodeKind::Slider => paint_slider_parts(ctx, true, true, out),
        NodeKind::ProgressBar => paint_progress_bar_node(ctx, out),
        NodeKind::Sparkline => {
            emit_box_bg_and_border(ctx, out)?;
            let inner = rect.inset(
                node.props.padding.left as u32,
                node.props.padding.top as u32,
                node.props.padding.right as u32,
                node.props.padding.bottom as u32,
            );
            let eff_clip = clip.intersect(inner);
            if !eff_clip.is_empty() {
                out.push(PaintCmd::Sparkline {
                    rect: inner,
                    clip: eff_clip,
                    color: ctx.part.indicator_color().unwrap_or(node.props.accent),
                    series: node.props.series,
                    min: node.props.min,
                    max: node.props.max,
                    stroke_width: ctx.part.sparkline_width().unwrap_or(1).max(1),
                })?;
            }
            Ok(())
        }
    }
}

fn paint_button_node(ctx: BuiltinPaintCtx<'_>, out: &mut CommandList) -> Result<(), BuildError> {
    let node = ctx.node;
    let part = ctx.part;
    let mut bg = resolve_state_bg(node, part, ctx.root_bg);
    if node.motion.state() == crate::view::MotionState::Pressed
        && let Some(p) = ctx.anims.cached_value(node.key, AnimProp::Style)
    {
        let off = state_background(node, part, ctx.root_bg, false);
        let on = state_background(node, part, ctx.root_bg, true);
        bg = match p {
            ..=0 => off,
            ANIM_SCALE.. => on,
            _ => {
                on.map(|color| blend_rgb565_scale(off.unwrap_or(ctx.root_bg), color, p, ANIM_SCALE))
            }
        };
    }
    emit_box_fill(ctx, bg, out)?;

    paint_control_content(ctx, out)
}

#[derive(Copy, Clone)]
struct ControlContent {
    inner: Rect,
    text: Point,
    wrap_w: u32,
    align: TextAlign,
    icon: Option<Rect>,
    bounds: Rect,
}

fn control_content(node: &RetainedNode, part: &WidgetPartStyle) -> ControlContent {
    let rect = node.layout_rect;
    let inner = rect.inset(
        node.props.padding.left as u32,
        node.props.padding.top as u32,
        node.props.padding.right as u32,
        node.props.padding.bottom as u32,
    );
    let has_icon = node.props.icon != IconKind::None;
    let icon_sz = if has_icon {
        u32::from(part.icon_size().unwrap_or(BUTTON_ICON_SIZE as u8))
    } else {
        0
    };
    let gap_w = if has_icon {
        u32::from(part.icon_gap().unwrap_or(BUTTON_ICON_GAP as u8))
    } else {
        0
    };
    let icon_pos = part.icon_pos().unwrap_or(IconPosition::Left);
    let icon_span = if has_icon { icon_sz + gap_w } else { 0 };
    let text_w = if node.text_cache.valid {
        node.text_cache.size.width
    } else {
        0
    };
    let text_h = if node.text_cache.valid {
        node.text_cache.size.height
    } else {
        16
    };
    let align = node.props.text_align();

    let (text_x, text_y, wrap_w, cmd_align, icon_pt) = match icon_pos {
        IconPosition::Left | IconPosition::Right => {
            let ty = if inner.h > text_h {
                inner.y + ((inner.h - text_h) / 2) as i32
            } else {
                inner.y
            };
            let iy = inner.y + ((inner.h.saturating_sub(icon_sz)) / 2) as i32;
            let ww = inner.w.saturating_sub(icon_span);
            if align == TextAlign::Start {
                let total_w = icon_sz + gap_w + text_w;
                let start_x = if inner.w > total_w {
                    inner.x + ((inner.w - total_w) / 2) as i32
                } else {
                    inner.x
                };
                if has_icon && icon_pos == IconPosition::Right {
                    let ix = start_x + text_w as i32 + gap_w as i32;
                    (start_x, ty, ww, TextAlign::Start, Some(Point::new(ix, iy)))
                } else if has_icon {
                    (
                        start_x + icon_span as i32,
                        ty,
                        ww,
                        TextAlign::Start,
                        Some(Point::new(start_x, iy)),
                    )
                } else {
                    (start_x, ty, ww, TextAlign::Start, None)
                }
            } else if has_icon && icon_pos == IconPosition::Right {
                let ix = inner.x + inner.w.saturating_sub(icon_sz) as i32;
                (inner.x, ty, ww, align, Some(Point::new(ix, iy)))
            } else if has_icon {
                (
                    inner.x + icon_span as i32,
                    ty,
                    ww,
                    align,
                    Some(Point::new(inner.x, iy)),
                )
            } else {
                (inner.x, ty, ww, align, None)
            }
        }
        IconPosition::Top | IconPosition::Bottom => {
            let total_h = icon_sz + gap_w + text_h;
            let top_y = if inner.h > total_h {
                inner.y + ((inner.h - total_h) / 2) as i32
            } else {
                inner.y
            };
            let ix = inner.x + ((inner.w.saturating_sub(icon_sz)) / 2) as i32;
            let (iy, ty) = if icon_pos == IconPosition::Top {
                (top_y, top_y + (icon_sz + gap_w) as i32)
            } else {
                (top_y + (text_h + gap_w) as i32, top_y)
            };
            (
                inner.x,
                ty,
                inner.w,
                align,
                has_icon.then_some(Point::new(ix, iy)),
            )
        }
    };

    let icon = icon_pt.map(|ip| Rect::new(ip.x, ip.y, icon_sz, icon_sz));
    let spare = wrap_w.saturating_sub(text_w);
    let label_x = text_x
        + match cmd_align {
            TextAlign::Start => 0,
            TextAlign::Center => spare / 2,
            TextAlign::End => spare,
        } as i32;
    let label = Rect::new(label_x, text_y, text_w.min(wrap_w), text_h);
    let bounds = icon.map_or(label, |r| r.union(label)).intersect(inner);
    ControlContent {
        inner,
        text: Point::new(text_x, text_y),
        wrap_w,
        align: cmd_align,
        icon,
        bounds,
    }
}

fn paint_control_content(
    ctx: BuiltinPaintCtx<'_>,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    draw_control_content(ctx, control_content(ctx.node, ctx.part), out)
}

fn draw_control_content(
    ctx: BuiltinPaintCtx<'_>,
    content: ControlContent,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    let node = ctx.node;
    let clip = node.clip_rect.intersect(content.inner);
    let fg_col = resolve_state_fg(node, ctx.part);
    let Point {
        x: text_x,
        y: text_y,
    } = content.text;
    let wrap_w = content.wrap_w;
    let cmd_align = content.align;
    let font = node.props.font();
    if let Some(rect) = content.icon {
        out.push(PaintCmd::Icon {
            rect,
            clip,
            kind: node.props.icon,
            color: ctx.part.icon_color().unwrap_or(fg_col),
        })?;
    }
    let text_style = node.props.text_style();
    let text_clip = if node.text_cache.valid
        && node.text_cache.align == cmd_align
        && (cmd_align == TextAlign::Start || node.text_cache.wrap_width == wrap_w)
    {
        let mut rel_ink = node.text_cache.rel_ink;
        if text_style.underline() || text_style.strikethrough() {
            rel_ink = rel_ink.union(Rect::new(
                0,
                0,
                node.text_cache.size.width.max(1),
                node.text_cache.size.height.saturating_add(3),
            ));
        }
        clip.intersect(rel_ink.translate(text_x, text_y))
    } else {
        clip
    };
    if !text_clip.is_empty() {
        out.push(PaintCmd::text_styled(
            Point::new(text_x, text_y),
            text_clip,
            TextId(ctx.slot),
            fg_col,
            wrap_w,
            node.props.max_lines.max(1),
            node.props.ellipsis(),
            font,
            cmd_align,
            text_style,
            node.props.letter_spacing,
            node.props.line_spacing,
        ))?;
    }
    Ok(())
}

fn paint_toggle_parts(
    ctx: BuiltinPaintCtx<'_>,
    draw_track: bool,
    draw_knob: bool,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    let node = ctx.node;
    let rect = node.layout_rect;
    let clip = node.clip_rect;
    let part = ctx.part;
    if rect.is_empty() {
        return Ok(());
    }
    let track_w = rect.w.min(48);
    let track_h = part
        .track_thickness()
        .map(|t| rect.h.min(u32::from(t.max(4))))
        .unwrap_or_else(|| rect.h.min(26));
    let track_x = rect.x + ((rect.w - track_w) / 2) as i32;
    let track_y = rect.y + ((rect.h - track_h) / 2) as i32;
    let track_rect = Rect::new(track_x, track_y, track_w, track_h);

    let default_progress = if node.props.checked() { ANIM_SCALE } else { 0 };
    let progress = ctx
        .anims
        .cached_value(
            node.key,
            if node.motion.state() == crate::view::MotionState::Checked {
                AnimProp::Style
            } else {
                AnimProp::ToggleKnob
            },
        )
        .unwrap_or(default_progress)
        .clamp(0, ANIM_SCALE);

    if draw_track {
        let track_off = part
            .track_color()
            .or_else(|| node.props.bg())
            .unwrap_or(Rgb565::new(10, 20, 10));
        let track_on = part
            .indicator_color()
            .or_else(|| part.checked_bg())
            .unwrap_or(node.props.accent);
        let track_color = blend_rgb565_scale(track_off, track_on, progress, ANIM_SCALE);
        let r = part.track_radius().unwrap_or((track_h / 2) as u8);
        out.push(PaintCmd::FillRoundedRect {
            rect: track_rect,
            clip,
            color: track_color,
            radius: r,
            border_color: part
                .track_border_color()
                .or_else(|| node.props.border_color()),
            border_width: part.track_border_width().unwrap_or(node.props.border_width),
        })?;
    }

    if draw_knob {
        let inset = u32::from(
            part.indicator_inset()
                .unwrap_or(((track_h / 4).min(2)) as u8),
        );
        let default_knob = track_h.saturating_sub(inset * 2).min(track_w);
        let (kw, kh) = part
            .knob_size()
            .map(|(w, h)| {
                (
                    rect.w.min(u32::from(w.max(2))),
                    rect.h.min(u32::from(h.max(2))),
                )
            })
            .unwrap_or((default_knob, default_knob));
        if kw > 0 && kh > 0 {
            let travel = track_w.saturating_sub(kw.saturating_add(inset * 2)) as i32;
            let knob_x =
                track_x + inset as i32 + ((travel * progress + ANIM_SCALE / 2) / ANIM_SCALE);
            let knob_y = track_y + ((track_h as i32 - kh as i32) / 2);
            let knob_rect = Rect::new(knob_x, knob_y, kw, kh);
            let knob_col = part
                .knob_color()
                .unwrap_or_else(|| resolve_state_fg(node, part));
            out.push(PaintCmd::FillRoundedRect {
                rect: knob_rect,
                clip,
                color: knob_col,
                radius: part.knob_radius().unwrap_or((kh / 2) as u8),
                border_color: part.knob_border_color(),
                border_width: part.knob_border_width().unwrap_or(
                    if part.knob_border_color().is_some() {
                        1
                    } else {
                        0
                    },
                ),
            })?;
        }
    }
    Ok(())
}

fn animated_width(ctx: BuiltinPaintCtx<'_>, width: u32, final_width: u32) -> u32 {
    let node = ctx.node;
    if node.motion.state() == crate::view::MotionState::Value
        && let Some(p) = ctx.anims.cached_value(node.key, AnimProp::Style)
        && p != node.motion_target(false)
    {
        return (u64::from(width) * p.clamp(0, ANIM_SCALE) as u64 / ANIM_SCALE as u64) as u32;
    }
    final_width
}

fn paint_slider_parts(
    ctx: BuiltinPaintCtx<'_>,
    draw_track: bool,
    draw_knob: bool,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    let node = ctx.node;
    let rect = node.layout_rect;
    let clip = node.clip_rect;
    let part = ctx.part;
    if rect.is_empty() {
        return Ok(());
    }
    let (track, _) = part.slider_track_bounds(rect.size());
    let track = track.translate(rect.x, rect.y);
    let (knob_w, knob_h) = part.slider_knob_size(rect.size());
    let pad_x = (knob_w / 2) as i32;
    let (track_x, track_y, track_h, usable_w) = (track.x, track.y, track.h, track.w);
    let track_bg = part
        .track_color()
        .or_else(|| node.props.bg())
        .unwrap_or(Rgb565::new(10, 20, 10));

    let min = node.props.min as i64;
    let max = node.props.max as i64;
    let span = (max - min).max(1);
    let clamped = (node.props.value as i64).clamp(min, max.max(min));
    let rel = (clamped - min).max(0);
    let exact_width = ((usable_w as i64 * rel) / span) as u32;
    let fill_w = animated_width(ctx, usable_w, exact_width);

    if draw_track && usable_w > 0 {
        let track_rect = Rect::new(track_x, track_y, usable_w, track_h);
        out.push(PaintCmd::FillRoundedRect {
            rect: track_rect,
            clip,
            color: track_bg,
            radius: part.track_radius().unwrap_or((track_h / 2) as u8),
            border_color: part.track_border_color(),
            border_width: part.track_border_width().unwrap_or(0),
        })?;

        if fill_w > 0 {
            out.push(PaintCmd::FillRoundedRect {
                rect: Rect::new(track_x, track_y, fill_w, track_h),
                clip,
                color: part.indicator_color().unwrap_or(node.props.accent),
                radius: part.indicator_radius().unwrap_or((track_h / 2) as u8),
                border_color: None,
                border_width: 0,
            })?;
        }
    }

    if draw_knob && knob_w > 0 && knob_h > 0 {
        let knob_x = if usable_w > 0 {
            track_x + fill_w as i32 - pad_x
        } else {
            rect.x + ((rect.w.saturating_sub(knob_w)) / 2) as i32
        };
        let knob_y = rect.y + ((rect.h.saturating_sub(knob_h)) / 2) as i32;
        let is_editing = ctx.focus.editing && ctx.focus.focused_slot == Some(ctx.slot);
        let knob_col = part
            .knob_color()
            .unwrap_or_else(|| resolve_state_fg(node, part));
        let default_border_w = part.knob_border_width().unwrap_or(1);
        out.push(PaintCmd::FillRoundedRect {
            rect: Rect::new(knob_x, knob_y, knob_w, knob_h),
            clip,
            color: knob_col,
            radius: part
                .knob_radius()
                .unwrap_or_else(|| 4u8.min((knob_h / 2) as u8)),
            border_color: Some(if is_editing {
                part.focused_border().unwrap_or(node.props.focus_color)
            } else {
                part.knob_border_color().unwrap_or(node.props.accent)
            }),
            border_width: if is_editing { 2 } else { default_border_w },
        })?;
    }
    Ok(())
}

fn paint_progress_bar_node(
    ctx: BuiltinPaintCtx<'_>,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    let node = ctx.node;
    let rect = node.layout_rect;
    let clip = node.clip_rect;
    let part = ctx.part;
    if rect.is_empty() || rect.intersect(clip).is_empty() {
        return Ok(());
    }
    let track_bg = part
        .track_color()
        .or_else(|| node.props.bg())
        .unwrap_or(Rgb565::new(10, 20, 10));
    let radius = part
        .track_radius()
        .unwrap_or(node.props.radius)
        .min((rect.h / 2).min(255) as u8);
    let border_color = part
        .track_border_color()
        .or_else(|| node.props.border_color());
    let border_width = part.track_border_width().unwrap_or(node.props.border_width);
    out.push(PaintCmd::FillRoundedRect {
        rect,
        clip,
        color: track_bg,
        radius,
        border_color,
        border_width,
    })?;

    let bw = part.indicator_inset().map(u32::from).unwrap_or_else(|| {
        if border_color.is_some() && border_width > 0 {
            border_width as u32
        } else {
            0
        }
    });
    let inner = rect.inset(bw, bw, bw, bw);
    if !inner.is_empty() {
        let min = node.props.min as i64;
        let max = node.props.max as i64;
        let fill_w = if min >= max {
            inner.w
        } else {
            let clamped = (node.props.value as i64).clamp(min, max);
            let span = max - min;
            (((inner.w as i64) * (clamped - min) + span / 2) / span).clamp(0, inner.w as i64) as u32
        };
        let fill_w = animated_width(ctx, inner.w, fill_w);
        if fill_w > 0 {
            let fill_rect = Rect::new(inner.x, inner.y, fill_w, inner.h);
            let fill_r = part
                .indicator_radius()
                .unwrap_or_else(|| radius.saturating_sub(bw as u8))
                .min((fill_rect.h / 2).min(255) as u8)
                .min((fill_rect.w / 2).min(255) as u8);
            out.push(PaintCmd::FillRoundedRect {
                rect: fill_rect,
                clip: clip.intersect(inner),
                color: part.indicator_color().unwrap_or(node.props.accent),
                radius: fill_r,
                border_color: None,
                border_width: 0,
            })?;
        }
    }
    Ok(())
}

fn emit_path_stroke(
    out: &mut CommandList,
    path: &'static crate::path::Path<'static>,
    rect: Rect,
    clip: Rect,
    width: u8,
    color: Rgb565,
) -> Result<(), BuildError> {
    let units = (u32::from(width) * 1024)
        .div_ceil(rect.w.min(rect.h).max(1))
        .min(u32::from(u16::MAX)) as u16;
    out.push(PaintCmd::StaticPath {
        rect,
        clip,
        path,
        color,
        paint: crate::path::PathPaint::Stroke(units),
    })
}
fn emit_focus_outline(
    style: &crate::effects::SurfaceStyle,
    node: &RetainedNode,
    part: &WidgetPartStyle,
    screen: Rect,
    editing: bool,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    let ring = node.props.focus_ring();
    if !ring.enabled || style.layer().is_some_and(|l| l.opacity == 0) {
        return Ok(());
    }
    let rect = node.layout_rect;
    let ring_clip = node.clip_rect.intersect(screen);
    if !rect.intersect(ring_clip).is_empty() {
        let width = ring.width.clamp(1, 3);
        let geometry = node.focus_outline(part, style, ring_clip);
        if geometry.body.is_empty() {
            return Ok(());
        }
        let ring_rect = geometry.bounds;
        let ring_radius = geometry.radius.min(255) as u8;
        let ring_col = part.focused_border().unwrap_or(node.props.focus_color);
        if let crate::composition::Outline::Path(path, rule) = geometry.shape {
            let outer = geometry.outer as i8;
            let stroke = |outer: i8, width: u8, color| PaintCmd::PathRing {
                path,
                rule,
                rect: geometry.body,
                clip: ring_clip,
                outer,
                inner: outer - width as i8,
                color,
            };
            out.push(stroke(outer, width, ring_col))?;
            if editing {
                out.push(stroke(outer - 1, 1, node.props.accent))?;
            }
            return Ok(());
        }
        if geometry.radius > 255 {
            out.push(PaintCmd::WideRound {
                rect: ring_rect,
                clip: ring_clip,
                radius: geometry.radius,
                width,
                color: ring_col,
            })?;
            if editing {
                out.push(PaintCmd::WideRound {
                    rect: ring_rect.inset(1, 1, 1, 1),
                    clip: ring_clip,
                    radius: geometry.radius - 1,
                    width: 1,
                    color: node.props.accent,
                })?;
            }
            return Ok(());
        }
        if ring_radius != 0 {
            out.push(PaintCmd::StrokeRoundedRect {
                rect: ring_rect,
                clip: ring_clip,
                color: ring_col,
                width,
                radius: ring_radius,
            })?;
        } else {
            out.push(PaintCmd::StrokeRect {
                rect: ring_rect,
                clip: ring_clip,
                color: ring_col,
                width,
            })?;
        }
        if editing {
            let inner_rect = ring_rect.inset(1, 1, 1, 1);
            if ring_radius != 0 {
                out.push(PaintCmd::StrokeRoundedRect {
                    rect: inner_rect,
                    clip: ring_clip,
                    color: node.props.accent,
                    width: 1,
                    radius: ring_radius.saturating_sub(1),
                })?;
            } else {
                out.push(PaintCmd::StrokeRect {
                    rect: inner_rect,
                    clip: ring_clip,
                    color: node.props.accent,
                    width: 1,
                })?;
            }
        }
    }
    Ok(())
}

fn emit_box_bg_and_border(
    ctx: BuiltinPaintCtx<'_>,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    emit_box_fill(ctx, resolve_state_bg(ctx.node, ctx.part, ctx.root_bg), out)
}
fn emit_box_fill(
    ctx: BuiltinPaintCtx<'_>,
    bg: Option<Rgb565>,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    emit_surface(ctx.node, ctx.composition, bg, out)
}
fn emit_surface(
    node: &RetainedNode,
    composition: Option<&crate::composition::SurfaceComposition>,
    bg: Option<Rgb565>,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    let rect = node.layout_rect;
    let clip = node.clip_rect;
    if !rect.intersects(clip) {
        return Ok(());
    }
    let outline = crate::composition::body_outline(node.props.geometry, composition);
    let (rect, radius) = outline.bounds_and_radius(rect, node.props.radius);
    if let crate::composition::Outline::Path(path, rule) = outline {
        if let Some(color) = bg {
            if composition.is_some_and(|c| c.clip_content) {
                out.push(PaintCmd::FillRect { rect, clip, color })?
            } else {
                out.push(PaintCmd::StaticPath {
                    rect,
                    clip,
                    path,
                    paint: crate::path::PathPaint::Fill(rule),
                    color,
                })?
            }
        }
        if let Some(color) = node.props.border_color()
            && node.props.border_width > 0
        {
            emit_path_stroke(out, path, rect, clip, node.props.border_width, color)?;
        }
        return Ok(());
    }
    if radius > 255 {
        if let Some(color) = bg {
            out.push(PaintCmd::WideRound {
                rect,
                clip,
                radius,
                width: 0,
                color,
            })?;
        }
        if let Some(color) = node.props.border_color()
            && node.props.border_width > 0
        {
            out.push(PaintCmd::WideRound {
                rect,
                clip,
                radius,
                width: node.props.border_width,
                color,
            })?;
        }
        return Ok(());
    }
    let radius = radius as u8;
    if let Some(color) = bg {
        if radius > 0
            || (node.props.border_color().is_some()
                && (node.kind == NodeKind::Button || node.props.border_width > 0))
        {
            out.push(PaintCmd::FillRoundedRect {
                rect,
                clip,
                color,
                radius,
                border_color: node.props.border_color(),
                border_width: node.props.border_width,
            })?;
        } else {
            out.push(PaintCmd::FillRect { rect, clip, color })?;
        }
    } else if let Some(bc) = node.props.border_color()
        && node.props.border_width > 0
    {
        if radius > 0 {
            out.push(PaintCmd::StrokeRoundedRect {
                rect,
                clip,
                color: bc,
                width: node.props.border_width,
                radius,
            })?;
        } else {
            out.push(PaintCmd::StrokeRect {
                rect,
                clip,
                color: bc,
                width: node.props.border_width,
            })?;
        }
    }
    Ok(())
}

fn apply_press_feedback(
    color: Rgb565,
    accent: Rgb565,
    feedback: crate::view::PressFeedback,
) -> Rgb565 {
    match feedback {
        crate::view::PressFeedback::Default => darken_or_mix(color, 3),
        crate::view::PressFeedback::None => color,
        crate::view::PressFeedback::Darken(a) => {
            blend_rgb565_scale(color, Rgb565::new(0, 0, 0), i32::from(a.min(15)), 15)
        }
        crate::view::PressFeedback::Lighten(a) => {
            blend_rgb565_scale(color, Rgb565::new(31, 63, 31), i32::from(a.min(15)), 15)
        }
        crate::view::PressFeedback::AccentTint(a) => {
            blend_rgb565_scale(color, accent, i32::from(a.min(15)), 15)
        }
    }
}

fn darken_or_mix(color: Rgb565, amount: u16) -> Rgb565 {
    use embedded_graphics_core::pixelcolor::{IntoStorage, raw::RawU16};
    let raw = color.into_storage();
    let r = ((raw >> 11) & 0x1F).saturating_sub(amount);
    let g = ((raw >> 5) & 0x3F).saturating_sub(amount * 2);
    let b = (raw & 0x1F).saturating_sub(amount);
    Rgb565::from(RawU16::new((r << 11) | (g << 5) | b))
}

pub fn blend_rgb565_scale(bg: Rgb565, fg: Rgb565, num: i32, den: i32) -> Rgb565 {
    use embedded_graphics_core::pixelcolor::{IntoStorage, raw::RawU16};
    if num <= 0 {
        return bg;
    }
    if num >= den {
        return fg;
    }
    let b = bg.into_storage() as i32;
    let f = fg.into_storage() as i32;
    let inv = den - num;

    let br = (b >> 11) & 0x1F;
    let bg_c = (b >> 5) & 0x3F;
    let bb = b & 0x1F;

    let fr = (f >> 11) & 0x1F;
    let fg_c = (f >> 5) & 0x3F;
    let fb = f & 0x1F;

    let rr = (fr * num + br * inv + den / 2) / den;
    let rg = (fg_c * num + bg_c * inv + den / 2) / den;
    let rb = (fb * num + bb * inv + den / 2) / den;

    Rgb565::from(RawU16::new(
        ((rr as u16) << 11) | ((rg as u16) << 5) | (rb as u16),
    ))
}
