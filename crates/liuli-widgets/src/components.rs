//! Capacity-aware forms of the standard composite widgets. Single-node controls
//! such as a gauge or checkbox can be used directly with `component::single`.
use crate::{DropdownState, TabLayout, TableColumn, Theme};
use liuli::{
    component::{Component, Demand},
    prelude::*,
};

/// Internal bridge for existing builder bodies. The declaration counts the same
/// header, row and item groups regardless of the initial model.
struct Choices<const N: usize, F, const PANEL: bool>(Key, usize, F);
impl<const N: usize, F, const PANEL: bool> Component for Choices<N, F, PANEL>
where
    F: FnOnce(&mut Children<'_, '_>) -> Result<(), BuildError>,
{
    const DEMAND: Demand = if PANEL {
        Demand::LEAF
            .siblings(Demand::LEAF.repeated(N).enclosed())
            .enclosed()
    } else {
        Demand::LEAF.repeated(N).enclosed()
    };
    fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
        if self.1 > N {
            return Err(ui.business_limit(self.0));
        }
        (self.2)(ui)
    }
}

pub fn selector<const OPTIONS: usize>(
    key: Key,
    items: &[(Key, TextRef)],
    selected: Key,
    theme: Theme,
) -> impl Component {
    Choices::<OPTIONS, _, false>(key, items.len(), move |ui: &mut Children<'_, '_>| {
        crate::selector(ui, key, items, selected, theme)
    })
}

pub fn dropdown<const OPTIONS: usize>(
    keys: [Key; 3],
    item_base: u32,
    options: &[&'static str],
    state: DropdownState,
    theme: Theme,
) -> impl Component {
    Choices::<OPTIONS, _, true>(keys[0], options.len(), move |ui: &mut Children<'_, '_>| {
        crate::dropdown(
            ui, keys[0], keys[1], keys[2], item_base, options, state, theme,
        )
    })
}

pub fn tab_strip<const TABS: usize>(
    key: Key,
    items: &[(Key, TextRef)],
    selected: Key,
    layout: TabLayout,
    theme: Theme,
) -> impl Component {
    Choices::<TABS, _, false>(key, items.len(), move |ui: &mut Children<'_, '_>| {
        crate::tab_strip(ui, key, items, selected, layout, theme)
    })
}

struct Panel<C>(Key, Key, NodeProps, C);
impl<C: Component> Component for Panel<C> {
    const DEMAND: Demand = C::DEMAND.enclosed();
    fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
        ui.tab_panel(self.0, self.1, self.2, |p| self.3.render(p))
    }
}
pub fn tab_panel(
    key: Key,
    selected: Key,
    props: NodeProps,
    page: impl Component,
) -> impl Component {
    Panel(key, selected, props, page)
}

struct Table<'a, const ROWS: usize, const COLS: usize> {
    key: Key,
    cols: &'a [TableColumn; COLS],
    rows: &'a [&'a [&'static str]],
    selected: Option<usize>,
    height: u16,
    theme: Theme,
}
impl<const ROWS: usize, const COLS: usize> Component for Table<'_, ROWS, COLS> {
    const DEMAND: Demand = Demand::LEAF
        .repeated(COLS)
        .enclosed()
        .repeated(ROWS.saturating_add(1))
        .enclosed();
    fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
        if self.rows.len() > ROWS {
            return Err(ui.business_limit(self.key));
        }
        crate::data_table(
            ui,
            self.key.0,
            self.cols,
            self.rows,
            self.selected,
            self.height,
            self.theme,
        )
    }
}
/// Columns follow the supplied array; only the business row limit needs a value.
pub fn data_table<'a, const ROWS: usize, const COLS: usize>(
    key: Key,
    cols: &'a [TableColumn; COLS],
    rows: &'a [&'a [&'static str]],
    selected: Option<usize>,
    height: u16,
    theme: Theme,
) -> impl Component {
    Table::<ROWS, COLS> {
        key,
        cols,
        rows,
        selected,
        height,
        theme,
    }
}
