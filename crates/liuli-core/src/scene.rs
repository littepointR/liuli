//! A complete application view. The same declaration is used to build and size it.
use crate::{
    capacity::BuildError,
    component::{Component, Demand},
    view::{Children, Key, NodeProps, ViewBuilder},
};

pub trait Scene {
    fn build(self, out: &mut ViewBuilder<'_>) -> Result<(), BuildError>;
    fn static_demand(&self) -> Option<Demand> {
        None
    }
}

struct Composed<C>(Key, NodeProps, C);

/// A root composed of bounded controls. Inactive branches participate through their types.
pub fn scene<C: Component>(key: Key, props: NodeProps, children: C) -> impl Scene {
    Composed(key, props, children)
}

impl<C: Component> Scene for Composed<C> {
    fn build(self, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        crate::component::root(out, self.0, self.1, self.2)
    }
    fn static_demand(&self) -> Option<Demand> {
        Some(C::DEMAND.enclosed())
    }
}

struct Layout<F>(Key, NodeProps, F);

/// A scoped layout. Declare structural alternatives with `Children::variant` and
/// additive optional content with `Children::optional`. Pure style choices need neither.
pub fn layout<F>(key: Key, props: NodeProps, children: F) -> impl Scene
where
    F: FnOnce(&mut Children<'_, '_>) -> Result<(), BuildError>,
{
    Layout(key, props, children)
}

impl<F> Scene for Layout<F>
where
    F: FnOnce(&mut Children<'_, '_>) -> Result<(), BuildError>,
{
    fn build(self, out: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        out.root(self.0, self.1, self.2)
    }
}

#[cfg(all(feature = "scene-planner", not(target_os = "none")))]
mod planning {
    use super::*;
    use crate::view::{App, ViewDraft};
    use alloc::vec::Vec;

    /// Mixed-radix traversal of the alternatives encountered on this path.
    /// Nested selections may have completely different lengths on the next path.
    #[derive(Default)]
    pub(crate) struct Choices {
        path: Vec<usize>,
        widths: Vec<usize>,
        pub(crate) reserved: Demand,
    }
    impl Choices {
        pub(crate) fn select(&mut self, count: usize) -> usize {
            let index = self.widths.len();
            self.widths.push(count);
            *self.path.get(index).unwrap_or(&0)
        }
        fn advance(&mut self) -> bool {
            self.path.resize(self.widths.len(), 0);
            while let Some(width) = self.widths.pop() {
                let last = self.path.last_mut().unwrap();
                *last += 1;
                if *last < width {
                    self.widths.clear();
                    return true;
                }
                self.path.pop();
            }
            false
        }
    }

    /// Evaluate every declared structural alternative. Errors stop the build.
    pub fn capacity<A: App>(model: &A::Model) -> Result<Demand, BuildError> {
        if let Some(bound) = A::view(model).static_demand() {
            return Ok(bound);
        }
        let mut choices = Choices::default();
        let mut draft = ViewDraft::new();
        let mut peak = Demand::EMPTY;
        loop {
            let mut out = ViewBuilder::new(&mut draft);
            out.choices = Some(&mut choices);
            A::view(model).build(&mut out)?;
            out.finish()?;
            let mut current = Demand {
                nodes: usize::from(draft.count),
                ..Demand::EMPTY
            };
            for node in &draft.nodes[..current.nodes] {
                current.text_slots += usize::from(node.kind.uses_text_slot());
                let mut depth = 1;
                let mut parent = node.parent;
                while let Some(slot) = parent.get() {
                    depth += 1;
                    parent = draft.nodes[slot].parent;
                }
                current.depth = current.depth.max(depth);
            }
            current.nodes += choices.reserved.nodes;
            current.text_slots += choices.reserved.text_slots;
            current.depth = current.depth.max(choices.reserved.depth);
            peak = peak.alternative(current);
            choices.reserved = Demand::EMPTY;
            if !choices.advance() {
                return Ok(peak);
            }
        }
    }
}

#[cfg(all(feature = "scene-planner", not(target_os = "none")))]
pub(crate) use planning::Choices;
#[cfg(all(feature = "scene-planner", not(target_os = "none")))]
pub use planning::capacity;
