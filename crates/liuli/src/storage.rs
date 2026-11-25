use liuli_core::damage::DamageSet;
use liuli_core::focus::{FocusState, PointerState, RepeatState};
use liuli_core::node::RetainedTree;
use liuli_core::paint::CommandList;
use liuli_core::time::{AnimPool, ClockTracker};
use liuli_core::view::ViewDraft;

pub struct Storage {
    pub trees: [RetainedTree; 2],
    pub active_tree_idx: u8,
    pub cmds: [CommandList; 2],
    pub active_cmd_idx: u8,
    pub draft: ViewDraft,
    pub damage: DamageSet,
    pub cand_damage: DamageSet,
    pub anims: AnimPool,
    pub(crate) cand_anims: AnimPool,
    pub focus: FocusState,
    pub pointer: PointerState,
    pub repeat: RepeatState,
    pub clock: ClockTracker,
}

impl Storage {
    pub const ZERO: Self = Self {
        trees: [RetainedTree::ZERO, RetainedTree::ZERO],
        active_tree_idx: 0,
        cmds: [CommandList::ZERO, CommandList::ZERO],
        active_cmd_idx: 0,
        draft: ViewDraft::ZERO,
        damage: DamageSet::new(),
        cand_damage: DamageSet::new(),
        anims: AnimPool::ZERO,
        cand_anims: AnimPool::ZERO,
        focus: FocusState::ZERO,
        pointer: PointerState::ZERO,
        repeat: RepeatState::ZERO,
        clock: ClockTracker::ZERO,
    };

    pub const fn new() -> Self {
        Self::ZERO
    }

    pub fn init_in_place(&mut self) {
        self.trees[0].reset();
        self.trees[1].reset();
        self.trees[0].generation_hwm = [0; liuli_core::capacity::MAX_NODES];
        self.trees[1].generation_hwm = [0; liuli_core::capacity::MAX_NODES];
        self.active_tree_idx = 0;
        self.cmds[0].reset();
        self.cmds[1].reset();
        self.active_cmd_idx = 0;
        self.draft.reset();
        self.damage.clear();
        self.cand_damage.clear();
        self.anims.reset();
        self.cand_anims.reset();
        self.focus.reset();
        self.pointer = PointerState::ZERO;
        self.repeat = RepeatState::ZERO;
        self.clock = ClockTracker::ZERO;
    }

    #[inline]
    pub fn active_tree(&self) -> &RetainedTree {
        &self.trees[self.active_tree_idx as usize]
    }

    #[inline]
    pub fn active_cmds(&self) -> &CommandList {
        &self.cmds[self.active_cmd_idx as usize]
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(
    target_pointer_width = "32",
    not(any(feature = "capacity-128", feature = "capacity-256"))
))]
const _: () = {
    assert!(core::mem::size_of::<liuli_core::paint::PaintCmd>() <= 48);
    assert!(core::mem::size_of::<liuli_core::node::RetainedNode>() <= 224);
    assert!(core::mem::size_of::<Storage>() <= 67_456);
};

#[cfg(all(
    target_pointer_width = "64",
    not(any(feature = "capacity-128", feature = "capacity-256"))
))]
const _: () = {
    assert!(core::mem::size_of::<liuli_core::paint::PaintCmd>() <= 48);
    assert!(core::mem::size_of::<liuli_core::node::RetainedNode>() <= 232);
    assert!(core::mem::size_of::<Storage>() <= 73_120);
};

#[cfg(all(
    target_pointer_width = "32",
    feature = "capacity-128",
    not(feature = "capacity-256")
))]
const _: () = {
    assert!(core::mem::size_of::<liuli_core::paint::PaintCmd>() <= 48);
    assert!(core::mem::size_of::<liuli_core::node::RetainedNode>() <= 224);
    assert!(core::mem::size_of::<Storage>() <= 160_000);
};

#[cfg(all(
    target_pointer_width = "64",
    feature = "capacity-128",
    not(feature = "capacity-256")
))]
const _: () = {
    assert!(core::mem::size_of::<liuli_core::paint::PaintCmd>() <= 48);
    assert!(core::mem::size_of::<liuli_core::node::RetainedNode>() <= 232);
    assert!(core::mem::size_of::<Storage>() <= 176_000);
};

#[cfg(all(target_pointer_width = "32", feature = "capacity-256"))]
const _: () = {
    assert!(core::mem::size_of::<liuli_core::paint::PaintCmd>() <= 48);
    assert!(core::mem::size_of::<liuli_core::node::RetainedNode>() <= 224);
    assert!(core::mem::size_of::<Storage>() <= 320_000);
};

#[cfg(all(target_pointer_width = "64", feature = "capacity-256"))]
const _: () = {
    assert!(core::mem::size_of::<liuli_core::paint::PaintCmd>() <= 48);
    assert!(core::mem::size_of::<liuli_core::node::RetainedNode>() <= 232);
    assert!(core::mem::size_of::<Storage>() <= 352_000);
};
