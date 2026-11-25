#[cfg(feature = "capacity-256")]
pub const MAX_NODES: usize = 250;
#[cfg(all(feature = "capacity-128", not(feature = "capacity-256")))]
pub const MAX_NODES: usize = 128;
#[cfg(not(any(feature = "capacity-128", feature = "capacity-256")))]
pub const MAX_NODES: usize = 64;

#[cfg(feature = "capacity-256")]
pub const MAX_DEPTH: usize = 16;
#[cfg(all(feature = "capacity-128", not(feature = "capacity-256")))]
pub const MAX_DEPTH: usize = 14;
#[cfg(not(any(feature = "capacity-128", feature = "capacity-256")))]
pub const MAX_DEPTH: usize = 12;

#[cfg(feature = "capacity-256")]
pub const MAX_TEXT_BYTES: usize = 4096;
#[cfg(all(feature = "capacity-128", not(feature = "capacity-256")))]
pub const MAX_TEXT_BYTES: usize = 2048;
#[cfg(not(any(feature = "capacity-128", feature = "capacity-256")))]
pub const MAX_TEXT_BYTES: usize = 1024;

#[cfg(feature = "capacity-256")]
pub const MAX_COMMANDS: usize = 1024;
#[cfg(all(feature = "capacity-128", not(feature = "capacity-256")))]
pub const MAX_COMMANDS: usize = 512;
#[cfg(not(any(feature = "capacity-128", feature = "capacity-256")))]
pub const MAX_COMMANDS: usize = 256;

#[cfg(feature = "capacity-256")]
pub const MAX_DAMAGE: usize = 24;
#[cfg(all(feature = "capacity-128", not(feature = "capacity-256")))]
pub const MAX_DAMAGE: usize = 16;
#[cfg(not(any(feature = "capacity-128", feature = "capacity-256")))]
pub const MAX_DAMAGE: usize = 12;

#[cfg(feature = "capacity-256")]
pub const MAX_ANIMATIONS: usize = 16;
#[cfg(all(feature = "capacity-128", not(feature = "capacity-256")))]
pub const MAX_ANIMATIONS: usize = 12;
#[cfg(not(any(feature = "capacity-128", feature = "capacity-256")))]
pub const MAX_ANIMATIONS: usize = 8;

pub const MAX_MODALS: usize = 1;
pub const MAX_ACTIONS: usize = 8;

#[cfg(feature = "capacity-256")]
pub const MAX_SPARKLINES: usize = 4;
#[cfg(all(feature = "capacity-128", not(feature = "capacity-256")))]
pub const MAX_SPARKLINES: usize = 4;
#[cfg(not(any(feature = "capacity-128", feature = "capacity-256")))]
pub const MAX_SPARKLINES: usize = 2;

pub const MAX_SPARKLINE_SAMPLES: usize = 32;

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
    Nodes,
    Depth,
    TextBytes,
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
    LayerPixels,
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
