pub const MAX_NODES: usize = 64;
pub const MAX_DEPTH: usize = 12;
pub const MAX_TEXT_BYTES: usize = 1024;
pub const MAX_COMMANDS: usize = 256;
pub const MAX_DAMAGE: usize = 12;
pub const MAX_ANIMATIONS: usize = 8;
pub const MAX_MODALS: usize = 1;
pub const MAX_ACTIONS: usize = 8;
pub const MAX_SPARKLINES: usize = 2;
pub const MAX_SPARKLINE_SAMPLES: usize = 32;

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
