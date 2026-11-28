include!(concat!(env!("OUT_DIR"), "/capacity.rs"));
/// A generic arena for host-only tests mounting unrelated and intentionally faulty scenes.
pub const HOST_TESTING: bool = cfg!(all(feature = "host-testing", not(target_os = "none")));
pub const MAX_TEXT_SLOTS: usize = if TEXT_SLOTS_LIMIT < MAX_NODES {
    TEXT_SLOTS_LIMIT
} else {
    MAX_NODES
};
pub const MAX_MODALS: usize = 1;
pub const MAX_ACTIONS: usize = 8;

/// Compact fixed-size bitset spanning `0..MAX_NODES`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct NodeMask {
    words: [u64; MAX_NODES.div_ceil(64)],
}

impl NodeMask {
    pub const ZERO: Self = Self {
        words: [0; MAX_NODES.div_ceil(64)],
    };

    #[inline]
    pub const fn new() -> Self {
        Self::ZERO
    }

    #[inline]
    pub fn set(&mut self, idx: usize) {
        if idx < MAX_NODES {
            self.words[idx >> 6] |= 1u64 << (idx & 63);
        }
    }

    #[inline]
    pub fn clear(&mut self, idx: usize) {
        if idx < MAX_NODES {
            self.words[idx >> 6] &= !(1u64 << (idx & 63));
        }
    }

    #[inline]
    pub const fn get(&self, idx: usize) -> bool {
        if idx < MAX_NODES {
            (self.words[idx >> 6] & (1u64 << (idx & 63))) != 0
        } else {
            false
        }
    }

    #[inline]
    pub fn any(&self) -> bool {
        self.words.iter().any(|&w| w != 0)
    }

    /// Removes and returns the lowest set node slot index, if any.
    #[inline]
    pub fn pop_first(&mut self) -> Option<usize> {
        for (wi, w) in self.words.iter_mut().enumerate() {
            if *w != 0 {
                let bit = w.trailing_zeros() as usize;
                *w &= *w - 1;
                let idx = (wi << 6) | bit;
                if idx < MAX_NODES {
                    return Some(idx);
                }
            }
        }
        None
    }
}

impl Default for NodeMask {
    fn default() -> Self {
        Self::ZERO
    }
}

pub const DEFAULT_SCREEN_WIDTH: u32 = 320;
pub const DEFAULT_SCREEN_HEIGHT: u32 = 240;
pub const DEFAULT_BAND_WIDTH: usize = 320;
pub const DEFAULT_BAND_HEIGHT: usize = 8;
pub const DEFAULT_BAND_PIXELS: usize = DEFAULT_BAND_WIDTH * DEFAULT_BAND_HEIGHT;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum BuildError {
    ItemLimit,
    CustomChildren,
    Nodes,
    Depth,
    TextBytes,
    TextSlots,
    Commands,
    DuplicateKey,
    UnmatchedEnd,
    UnclosedContainer,
    NestedModal,
    InvalidScrollFill,
    GenerationExhausted,
    NoRoot,
    MultipleRoots,
    InvalidKind,
    InvalidRange,
    Sparklines,
    MotionCurves,
    PartStyles,
    Effects,
    LayoutExtras,
    LayerPixels,
    UnsupportedEffect,
    PathEdges,
    Layout(LayoutError),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LayoutError {
    InvalidScrollFill,
    EmptyTree,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TimeError {
    WentBackwards,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum DispatchError<E = ()> {
    WentBackwards,
    Busy(E),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TransactionError<E = ()> {
    WentBackwards,
    Busy(E),
    Build(BuildError),
}

impl<E> From<TimeError> for TransactionError<E> {
    fn from(value: TimeError) -> Self {
        match value {
            TimeError::WentBackwards => Self::WentBackwards,
        }
    }
}

impl<E> From<BuildError> for TransactionError<E> {
    fn from(value: BuildError) -> Self {
        Self::Build(value)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PresentStatus {
    Idle,
    Pending,
    Presented,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PresentError<E> {
    ShortGlyph { needed: usize, actual: usize },
    BufferTooSmall,
    Target(E),
    DmaFaulted,
    DmaBusy,
    NoDamage,
    ViewPending,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct FrameMetrics {
    pub measured_nodes: u32,
    pub bands: u32,
    pub pixels_sent: u32,
    pub commands_replayed: u32,
}

/// Internal counters disappear from presenters which do not collect frame metrics.
#[derive(Default)]
pub struct FrameCounters([FrameMetrics; METRICS as usize]);
impl FrameCounters {
    #[inline]
    pub fn replayed(&mut self, n: u32) {
        if let Some(m) = self.0.first_mut() {
            m.commands_replayed = m.commands_replayed.saturating_add(n);
        }
    }
    #[inline]
    pub fn sent(&mut self, pixels: u32) {
        if let Some(m) = self.0.first_mut() {
            m.bands = m.bands.saturating_add(1);
            m.pixels_sent = m.pixels_sent.saturating_add(pixels);
        }
    }
    #[inline]
    pub fn snapshot(&self) -> FrameMetrics {
        self.0.first().copied().unwrap_or_default()
    }
}
