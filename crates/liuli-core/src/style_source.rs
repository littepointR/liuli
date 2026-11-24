//! Theme provenance for the desktop inspector. Firmware builds use a zero-sized marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleSource {
    #[cfg(feature = "inspector")]
    pub theme: &'static str,
    #[cfg(feature = "inspector")]
    pub variant: Option<&'static str>,
    #[cfg(feature = "inspector")]
    pub material: Option<&'static str>,
    #[cfg(feature = "inspector")]
    pub declaration: Option<&'static core::panic::Location<'static>>,
    #[cfg(feature = "inspector")]
    pub overrides: [Option<&'static core::panic::Location<'static>>; 14],
}
impl StyleSource {
    pub const TOKENS: [&str; 14] = [
        "background",
        "surface",
        "border",
        "text",
        "accent",
        "semantic",
        "focus",
        "press",
        "font",
        "density",
        "shape",
        "elevation",
        "glass",
        "effects",
    ];
    pub const fn named(_theme: &'static str) -> Self {
        Self {
            #[cfg(feature = "inspector")]
            theme: _theme,
            #[cfg(feature = "inspector")]
            variant: None,
            #[cfg(feature = "inspector")]
            material: None,
            #[cfg(feature = "inspector")]
            declaration: None,
            #[cfg(feature = "inspector")]
            overrides: [None; 14],
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
    pub const fn with_variant(self, _variant: &'static str) -> Self {
        #[cfg(feature = "inspector")]
        {
            Self {
                variant: Some(_variant),
                ..self
            }
        }
        #[cfg(not(feature = "inspector"))]
        {
            self
        }
    }
    pub const fn with_material(self, _material: &'static str) -> Self {
        #[cfg(feature = "inspector")]
        {
            Self {
                material: Some(_material),
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
        self.scoped_tokens([
            _changed[0],
            _changed[1],
            _changed[1],
            _changed[2],
            _changed[3],
            _changed[3],
            _changed[4],
            false,
            _changed[5],
            _changed[6] || _changed[7],
            _changed[6],
            false,
            false,
            false,
        ])
    }
    #[track_caller]
    pub fn scoped_tokens(self, _changed: [bool; 14]) -> Self {
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
    #[track_caller]
    pub fn scoped_index(self, _index: usize) -> Self {
        #[cfg(feature = "inspector")]
        {
            let mut next = self;
            if let Some(slot) = next.overrides.get_mut(_index) {
                *slot = Some(core::panic::Location::caller());
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
