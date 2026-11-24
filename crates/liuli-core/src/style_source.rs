//! Theme provenance for the desktop inspector. Firmware builds use a zero-sized marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleSource {
    #[cfg(feature = "inspector")]
    pub theme: &'static str,
    #[cfg(feature = "inspector")]
    pub declaration: Option<&'static core::panic::Location<'static>>,
    #[cfg(feature = "inspector")]
    pub overrides: [Option<&'static core::panic::Location<'static>>; 8],
}
impl StyleSource {
    pub const TOKENS: [&str; 8] = [
        "background",
        "surface",
        "text",
        "accent",
        "focus",
        "font",
        "spacing",
        "control_height",
    ];
    pub const fn named(_theme: &'static str) -> Self {
        Self {
            #[cfg(feature = "inspector")]
            theme: _theme,
            #[cfg(feature = "inspector")]
            declaration: None,
            #[cfg(feature = "inspector")]
            overrides: [None; 8],
        }
    }
    #[track_caller]
    pub const fn declared(self) -> Self {
        #[cfg(feature = "inspector")]
        {
            Self {
                declaration: Some(core::panic::Location::caller()),
                ..self
            }
        }
        #[cfg(not(feature = "inspector"))]
        {
            self
        }
    }
    #[track_caller]
    pub fn scoped(self, _changed: [bool; 8]) -> Self {
        #[cfg(feature = "inspector")]
        {
            let mut next = self;
            for (source, changed) in next.overrides.iter_mut().zip(_changed) {
                if changed {
                    *source = Some(core::panic::Location::caller());
                }
            }
            next
        }
        #[cfg(not(feature = "inspector"))]
        {
            self
        }
    }
}
#[cfg(feature = "inspector")]
#[derive(Default)]
pub struct StyleInspection {
    pub(crate) staging: alloc::vec::Vec<(crate::view::Key, StyleSource)>,
    active: alloc::vec::Vec<(crate::view::Key, StyleSource)>,
}
#[cfg(feature = "inspector")]
impl StyleInspection {
    pub fn commit(&mut self) {
        core::mem::swap(&mut self.active, &mut self.staging);
    }
    pub fn source(&self, key: crate::view::Key) -> Option<&StyleSource> {
        self.active
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, source)| source)
    }
}
