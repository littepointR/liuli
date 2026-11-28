//! Composable, allocation-free descriptions of a view's maximum live tree.
//! A component's type describes all branches, even when the initial model is empty.
use crate::capacity::BuildError;
use crate::view::{Children, Key, NodeKind, NodeProps, ViewBuilder};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Demand {
    pub nodes: usize,
    pub text_slots: usize,
    /// Root and leaf each count as one level.
    pub depth: usize,
}

impl Demand {
    pub const EMPTY: Self = Self {
        nodes: 0,
        text_slots: 0,
        depth: 0,
    };
    pub const LEAF: Self = Self {
        nodes: 1,
        text_slots: 1,
        depth: 1,
    };
    pub const fn siblings(self, other: Self) -> Self {
        Self {
            text_slots: self.text_slots.saturating_add(other.text_slots),
            nodes: self.nodes.saturating_add(other.nodes),
            depth: maximum(self.depth, other.depth),
        }
    }
    pub const fn alternative(self, other: Self) -> Self {
        Self {
            text_slots: maximum(self.text_slots, other.text_slots),
            nodes: maximum(self.nodes, other.nodes),
            depth: maximum(self.depth, other.depth),
        }
    }
    pub const fn repeated(self, limit: usize) -> Self {
        if limit == 0 {
            Self::EMPTY
        } else {
            Self {
                text_slots: self.text_slots.saturating_mul(limit),
                nodes: self.nodes.saturating_mul(limit),
                depth: self.depth,
            }
        }
    }
    pub const fn enclosed(self) -> Self {
        Self {
            text_slots: self.text_slots,
            nodes: self.nodes.saturating_add(1),
            depth: self.depth.saturating_add(1),
        }
    }
}

const fn maximum(a: usize, b: usize) -> usize {
    if a > b { a } else { b }
}

/// Third-party composite controls can return `impl Component` built with these
/// combinators. Implementors rendering imperatively must bound every emitted branch.
pub trait Component {
    const DEMAND: Demand;
    fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError>;
}

pub(crate) fn root<C: Component>(
    out: &mut ViewBuilder<'_>,
    key: Key,
    props: NodeProps,
    children: C,
) -> Result<(), BuildError> {
    const {
        let d = C::DEMAND.enclosed();
        assert!(
            !crate::capacity::PLANNED
                || (d.nodes <= crate::capacity::MAX_NODES
                    && d.depth <= crate::capacity::MAX_DEPTH
                    && d.text_slots <= crate::capacity::MAX_TEXT_SLOTS),
            "target component declaration exceeds the host capacity plan"
        );
    }
    out.root(key, props, |ui| children.render(ui))
}

/// Adapts a one-node widget written against the imperative builder. Formatting,
/// revision and decoration calls can accompany the node. The key and arity are checked.
pub fn single<F>(key: Key, build: F) -> impl Component
where
    F: FnOnce(&mut Children<'_, '_>) -> Result<(), BuildError>,
{
    Single(key, build)
}
struct Single<F>(Key, F);
impl<F> Component for Single<F>
where
    F: FnOnce(&mut Children<'_, '_>) -> Result<(), BuildError>,
{
    const DEMAND: Demand = Demand::LEAF;
    fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
        ui.single_node(self.0, self.1)
    }
}

impl<C: Component, const N: usize> Component for [C; N] {
    const DEMAND: Demand = C::DEMAND.repeated(N);
    fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
        for child in self {
            child.render(ui)?;
        }
        Ok(())
    }
}

struct Leaf<P>(Key, NodeKind, P);
pub fn leaf<P: FnOnce() -> NodeProps>(key: Key, kind: NodeKind, props: P) -> impl Component {
    Leaf(key, kind, props)
}
impl<P: FnOnce() -> NodeProps> Component for Leaf<P> {
    const DEMAND: Demand = Demand::LEAF;
    fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
        ui.leaf(self.0, self.1, self.2())
    }
}

pub struct TextBuilder<'u, 'a, 'b>(&'u mut Children<'a, 'b>);
impl TextBuilder<'_, '_, '_> {
    pub fn format(
        &mut self,
        args: core::fmt::Arguments<'_>,
    ) -> Result<crate::view::TextRef, BuildError> {
        self.0.text_fmt(args)
    }
    pub fn copy(&mut self, text: &str) -> Result<crate::view::TextRef, BuildError> {
        self.0.text_dyn(text)
    }
}
struct TextLeaf<F>(Key, NodeKind, F);
pub fn text_leaf<F>(key: Key, kind: NodeKind, props: F) -> impl Component
where
    F: FnOnce(&mut TextBuilder<'_, '_, '_>) -> Result<NodeProps, BuildError>,
{
    TextLeaf(key, kind, props)
}
impl<F> Component for TextLeaf<F>
where
    F: FnOnce(&mut TextBuilder<'_, '_, '_>) -> Result<NodeProps, BuildError>,
{
    const DEMAND: Demand = Demand::LEAF;
    fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
        let props = self.2(&mut TextBuilder(ui))?;
        ui.leaf(self.0, self.1, props)
    }
}

struct Container<P, C>(Key, NodeKind, P, C);
pub fn container<P: FnOnce() -> NodeProps, C: Component>(
    key: Key,
    kind: NodeKind,
    props: P,
    children: C,
) -> impl Component {
    Container(key, kind, props, children)
}
impl<P: FnOnce() -> NodeProps, C: Component> Component for Container<P, C> {
    const DEMAND: Demand = C::DEMAND.enclosed();
    fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
        ui.branch(self.0, self.1, self.2(), |ui| self.3.render(ui))
    }
}

impl Component for () {
    const DEMAND: Demand = Demand::EMPTY;
    fn render(self, _: &mut Children<'_, '_>) -> Result<(), BuildError> {
        Ok(())
    }
}
macro_rules! siblings {
    ($($t:ident $field:tt),+) => {
        impl<$($t: Component),+> Component for ($($t,)+) {
            const DEMAND: Demand = Demand::EMPTY$(.siblings($t::DEMAND))+;
            fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
                $(self.$field.render(ui)?;)+ Ok(())
            }
        }
    };
}
siblings!(A 0, B 1);
siblings!(A 0, B 1, C 2);
siblings!(A 0, B 1, C 2, D 3);

impl<C: Component> Component for Option<C> {
    const DEMAND: Demand = C::DEMAND;
    fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
        if let Some(component) = self {
            component.render(ui)?;
        }
        Ok(())
    }
}
pub enum Either<A, B> {
    First(A),
    Second(B),
}
impl<A: Component, B: Component> Component for Either<A, B> {
    const DEMAND: Demand = A::DEMAND.alternative(B::DEMAND);
    fn render(self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
        match self {
            Self::First(a) => a.render(ui),
            Self::Second(b) => b.render(ui),
        }
    }
}

pub struct Bounded<const LIMIT: usize>;
struct Rows<const LIMIT: usize, I, F>(Key, I, F);
impl<const LIMIT: usize> Bounded<LIMIT> {
    /// Owns no item storage. Supply keys derived from stable record IDs in `row`.
    pub fn rows<I, F, C>(key: Key, items: I, row: F) -> impl Component
    where
        I: IntoIterator,
        F: FnMut(I::Item) -> C,
        C: Component,
    {
        Rows::<LIMIT, _, _>(key, items, row)
    }
}
impl<const LIMIT: usize, I, F, C> Component for Rows<LIMIT, I, F>
where
    I: IntoIterator,
    F: FnMut(I::Item) -> C,
    C: Component,
{
    const DEMAND: Demand = C::DEMAND.repeated(LIMIT);
    fn render(mut self, ui: &mut Children<'_, '_>) -> Result<(), BuildError> {
        for (index, item) in self.1.into_iter().enumerate() {
            if index >= LIMIT {
                return Err(ui.business_limit(self.0));
            }
            self.2(item).render(ui)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::ViewDraft;
    #[test]
    fn custom_container_rejects_first_visible_child_above_its_budget() {
        use crate::capacity::{MAX_CUSTOM_CHILDREN, MAX_NODES};
        if MAX_CUSTOM_CHILDREN + 3 > MAX_NODES {
            return;
        }
        let mut draft = ViewDraft::new();
        let mut builder = ViewBuilder::new(&mut draft);
        let result = builder.root(Key(0), NodeProps::new(), |ui| {
            ui.custom_container(Key(1), NodeProps::new(), |c| {
                for i in 0..=MAX_CUSTOM_CHILDREN {
                    c.custom(Key(i as u32 + 2), NodeProps::new())?;
                }
                Ok(())
            })
        });
        assert_eq!(result, Err(BuildError::CustomChildren));
        assert_eq!(
            builder.operation_key(),
            Some(Key(MAX_CUSTOM_CHILDREN as u32 + 2))
        );
    }
}
