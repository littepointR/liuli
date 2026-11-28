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
    #[cfg(liuli_pointer)]
    pub pointer: PointerState,
    #[cfg(liuli_repeat)]
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
        #[cfg(liuli_pointer)]
        pointer: PointerState::ZERO,
        #[cfg(liuli_repeat)]
        repeat: RepeatState::ZERO,
        clock: ClockTracker::ZERO,
    };

    pub(crate) const fn pointer_state(&self) -> &PointerState {
        #[cfg(liuli_pointer)]
        {
            &self.pointer
        }
        #[cfg(not(liuli_pointer))]
        {
            &PointerState::ZERO
        }
    }
    pub(crate) fn set_pointer(&mut self, value: PointerState) {
        #[cfg(liuli_pointer)]
        {
            self.pointer = value;
        }
        #[cfg(not(liuli_pointer))]
        let _ = value;
    }
    pub(crate) const fn repeat_state(&self) -> &RepeatState {
        #[cfg(liuli_repeat)]
        {
            &self.repeat
        }
        #[cfg(not(liuli_repeat))]
        {
            &RepeatState::ZERO
        }
    }
    pub(crate) fn set_repeat(&mut self, value: RepeatState) {
        #[cfg(liuli_repeat)]
        {
            self.repeat = value;
        }
        #[cfg(not(liuli_repeat))]
        let _ = value;
    }
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
        self.set_pointer(PointerState::ZERO);
        self.set_repeat(RepeatState::ZERO);
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

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(core::mem::size_of::<liuli_core::paint::PaintCmd>() <= 48);
    assert!(core::mem::size_of::<liuli_core::node::RetainedNode>() <= 224);
    // Three effect tables and two command lists also retain layer metadata when enabled.
    // The basic-profile limit is shared with checks/audit-firmware.ps1.
    let layer_metadata = if liuli_core::effects::SurfaceStyle::has_layers_support() {
        256
    } else {
        0
    };
    assert!(
        liuli_core::capacity::CUSTOM_BUDGET
            || core::mem::size_of::<Storage>().saturating_sub(PAINTER_BUDGET)
                <= 67_456 + layer_metadata
    );
};

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(core::mem::size_of::<liuli_core::paint::PaintCmd>() <= 48);
    assert!(core::mem::size_of::<liuli_core::node::RetainedNode>() <= 232);
    assert!(
        liuli_core::capacity::CUSTOM_BUDGET
            || core::mem::size_of::<Storage>().saturating_sub(PAINTER_BUDGET) <= 73_120
    );
};

const PAINTER_BUDGET: usize = if cfg!(feature = "part-painters") {
    liuli_core::part_style::MAX_PART_STYLES
        * 3
        * core::mem::size_of::<liuli_core::part_style::WidgetPartStyle>().saturating_sub(52)
} else {
    0
};
