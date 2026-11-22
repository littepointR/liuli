use liuli_core::damage::DamageSet;
use liuli_core::focus::{FocusState, PointerState};
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
    pub focus: FocusState,
    pub pointer: PointerState,
    pub clock: ClockTracker,
}

impl Storage {
    pub const fn new() -> Self {
        Self {
            trees: [RetainedTree::new(), RetainedTree::new()],
            active_tree_idx: 0,
            cmds: [CommandList::new(), CommandList::new()],
            active_cmd_idx: 0,
            draft: ViewDraft::new(),
            damage: DamageSet::new(),
            cand_damage: DamageSet::new(),
            anims: AnimPool::new(),
            focus: FocusState::new(),
            pointer: PointerState::new(),
            clock: ClockTracker::new(),
        }
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
        self.focus.reset();
        self.pointer = PointerState::new();
        self.clock = ClockTracker::new();
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
