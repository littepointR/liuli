use crate::capacity::{BuildError, MAX_COMMANDS, MAX_DEPTH, MAX_NODES};
use crate::geometry::{Point, Rect, Rgb565};
use crate::node::RetainedTree;
use crate::time::{ANIM_SCALE, AnimPool, AnimProp};
use crate::view::{
    Align, BUTTON_ICON_GAP, BUTTON_ICON_SIZE, BUTTON_ICON_SPAN, FontId, IconKind, NodeKind,
    SlotLink, TextAlign,
};

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
    } = 4,
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
}

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
        }
    }

    pub fn bounds(&self) -> Rect {
        match *self {
            Self::Empty => Rect::ZERO,
            Self::Shadow {
                rect, clip, shadow, ..
            } => shadow.bounds(rect).intersect(clip),
            Self::Group { rect, clip, .. }
            | Self::Surface { rect, clip, .. }
            | Self::Bitmap { rect, clip, .. }
            | Self::FillRect { rect, clip, .. }
            | Self::StrokeRect { rect, clip, .. }
            | Self::StrokeRoundedRect { rect, clip, .. }
            | Self::FillRoundedRect { rect, clip, .. }
            | Self::Overlay { rect, clip, .. }
            | Self::Icon { rect, clip, .. }
            | Self::Sparkline { rect, clip, .. } => rect.intersect(clip),
            Self::Text { clip, .. } => clip,
        }
    }
}

pub struct CommandList {
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

    pub fn reset(&mut self) {
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
    fn text(&self, id: TextId) -> &str;
    fn sparkline(&self, series: u8) -> &[i16];
}

impl SceneResources for RetainedTree {
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

/// Coordinates supplied to fill are relative to the widget.
pub struct WidgetCanvas<'a> {
    bounds: Rect,
    clip: Rect,
    commands: &'a mut CommandList,
    pub pressed: bool,
}
impl WidgetCanvas<'_> {
    pub fn size(&self) -> crate::geometry::Size {
        self.bounds.size()
    }
    #[cfg(feature = "owned-images")]
    pub fn surface(
        &mut self,
        rect: Rect,
        source: alloc::sync::Arc<crate::bitmap::OwnedBitmap>,
        angle: u8,
        alpha: u8,
    ) -> Result<(), BuildError> {
        let rect = rect.translate(self.bounds.x, self.bounds.y);
        if alpha == 0 || !rect.intersects(self.clip) {
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
            clip: self.clip,
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
        let rect = Rect::new(
            self.bounds.x.saturating_add(rect.x),
            self.bounds.y.saturating_add(rect.y),
            rect.w,
            rect.h,
        );
        if alpha != 0 && rect.intersects(self.clip) {
            self.commands.push(PaintCmd::Bitmap {
                rect,
                clip: self.clip,
                source,
                angle,
                alpha: alpha.min(15),
            })?;
        }
        Ok(())
    }
    pub fn fill(&mut self, rect: Rect, color: Rgb565) -> Result<(), BuildError> {
        let rect = Rect::new(
            self.bounds.x.saturating_add(rect.x),
            self.bounds.y.saturating_add(rect.y),
            rect.w,
            rect.h,
        )
        .intersect(self.clip);
        if !rect.is_empty() {
            self.commands.push(PaintCmd::FillRect {
                rect,
                clip: self.clip,
                color,
            })?;
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
    mut custom: impl FnMut(
        crate::view::Key,
        &crate::view::StoredProps,
        &mut WidgetCanvas<'_>,
    ) -> Result<(), BuildError>,
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
    #[cfg(feature = "layers")]
    let mut group_at = [None; MAX_DEPTH + 2];
    while sp > 0 {
        let top = sp - 1;
        let frame = stack[top];
        let node = &tree.nodes[frame.slot as usize];
        if !node.occupied || !node.props.visible() {
            sp -= 1;
            continue;
        }

        if frame.stage == 0 {
            let clip = node.clip_rect;
            let rect = node.layout_rect;
            let visible = !rect.intersect(clip).is_empty() && !node.visible_rect.is_empty();

            #[cfg(feature = "layers")]
            let backdrop_end = out.count;
            let effect = tree.effects.get(node.key);
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
                    out.push(PaintCmd::Shadow {
                        rect,
                        clip,
                        shadow,
                        projection,
                        radius: node.props.radius,
                    })?;
                }
            }
            #[cfg(feature = "layers")]
            {
                group_at[top] = None;
                if visible && let Some(layer) = effect.layer {
                    out.effect_error_key = Some(node.key);
                    group_at[top] = Some(out.begin_layer(
                        node.key,
                        rect,
                        clip,
                        layer,
                        node.props.radius,
                        backdrop_end,
                    )?);
                    out.effect_error_key = None;
                }
            }
            if visible || node.kind == NodeKind::Modal {
                match node.kind {
                    NodeKind::Custom => {
                        let mut canvas = WidgetCanvas {
                            bounds: rect,
                            clip: clip.intersect(rect),
                            commands: out,
                            pressed: node.pressed,
                        };
                        custom(node.key, &node.props, &mut canvas)?;
                    }
                    NodeKind::Root => {
                        if let Some(bg) = node.props.bg() {
                            out.push(PaintCmd::FillRect {
                                rect,
                                clip,
                                color: bg,
                            })?;
                        }
                    }
                    NodeKind::Row | NodeKind::Column | NodeKind::Stack | NodeKind::ScrollY => {
                        emit_box_bg_and_border(node, rect, clip, out)?;
                    }
                    NodeKind::Modal => {
                        if node.props.overlay_alpha > 0 {
                            out.push(PaintCmd::Overlay {
                                rect: screen,
                                clip: screen,
                                color: node.props.overlay_color,
                                alpha: node.props.overlay_alpha.min(15),
                            })?;
                        }
                        emit_box_bg_and_border(node, rect, clip, out)?;
                    }
                    NodeKind::Label => {
                        emit_box_bg_and_border(node, rect, clip, out)?;
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
                        let mut text_clip = clip;
                        if node.text_cache.valid {
                            let text_size = node.text_cache.size;
                            if node.props.align_cross == Align::Center && inner.h > text_size.height
                            {
                                text_y += ((inner.h - text_size.height) / 2) as i32;
                            } else if node.props.align_cross == Align::End
                                && inner.h > text_size.height
                            {
                                text_y += (inner.h - text_size.height) as i32;
                            }
                            if node.text_cache.align == align
                                && (align == TextAlign::Start
                                    || node.text_cache.wrap_width == inner.w)
                            {
                                text_clip = clip
                                    .intersect(node.text_cache.rel_ink.translate(text_x, text_y));
                            }
                        }
                        if !text_clip.is_empty() {
                            out.push(PaintCmd::text(
                                Point::new(text_x, text_y),
                                text_clip,
                                TextId(frame.slot),
                                node.props.fg,
                                inner.w,
                                node.props.max_lines,
                                node.props.ellipsis(),
                                font,
                                align,
                            ))?;
                        }
                    }
                    NodeKind::Button => {
                        let bg = if node.pressed {
                            match (node.props.bg(), node.props.press_feedback()) {
                                (Some(c), fb) => {
                                    Some(apply_press_feedback(c, node.props.accent, fb))
                                }
                                (None, crate::view::PressFeedback::None) => None,
                                (None, fb) => {
                                    Some(apply_press_feedback(out.root_bg, node.props.accent, fb))
                                }
                            }
                        } else {
                            node.props.bg()
                        };
                        if let Some(bg_col) = bg {
                            if node.props.radius > 0 || node.props.border_color().is_some() {
                                out.push(PaintCmd::FillRoundedRect {
                                    rect,
                                    clip,
                                    color: bg_col,
                                    radius: node.props.radius,
                                    border_color: node.props.border_color(),
                                    border_width: node.props.border_width,
                                })?;
                            } else {
                                out.push(PaintCmd::FillRect {
                                    rect,
                                    clip,
                                    color: bg_col,
                                })?;
                            }
                        } else if let Some(bc) = node.props.border_color()
                            && node.props.border_width > 0
                        {
                            if node.props.radius > 0 {
                                out.push(PaintCmd::StrokeRoundedRect {
                                    rect,
                                    clip,
                                    color: bc,
                                    width: node.props.border_width,
                                    radius: node.props.radius,
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

                        let inner = rect.inset(
                            node.props.padding.left as u32,
                            node.props.padding.top as u32,
                            node.props.padding.right as u32,
                            node.props.padding.bottom as u32,
                        );
                        let has_icon = node.props.icon != IconKind::None;
                        let icon_w = if has_icon { BUTTON_ICON_SIZE } else { 0 };
                        let gap_w = if has_icon { BUTTON_ICON_GAP } else { 0 };
                        let icon_span = if has_icon { BUTTON_ICON_SPAN } else { 0 };
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
                        let mut text_y = inner.y;
                        if inner.h > text_h {
                            text_y = inner.y + ((inner.h - text_h) / 2) as i32;
                        }
                        let wrap_w = inner.w.saturating_sub(icon_span);
                        let align = node.props.text_align();
                        let font = node.props.font();
                        let text_x;
                        let cmd_align;
                        if align == TextAlign::Start {
                            let total_w = icon_w + gap_w + text_w;
                            let start_x = if inner.w > total_w {
                                inner.x + ((inner.w - total_w) / 2) as i32
                            } else {
                                inner.x
                            };
                            if has_icon {
                                let iy = inner.y
                                    + ((inner.h.saturating_sub(BUTTON_ICON_SIZE)) / 2) as i32;
                                out.push(PaintCmd::Icon {
                                    rect: Rect::new(
                                        start_x,
                                        iy,
                                        BUTTON_ICON_SIZE,
                                        BUTTON_ICON_SIZE,
                                    ),
                                    clip,
                                    kind: node.props.icon,
                                    color: node.props.fg,
                                })?;
                                text_x = start_x + BUTTON_ICON_SPAN as i32;
                            } else {
                                text_x = start_x;
                            }
                            cmd_align = TextAlign::Start;
                        } else {
                            if has_icon {
                                let iy = inner.y
                                    + ((inner.h.saturating_sub(BUTTON_ICON_SIZE)) / 2) as i32;
                                out.push(PaintCmd::Icon {
                                    rect: Rect::new(
                                        inner.x,
                                        iy,
                                        BUTTON_ICON_SIZE,
                                        BUTTON_ICON_SIZE,
                                    ),
                                    clip,
                                    kind: node.props.icon,
                                    color: node.props.fg,
                                })?;
                            }
                            text_x = inner.x + icon_span as i32;
                            cmd_align = align;
                        }
                        let text_clip = if node.text_cache.valid
                            && node.text_cache.align == cmd_align
                            && (cmd_align == TextAlign::Start
                                || node.text_cache.wrap_width == wrap_w)
                        {
                            clip.intersect(node.text_cache.rel_ink.translate(text_x, text_y))
                        } else {
                            clip
                        };
                        if !text_clip.is_empty() {
                            out.push(PaintCmd::text(
                                Point::new(text_x, text_y),
                                text_clip,
                                TextId(frame.slot),
                                node.props.fg,
                                wrap_w,
                                node.props.max_lines.max(1),
                                node.props.ellipsis(),
                                font,
                                cmd_align,
                            ))?;
                        }
                    }
                    NodeKind::Toggle => {
                        if !rect.is_empty() {
                            let track_w = rect.w.min(48);
                            let track_h = rect.h.min(26);
                            let track_x = rect.x + ((rect.w - track_w) / 2) as i32;
                            let track_y = rect.y + ((rect.h - track_h) / 2) as i32;
                            let track_rect = Rect::new(track_x, track_y, track_w, track_h);

                            let default_progress =
                                if node.props.checked() { ANIM_SCALE } else { 0 };
                            let progress = anims
                                .cached_value(node.key, AnimProp::ToggleKnob)
                                .unwrap_or(default_progress)
                                .clamp(0, ANIM_SCALE);

                            let track_off = node.props.bg().unwrap_or(Rgb565::new(10, 20, 10));
                            let track_on = node.props.accent;
                            let track_color =
                                blend_rgb565_scale(track_off, track_on, progress, ANIM_SCALE);

                            let r = (track_h / 2) as u8;
                            out.push(PaintCmd::FillRoundedRect {
                                rect: track_rect,
                                clip,
                                color: track_color,
                                radius: r,
                                border_color: node.props.border_color(),
                                border_width: node.props.border_width,
                            })?;

                            let inset = (track_h / 4).min(2);
                            let knob_size = track_h.saturating_sub(inset * 2).min(track_w);
                            if knob_size > 0 {
                                let travel = track_w
                                    .saturating_sub(knob_size.saturating_add(inset * 2))
                                    as i32;
                                let knob_x = track_x
                                    + inset as i32
                                    + ((travel * progress + ANIM_SCALE / 2) / ANIM_SCALE);
                                let knob_y =
                                    track_y + ((track_h.saturating_sub(knob_size)) / 2) as i32;
                                let knob_rect = Rect::new(knob_x, knob_y, knob_size, knob_size);
                                out.push(PaintCmd::FillRoundedRect {
                                    rect: knob_rect,
                                    clip,
                                    color: node.props.fg,
                                    radius: (knob_size / 2) as u8,
                                    border_color: None,
                                    border_width: 0,
                                })?;
                            }
                        }
                    }
                    NodeKind::Slider => {
                        if !rect.is_empty() {
                            let track_h = rect.h.min(6).max(1);
                            let knob_w = rect.w.min(12);
                            let knob_h = rect.h.min(20);
                            let pad_x = (knob_w / 2) as i32;
                            let usable_w = rect.w.saturating_sub(knob_w);
                            let track_x = rect.x + pad_x;
                            let track_y = rect.y + ((rect.h.saturating_sub(track_h)) / 2) as i32;
                            let track_bg = node.props.bg().unwrap_or(Rgb565::new(10, 20, 10));

                            let min = node.props.min as i64;
                            let max = node.props.max as i64;
                            let span = (max - min).max(1);
                            let clamped = (node.props.value as i64).clamp(min, max.max(min));
                            let rel = (clamped - min).max(0);
                            let fill_w = ((usable_w as i64 * rel) / span) as u32;

                            if usable_w > 0 {
                                let track_rect = Rect::new(track_x, track_y, usable_w, track_h);
                                out.push(PaintCmd::FillRoundedRect {
                                    rect: track_rect,
                                    clip,
                                    color: track_bg,
                                    radius: (track_h / 2) as u8,
                                    border_color: None,
                                    border_width: 0,
                                })?;

                                if fill_w > 0 {
                                    out.push(PaintCmd::FillRoundedRect {
                                        rect: Rect::new(track_x, track_y, fill_w, track_h),
                                        clip,
                                        color: node.props.accent,
                                        radius: (track_h / 2) as u8,
                                        border_color: None,
                                        border_width: 0,
                                    })?;
                                }
                            }

                            if knob_w > 0 && knob_h > 0 {
                                let knob_x = if usable_w > 0 {
                                    track_x + fill_w as i32 - pad_x
                                } else {
                                    rect.x + ((rect.w.saturating_sub(knob_w)) / 2) as i32
                                };
                                let knob_y = rect.y + ((rect.h.saturating_sub(knob_h)) / 2) as i32;
                                let is_editing =
                                    focus.editing && focus.focused_slot == Some(frame.slot);
                                out.push(PaintCmd::FillRoundedRect {
                                    rect: Rect::new(knob_x, knob_y, knob_w, knob_h),
                                    clip,
                                    color: node.props.fg,
                                    radius: 4u8.min((knob_h / 2) as u8),
                                    border_color: Some(if is_editing {
                                        node.props.focus_color
                                    } else {
                                        node.props.accent
                                    }),
                                    border_width: if is_editing { 2 } else { 1 },
                                })?;
                            }
                        }
                    }
                    NodeKind::ProgressBar => {
                        if !rect.is_empty() && !rect.intersect(clip).is_empty() {
                            let track_bg = node.props.bg().unwrap_or(Rgb565::new(10, 20, 10));
                            let radius = node.props.radius.min((rect.h / 2).min(255) as u8);
                            let border_color = node.props.border_color();
                            let border_width = node.props.border_width;
                            out.push(PaintCmd::FillRoundedRect {
                                rect,
                                clip,
                                color: track_bg,
                                radius,
                                border_color,
                                border_width,
                            })?;

                            let bw = if border_color.is_some() && border_width > 0 {
                                border_width as u32
                            } else {
                                0
                            };
                            let inner = rect.inset(bw, bw, bw, bw);
                            if !inner.is_empty() {
                                let min = node.props.min as i64;
                                let max = node.props.max as i64;
                                let fill_w = if min >= max {
                                    inner.w
                                } else {
                                    let clamped = (node.props.value as i64).clamp(min, max);
                                    let span = max - min;
                                    (((inner.w as i64) * (clamped - min) + span / 2) / span)
                                        .clamp(0, inner.w as i64)
                                        as u32
                                };
                                if fill_w > 0 {
                                    let fill_rect = Rect::new(inner.x, inner.y, fill_w, inner.h);
                                    let fill_r = radius
                                        .saturating_sub(bw as u8)
                                        .min((fill_rect.h / 2).min(255) as u8)
                                        .min((fill_rect.w / 2).min(255) as u8);
                                    out.push(PaintCmd::FillRoundedRect {
                                        rect: fill_rect,
                                        clip: clip.intersect(inner),
                                        color: node.props.accent,
                                        radius: fill_r,
                                        border_color: None,
                                        border_width: 0,
                                    })?;
                                }
                            }
                        }
                    }
                    NodeKind::Sparkline => {
                        emit_box_bg_and_border(node, rect, clip, out)?;
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
                                color: node.props.accent,
                                series: node.props.series,
                                min: node.props.min,
                                max: node.props.max,
                                stroke_width: 1,
                            })?;
                        }
                    }
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
                stack[sp] = PaintDepthFrame {
                    slot: ch_idx as u16,
                    stage: 0,
                    next_child: SlotLink::NONE,
                };
                sp += 1;
                continue;
            }

            // Stage post-children: ScrollY scrollbar & focus ring, then pop frame
            let clip = node.clip_rect;
            let rect = node.layout_rect;

            if node.kind == NodeKind::ScrollY
                && node.content_height > rect.h
                && rect.h > 8
                && rect.w > 6
            {
                let track_h = rect.h.saturating_sub(4);
                let min_thumb = 12.min(track_h) as u64;
                let thumb_h = ((track_h as u64 * rect.h as u64) / node.content_height as u64)
                    .clamp(min_thumb, track_h as u64) as u32;
                let max_scroll = node.content_height.saturating_sub(rect.h).max(1);
                let thumb_travel = track_h.saturating_sub(thumb_h);
                let thumb_y = rect.y
                    + 2
                    + ((thumb_travel as u64 * (node.scroll_y.max(0) as u64)) / max_scroll as u64)
                        as i32;
                let thumb_rect = Rect::new(rect.x + rect.w as i32 - 4, thumb_y, 3, thumb_h);
                if !thumb_rect.intersect(clip).is_empty() {
                    out.push(PaintCmd::FillRoundedRect {
                        rect: thumb_rect,
                        clip,
                        color: node.props.accent,
                        radius: 1,
                        border_color: None,
                        border_width: 0,
                    })?;
                }
            }

            if focus.ring_visible && focus.focused_slot == Some(frame.slot) {
                // Paint above adjacent controls and scrollbars in this flow, but below
                // the next overlay sibling (for example a toast or a modal).
                let mut layer = frame.slot;
                while let Some(parent) = tree.nodes[usize::from(layer)].parent.get_u16() {
                    if tree
                        .effects
                        .get(tree.nodes[usize::from(layer)].key)
                        .layer()
                        .is_some()
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
            if outline_layer == Some(frame.slot)
                && let Some(slot) = focus.focused_slot
            {
                let focused = &tree.nodes[usize::from(slot)];
                emit_focus_outline(focused, screen, focus.editing, out)?;
                outline_layer = None;
            }
            #[cfg(feature = "layers")]
            if let Some(start) = group_at[top].take() {
                let end = out.count as u16;
                if let PaintCmd::Group { end: limit, .. } = &mut out.cmds[start] {
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

fn emit_focus_outline(
    node: &crate::node::RetainedNode,
    screen: Rect,
    editing: bool,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    let ring = node.props.focus_ring();
    if !ring.enabled {
        return Ok(());
    }
    let rect = node.layout_rect;
    let ring_clip = node.clip_rect.intersect(screen);
    if !rect.intersect(ring_clip).is_empty() {
        // Fit the outward stroke on sides that reach a container edge. Keeping
        // the widget's full bounds avoids inventing a border at a scroll cut.
        let offset = u32::from(ring.offset);
        let width = ring.width.clamp(1, 3);
        let ring_rect = rect.union(rect.expand(offset).intersect(ring_clip));
        let use_rounded = ring.rounded && node.props.radius > 0;
        let ring_radius = node.props.radius.saturating_add(ring.offset);
        if use_rounded {
            out.push(PaintCmd::StrokeRoundedRect {
                rect: ring_rect,
                clip: ring_clip,
                color: node.props.focus_color,
                width,
                radius: ring_radius,
            })?;
        } else {
            out.push(PaintCmd::StrokeRect {
                rect: ring_rect,
                clip: ring_clip,
                color: node.props.focus_color,
                width,
            })?;
        }
        if editing {
            let inner_rect = ring_rect.inset(1, 1, 1, 1);
            if use_rounded {
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
    node: &crate::node::RetainedNode,
    rect: Rect,
    clip: Rect,
    out: &mut CommandList,
) -> Result<(), BuildError> {
    if rect.intersect(clip).is_empty() {
        return Ok(());
    }
    let bg = if node.pressed {
        node.props
            .bg()
            .map(|c| apply_press_feedback(c, node.props.accent, node.props.press_feedback()))
    } else {
        node.props.bg()
    };
    if let Some(color) = bg {
        if node.props.radius > 0
            || (node.props.border_color().is_some() && node.props.border_width > 0)
        {
            out.push(PaintCmd::FillRoundedRect {
                rect,
                clip,
                color,
                radius: node.props.radius,
                border_color: node.props.border_color(),
                border_width: node.props.border_width,
            })?;
        } else {
            out.push(PaintCmd::FillRect { rect, clip, color })?;
        }
    } else if let Some(bc) = node.props.border_color()
        && node.props.border_width > 0
    {
        if node.props.radius > 0 {
            out.push(PaintCmd::StrokeRoundedRect {
                rect,
                clip,
                color: bc,
                width: node.props.border_width,
                radius: node.props.radius,
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
