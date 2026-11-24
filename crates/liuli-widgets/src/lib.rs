#![no_std]
#![forbid(unsafe_code)]

pub mod controls;
pub mod style;
pub mod theme;
pub mod tokens;
pub mod ui;

use liuli::prelude::*;

pub use controls::{
    arc_knob, arc_knob_from_pointer, badge_button, paint_arc_knob, paint_ticked_slider,
};
pub use style::{ButtonVariant, Compositor, Elevation, Material, ResolvedSurface, SurfaceSpec};
pub use theme::{Style, Theme, ThemePreset};
pub use tokens::{
    ColorScheme, ComponentParts, Density, EffectTier, EffectsConfig, ElevationLevel,
    ElevationScale, GlassStyle, InteractionStyle, Motion, Shape, Typography, contrast_ratio_x10,
    rgb8,
};
pub use ui::ThemedUi;

/// Typed route stack. Route variants carry parameters and per-page state.
/// The root always remains present; a failed push gives ownership back.
pub struct Navigator<R: Copy, const N: usize> {
    root: R,
    pages: [Option<R>; N],
    len: usize,
}
impl<R: Copy, const N: usize> Navigator<R, N> {
    pub const fn new(root: R) -> Self {
        Self {
            root,
            pages: [None; N],
            len: 0,
        }
    }
    pub fn current(&self) -> R {
        self.len
            .checked_sub(1)
            .and_then(|i| self.pages[i])
            .unwrap_or(self.root)
    }
    pub fn push(&mut self, page: R) -> Result<(), R> {
        let Some(slot) = self.pages.get_mut(self.len) else {
            return Err(page);
        };
        *slot = Some(page);
        self.len += 1;
        Ok(())
    }
    pub fn pop(&mut self) -> Option<R> {
        self.len = self.len.checked_sub(1)?;
        self.pages[self.len].take()
    }
}

/// A typed result delivered to the page revealed by a successful pop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteReturn<R, T> {
    pub from: R,
    pub to: R,
    pub value: T,
}
impl<R: Copy, const N: usize> Navigator<R, N> {
    pub fn pop_with<T>(&mut self, value: T) -> Result<RouteReturn<R, T>, T> {
        let Some(from) = self.pop() else {
            return Err(value);
        };
        Ok(RouteReturn {
            from,
            to: self.current(),
            value,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateCapacity {
    pub page: u8,
    pub key: Key,
}
/// Bounded component-local state kept in the application model across rebuilds.
pub struct KeyedState<T, const N: usize> {
    entries: [Option<(u8, Key, T)>; N],
}
impl<T, const N: usize> KeyedState<T, N> {
    pub const fn new() -> Self {
        Self {
            entries: [const { None }; N],
        }
    }
    pub fn get_or_insert(
        &mut self,
        page: u8,
        key: Key,
        new: impl FnOnce() -> T,
    ) -> Result<&mut T, StateCapacity> {
        let index = self
            .entries
            .iter()
            .position(|e| e.as_ref().is_some_and(|(p, k, _)| *p == page && *k == key))
            .or_else(|| self.entries.iter().position(Option::is_none))
            .ok_or(StateCapacity { page, key })?;
        Ok(&mut self.entries[index]
            .get_or_insert_with(|| (page, key, new()))
            .2)
    }
    pub fn retain(&mut self, mut keep: impl FnMut(u8, Key) -> bool) {
        for e in &mut self.entries {
            if e.as_ref().is_some_and(|(page, key, _)| !keep(*page, *key)) {
                *e = None;
            }
        }
    }
}
impl<T, const N: usize> Default for KeyedState<T, N> {
    fn default() -> Self {
        Self::new()
    }
}
/// A fixed-height list reserves the offscreen extent with two spacers.
/// Item keys must be stable and distinct from the three container keys.
pub fn virtual_list(
    ui: &mut Children<'_, '_>,
    keys: [Key; 3],
    count: u32,
    row_height: core::num::NonZeroU16,
    offset: i32,
    viewport: u16,
    mut row: impl FnMut(&mut Children<'_, '_>, u32) -> Result<(), BuildError>,
) -> Result<(), BuildError> {
    let h = u32::from(row_height.get());
    let extent = count
        .checked_mul(h)
        .filter(|v| *v <= i32::MAX as u32)
        .ok_or(BuildError::InvalidRange)?;
    let y = (offset.max(0) as u32).min(extent.saturating_sub(viewport.into()));
    let first = (y / h).saturating_sub(1);
    let end = (y + u32::from(viewport))
        .div_ceil(h)
        .saturating_add(1)
        .min(count);
    let props = NodeProps {
        width: Length::Fill(1),
        height: Length::Px(viewport.into()),
        align_cross: Align::Stretch,
        ..NodeProps::new()
    };
    ui.scroll_y(keys[0], props, |list| {
        let spacer = |height| NodeProps {
            height: Length::Px(height),
            width: Length::Px(1),
            ..NodeProps::new()
        };
        list.label(keys[1], spacer(first * h))?;
        for index in first..end {
            row(list, index)?;
        }
        list.label(keys[2], spacer((count - end) * h))
    })
}

/// A threshold meter rendered entirely through the public custom-widget API.
pub fn paint_meter(props: &StoredProps, canvas: &mut WidgetCanvas<'_>) -> Result<(), BuildError> {
    let size = canvas.size();
    canvas.fill(Rect::new(0, 0, size.width, size.height), props.bg_raw)?;
    let range = i32::from(props.max) - i32::from(props.min);
    if range > 0 {
        let filled = (u64::from(size.width)
            * (i32::from(props.value) - i32::from(props.min)).clamp(0, range) as u64
            / range as u64) as u32;
        canvas.fill(Rect::new(0, 0, filled, size.height), props.accent)?;
    }
    Ok(())
}

/// Numeric editing via touch drag or rotary steps. Actions carry the clamped value.
pub fn number_editor(
    ui: &mut Children<'_, '_>,
    key: Key,
    range: core::ops::RangeInclusive<i16>,
    value: i16,
    step: i16,
    theme: Theme,
) -> Result<(), BuildError> {
    let (min, max) = range.into_inner();
    if min > max || step <= 0 {
        return Err(BuildError::InvalidRange);
    }
    ui.slider(
        key,
        NodeProps {
            min,
            max,
            step,
            ..theme.slider(value.clamp(min, max))
        },
    )
}
/// A mutually exclusive set of buttons. Each action uses the caller's stable key.
pub fn selector(
    ui: &mut Children<'_, '_>,
    key: Key,
    items: &[(Key, TextRef)],
    selected: Key,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.column(
        key,
        NodeProps {
            width: Length::Fill(1),
            gap: theme.spacing,
            ..NodeProps::new()
        },
        |choices| choice_buttons(choices, items, selected, theme),
    )
}
pub fn tabs(
    ui: &mut Children<'_, '_>,
    key: Key,
    items: &[(Key, TextRef)],
    selected: Key,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.row(
        key,
        NodeProps {
            width: Length::Fill(1),
            gap: theme.spacing,
            ..NodeProps::new()
        },
        |bar| choice_buttons(bar, items, selected, theme),
    )
}
fn choice_buttons(
    ui: &mut Children<'_, '_>,
    items: &[(Key, TextRef)],
    selected: Key,
    theme: Theme,
) -> Result<(), BuildError> {
    for &(key, text) in items {
        let chosen = key == selected;
        let base = if chosen {
            theme.primary_button()
        } else {
            theme.control()
        };
        ui.button(
            key,
            NodeProps {
                text,
                checked: chosen,
                ..base
            },
        )?;
    }
    Ok(())
}
/// Modal focus and Back are managed by the engine. The application handles the two action keys.
pub fn confirm_dialog(
    ui: &mut Children<'_, '_>,
    keys: [Key; 4],
    labels: [TextRef; 2],
    theme: Theme,
) -> Result<(), BuildError> {
    ui.modal(
        keys[0],
        NodeProps {
            width: Length::Px(180),
            height: Length::Content,
            padding: EdgeInsets::all(8),
            bg: Some(theme.surface),
            border_color: Some(theme.border),
            border_width: theme.shape.border_width,
            radius: theme.radius,
            overlay_color: theme.overlay,
            overlay_alpha: theme.interaction.modal_scrim_alpha,
            ..NodeProps::new()
        },
        |modal| confirm_dialog_actions(modal, keys, labels, theme),
    )
}
/// Centered modal confirmation dialog covering the viewport with a backdrop scrim.
pub fn centered_confirm_dialog(
    ui: &mut Children<'_, '_>,
    keys: [Key; 4],
    labels: [TextRef; 2],
    theme: Theme,
) -> Result<(), BuildError> {
    let panel = theme.resolve_surface(
        SurfaceSpec::FLOATING.with_radius(theme.shape.radius_lg),
        NodeProps {
            width: Length::Px(204),
            height: Length::Content,
            padding: EdgeInsets::all(theme.density.card_pad.saturating_add(2)),
            gap: theme.spacing,
            align_cross: Align::Stretch,
            ..NodeProps::new()
        },
    );
    ui.modal(
        keys[0],
        NodeProps {
            width: Length::Fill(1),
            height: Length::Fill(1),
            overlay_color: theme.overlay,
            overlay_alpha: theme.interaction.modal_scrim_alpha,
            ..NodeProps::new()
        },
        |modal| {
            modal.column(keys[1], panel.props, |actions| {
                actions.button(
                    keys[2],
                    NodeProps {
                        text: labels[0],
                        ..theme.primary_button()
                    },
                )?;
                actions.button(
                    keys[3],
                    NodeProps {
                        text: labels[1],
                        ..theme.secondary_button()
                    },
                )
            })?;
            if panel.effect.shadow.is_some() || panel.effect.layer().is_some() {
                modal.decorate(keys[1], panel.effect)?;
            }
            Ok(())
        },
    )
}
fn confirm_dialog_actions(
    modal: &mut Children<'_, '_>,
    keys: [Key; 4],
    labels: [TextRef; 2],
    theme: Theme,
) -> Result<(), BuildError> {
    modal.column(
        keys[1],
        NodeProps {
            width: Length::Fill(1),
            gap: theme.spacing,
            ..NodeProps::new()
        },
        |actions| {
            actions.button(
                keys[2],
                NodeProps {
                    text: labels[0],
                    ..theme.primary_button()
                },
            )?;
            actions.button(
                keys[3],
                NodeProps {
                    text: labels[1],
                    ..theme.secondary_button()
                },
            )
        },
    )
}
/// Insert last in an overlay stack. The application owns the dismissal deadline.
pub fn toast(
    ui: &mut Children<'_, '_>,
    key: Key,
    text: TextRef,
    theme: Theme,
) -> Result<(), BuildError> {
    ui.label(
        key,
        NodeProps {
            text,
            width: Length::Fill(1),
            height: Length::Px(theme.control_height.into()),
            padding: EdgeInsets::symmetric(
                theme.density.pad_x.saturating_add(2),
                theme.density.pad_y,
            ),
            bg: Some(theme.surface_alt),
            border_color: Some(theme.accent),
            border_width: theme.shape.border_width,
            radius: theme.radius,
            fg: theme.text,
            font: theme.font,
            align_cross: Align::Center,
            text_align: TextAlign::Center,
            ..NodeProps::new()
        },
    )
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicI16, Ordering};

    use liuli_sim::screen::MemoryScreen;

    use super::*;

    const TEST_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");

    #[test]
    fn component_parts_propagate_via_themed_ui_without_wasting_default_slots() {
        let mut draft = liuli::core::view::ViewDraft::new();
        let mut vb = ViewBuilder::new(&mut draft);
        vb.root(Key(1), NodeProps::new(), |root| {
            let mut ui = ThemedUi::new(root, Theme::DARK);
            ui.slider(Key(2), 40)?;
            let custom_parts = ComponentParts::DEFAULT
                .with_slider(
                    WidgetPartStyle::new()
                        .with_track_color(rgb8(30, 40, 50))
                        .with_knob_radius(7),
                )
                .with_button(WidgetPartStyle::new().with_pressed_bg(rgb8(200, 20, 20)));
            ui.with_parts(custom_parts, |scoped| {
                scoped.slider(Key(3), 75)?;
                scoped.button(Key(4), "Apply", IconKind::Check, ButtonVariant::Primary)
            })
        })
        .unwrap();
        vb.finish().unwrap();

        assert!(draft.part_styles.get(Key(2)).is_empty());
        assert_eq!(draft.part_styles.get(Key(3)).knob_radius(), Some(7));
        assert_eq!(
            draft.part_styles.get(Key(3)).track_color(),
            Some(rgb8(30, 40, 50))
        );
        assert_eq!(
            draft.part_styles.get(Key(4)).pressed_bg(),
            Some(rgb8(200, 20, 20))
        );
    }

    #[test]
    fn arc_knob_pointer_mapping_handles_angles_deadzone_and_steps() {
        let sz = Size::new(100, 100);
        let ev = |x, y| PointerEvent {
            phase: PointerPhase::Move,
            local: Point::new(x, y),
            delta: Point::zero(),
            size: sz,
        };
        assert_eq!(arc_knob_from_pointer(ev(15, 85), 0, 100, 1), 0);
        assert_eq!(arc_knob_from_pointer(ev(50, 10), 0, 100, 1), 50);
        assert_eq!(arc_knob_from_pointer(ev(85, 85), 0, 100, 1), 100);
        assert_eq!(arc_knob_from_pointer(ev(45, 95), 0, 100, 1), 0);
        assert_eq!(arc_knob_from_pointer(ev(55, 95), 0, 100, 1), 100);
        assert_eq!(arc_knob_from_pointer(ev(50, 10), 0, 100, 25), 50);
        assert_eq!(arc_knob_from_pointer(ev(90, 50), 0, 100, 25), 75);
    }

    #[derive(Clone, Copy)]
    struct ControlsDemoModel {
        knob: i16,
        slider: i16,
        clicks: u16,
    }

    #[derive(Clone, Copy)]
    enum ControlsDemoMsg {
        SetKnob(i16),
        SetSlider(i16),
        ClickBadge,
    }

    static LAST_BADGE_HITS: AtomicI16 = AtomicI16::new(0);

    struct ControlsDemoApp;

    impl App for ControlsDemoApp {
        type Model = ControlsDemoModel;
        type Msg = ControlsDemoMsg;

        fn update(model: &mut Self::Model, msg: Self::Msg, _now: InstantMs) -> Change {
            match msg {
                ControlsDemoMsg::SetKnob(v) => model.knob = v.clamp(0, 100),
                ControlsDemoMsg::SetSlider(v) => model.slider = v.clamp(0, 100),
                ControlsDemoMsg::ClickBadge => {
                    model.clicks += 1;
                    LAST_BADGE_HITS.store(model.clicks as i16, Ordering::Relaxed);
                }
            }
            Change::View
        }

        fn view(model: &Self::Model, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
            let theme = Theme::DARK;
            out.root(
                Key(1),
                NodeProps {
                    bg: Some(theme.background),
                    width: Length::Fill(1),
                    height: Length::Fill(1),
                    padding: EdgeInsets::all(8),
                    gap: 8,
                    ..NodeProps::new()
                },
                |ui| {
                    ui.column(
                        Key(2),
                        NodeProps {
                            width: Length::Fill(1),
                            height: Length::Content,
                            gap: 8,
                            ..NodeProps::new()
                        },
                        |col| {
                            arc_knob(
                                col,
                                Key(10),
                                64,
                                0..=100,
                                model.knob,
                                5,
                                TextRef::Static("Vol"),
                                theme,
                            )?;
                            col.slider(
                                Key(11),
                                NodeProps {
                                    paint_mode: PaintMode::Override,
                                    ..theme.slider(model.slider)
                                },
                            )?;
                            badge_button(
                                col,
                                [Key(12), Key(13), Key(14)],
                                TextRef::Static("Alerts"),
                                TextRef::Static("3"),
                                theme.danger,
                                theme,
                            )
                        },
                    )
                },
            )
        }

        fn paint_custom(
            key: Key,
            props: &StoredProps,
            canvas: &mut WidgetCanvas<'_>,
        ) -> Result<(), BuildError> {
            if key == Key(10) {
                return paint_arc_knob(
                    props,
                    PaintState {
                        pressed: canvas.pressed,
                        focused: false,
                        editing: false,
                        checked: props.checked(),
                        enabled: props.enabled(),
                        anim_progress: 0,
                    },
                    canvas,
                );
            }
            Ok(())
        }

        fn paint_override(
            _model: &Self::Model,
            key: Key,
            _kind: NodeKind,
            props: &StoredProps,
            state: PaintState,
            canvas: &mut WidgetCanvas<'_>,
        ) -> Result<PaintOverride, BuildError> {
            if key == Key(11) {
                return paint_ticked_slider(props, state, 5, canvas);
            }
            Ok(PaintOverride::Default)
        }

        fn on_pointer(_model: &Self::Model, key: Key, event: PointerEvent) -> Option<Self::Msg> {
            if key == Key(10) && matches!(event.phase, PointerPhase::Down | PointerPhase::Move) {
                return Some(ControlsDemoMsg::SetKnob(arc_knob_from_pointer(
                    event, 0, 100, 5,
                )));
            }
            None
        }

        fn on_action(_model: &Self::Model, event: UiAction) -> Option<Self::Msg> {
            match (event.key, event.kind) {
                (Key(11), ActionKind::SetValue(v)) => Some(ControlsDemoMsg::SetSlider(v)),
                (Key(13), ActionKind::Activate) => Some(ControlsDemoMsg::ClickBadge),
                _ => None,
            }
        }
    }

    #[test]
    fn arc_knob_ticked_slider_and_badge_button_render_and_interact() {
        LAST_BADGE_HITS.store(0, Ordering::Relaxed);
        let font = Lf4Font::from_bytes(TEST_LF4).unwrap();
        let screen_rect = Rect::new(0, 0, 240, 200);
        let mut storage = Storage::new();
        let mut engine = Engine::<ControlsDemoApp>::new_in_place(
            &mut storage,
            ControlsDemoModel {
                knob: 25,
                slider: 50,
                clicks: 0,
            },
            font,
            screen_rect,
            InstantMs(0),
        )
        .unwrap();

        let mut mem = MemoryScreen::new(240, 200);
        let mut scratch = [0u16; 240 * 8];
        engine.present_sync(&mut mem, &mut scratch).unwrap();

        let knob_rect = engine
            .active_tree()
            .node_by_key(Key(10))
            .unwrap()
            .layout_rect;
        let badge_btn_rect = engine
            .active_tree()
            .node_by_key(Key(13))
            .unwrap()
            .layout_rect;
        let badge_lbl_rect = engine
            .active_tree()
            .node_by_key(Key(14))
            .unwrap()
            .layout_rect;

        assert!(badge_lbl_rect.y < badge_btn_rect.y);
        assert!(badge_lbl_rect.x > badge_btn_rect.x);

        let top_x = knob_rect.x + (knob_rect.w as i32 / 2);
        let top_y = knob_rect.y + 4;
        engine
            .dispatch(
                RawInput::PointerDown {
                    id: PointerId(1),
                    pos: Point::new(top_x, top_y),
                },
                InstantMs(10),
                false,
            )
            .unwrap();
        engine
            .dispatch(
                RawInput::PointerUp {
                    id: PointerId(1),
                    pos: Point::new(top_x, top_y),
                },
                InstantMs(20),
                false,
            )
            .unwrap();
        assert_eq!(engine.model().knob, 50);

        let bx = badge_btn_rect.x + 20;
        let by = badge_btn_rect.y + (badge_btn_rect.h as i32 / 2);
        engine
            .dispatch(
                RawInput::PointerDown {
                    id: PointerId(2),
                    pos: Point::new(bx, by),
                },
                InstantMs(30),
                false,
            )
            .unwrap();
        engine
            .dispatch(
                RawInput::PointerUp {
                    id: PointerId(2),
                    pos: Point::new(bx, by),
                },
                InstantMs(40),
                false,
            )
            .unwrap();
        assert_eq!(LAST_BADGE_HITS.load(Ordering::Relaxed), 1);

        engine.present_sync(&mut mem, &mut scratch).unwrap();
        let non_bg = (knob_rect.y..knob_rect.y + knob_rect.h as i32)
            .flat_map(|y| {
                (knob_rect.x..knob_rect.x + knob_rect.w as i32).map(move |x| (x as u32, y as u32))
            })
            .filter(|&(x, y)| {
                mem.pixel(x, y) != liuli::core::geometry::rgb565_to_u16(Theme::DARK.background)
            })
            .count();
        assert!(non_bg > 50);
    }

    #[test]
    fn all_preset_color_schemes_meet_readability_contrast() {
        for preset in ThemePreset::ALL {
            for dark in [false, true] {
                let t = Theme::preset(preset, dark);
                let c = t.colors;
                let text_on_bg = contrast_ratio_x10(c.text, c.background);
                let text_on_surf = contrast_ratio_x10(c.text, c.surface);
                let text_on_alt = contrast_ratio_x10(c.text, c.surface_alt);
                let muted_on_bg = contrast_ratio_x10(c.text_muted, c.background);
                let muted_on_surf = contrast_ratio_x10(c.text_muted, c.surface);
                let on_accent = contrast_ratio_x10(c.on_accent, c.accent);
                let on_danger = contrast_ratio_x10(c.on_danger, c.danger);
                assert!(
                    text_on_bg >= 45 && text_on_surf >= 45 && text_on_alt >= 45,
                    "{preset:?} dark={dark} primary text contrast too low: {text_on_bg}/{text_on_surf}/{text_on_alt}"
                );
                assert!(
                    muted_on_bg >= 30 && muted_on_surf >= 30,
                    "{preset:?} dark={dark} muted contrast too low: {muted_on_bg}/{muted_on_surf}"
                );
                assert!(
                    on_accent >= 30 && on_danger >= 30,
                    "{preset:?} dark={dark} button ink contrast too low: {on_accent}/{on_danger}"
                );
            }
        }
    }

    #[test]
    fn granular_effects_capabilities_toggle_and_clamp_independently() {
        let flat = Theme::DARK.with_effect_tier(EffectTier::Flat);
        assert_eq!(flat.elevation(Elevation::Floating), SurfaceStyle::FLAT);

        // Disable only shadows while keeping press depth & glass tint.
        let no_shadow = Theme::DARK
            .with_shadows(false)
            .with_glass_blur(false)
            .with_glass_tint(None, 8);
        assert_eq!(no_shadow.elevation(Elevation::Raised).shadow, None);

        // Cap shadow blur and scale shadow opacity independently.
        let capped = Theme::DARK.with_effects(
            EffectsConfig::GLASS
                .with_max_shadow_blur(3)
                .with_shadow_opacity_scale(8),
        );
        let sh = capped.elevation(Elevation::Floating).shadow.unwrap();
        assert_eq!(sh.blur, 3);
        assert!(
            sh.opacity
                < Theme::DARK
                    .elevation(Elevation::Floating)
                    .shadow
                    .unwrap()
                    .opacity
        );

        // Glass blur vs glass tint fallback resolution works independently of Cargo features.
        let tint_only = Theme::DARK.with_glass_blur(false).with_glass_tint(None, 9);
        let res = tint_only.resolve_surface(SurfaceSpec::GLASS, tint_only.card());
        assert_eq!(
            res.effect.press_depth.duration_ms(),
            tint_only.motion.press_depth_ms
        );

        let no_press = Theme::DARK.with_press_depth(false);
        assert!(
            !no_press
                .elevation(Elevation::Raised)
                .press_depth
                .is_enabled()
        );
    }

    #[test]
    fn legacy_theme_with_and_field_defaults_remain_compatible() {
        assert_eq!(Theme::DARK.control_height, 32);
        assert_eq!(Theme::DARK.spacing, 6);
        assert_eq!(Theme::DARK.radius, 8);
        assert_eq!(Theme::LIGHT.control_height, 32);

        let custom = Theme::DARK.with(Style {
            control_height: Some(38),
            spacing: Some(10),
            radius: Some(4),
            accent: Some(rgb8(255, 0, 128)),
            ..Style::default()
        });
        assert_eq!(custom.control_height, 38);
        assert_eq!(custom.density.control_height, 38);
        assert_eq!(custom.spacing, 10);
        assert_eq!(custom.density.spacing, 10);
        assert_eq!(custom.radius, 4);
        assert_eq!(custom.shape.radius_md, 4);
        assert_eq!(custom.accent, rgb8(255, 0, 128));
        assert_eq!(custom.colors.accent, rgb8(255, 0, 128));
    }

    #[test]
    fn compositor_and_variant_buttons_honor_individual_gates() {
        let mut draft = liuli::core::view::ViewDraft::new();
        let no_3d_no_opacity = Theme::DARK.with_effects(
            EffectsConfig::GLASS
                .with_projection_3d(false)
                .with_group_opacity(false)
                .with_glass_blur(false)
                .with_glass_tint(false),
        );
        let resolved = no_3d_no_opacity.resolve_surface(
            SurfaceSpec::GLASS.with_compositor(Compositor::isolated(6, Projection(32))),
            no_3d_no_opacity.card(),
        );
        // Glass degraded cleanly to elevated solid card and blocked 3D/opacity when gated off.
        assert!(resolved.props.bg.is_some());
        if let Some(layer) = resolved.effect.layer() {
            assert_eq!(layer.opacity, 15);
            assert_eq!(layer.projection, Projection(0));
            assert_eq!(layer.backdrop_radius, 0);
            assert_eq!(layer.tint_alpha, 0);
        }

        let theme = Theme::DARK
            .with_rounded_focus(true)
            .with_motion(Motion::SNAPPY);
        let mut vb = ViewBuilder::new(&mut draft);
        vb.root(Key(1), NodeProps::new(), |root| {
            let mut ui = ThemedUi::new(root, theme);
            ui.card(Key(2), SurfaceSpec::RAISED, |card| {
                card.button(Key(3), "Save", IconKind::Check, ButtonVariant::Primary)?;
                card.button(Key(4), "Delete", IconKind::Warning, ButtonVariant::Danger)?;
                card.button(Key(5), "Info", IconKind::Sliders, ButtonVariant::Outline)
            })
        })
        .unwrap();
        vb.finish().unwrap();
        assert_eq!(draft.nodes[2].props.focus_ring().rounded, true);
        assert_eq!(draft.nodes[4].props.bg(), None);
        assert_eq!(draft.nodes[4].props.fg, theme.accent);
    }
}
