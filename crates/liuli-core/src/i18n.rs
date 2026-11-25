use core::fmt;

/// Compact catalog key index into a static [`LocaleTable`].
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(transparent)]
pub struct TextKey(pub u16);

/// Locale table index into a slice of [`LocaleTable`]s (index `0` is the fallback locale).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(transparent)]
pub struct LocaleId(pub u8);

/// Basic plural category matching integer quantities across embedded UI locales.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum PluralForm {
    One,
    Other,
}

#[inline]
pub const fn plural_en(count: u64) -> PluralForm {
    if count == 1 {
        PluralForm::One
    } else {
        PluralForm::Other
    }
}

#[inline]
pub const fn plural_cjk(_count: u64) -> PluralForm {
    PluralForm::Other
}

/// Static compile-time string table for a single locale (`#![no_std]`, zero heap allocation).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct LocaleTable {
    pub code: &'static str,
    pub entries: &'static [&'static str],
}

impl LocaleTable {
    #[inline]
    pub const fn new(code: &'static str, entries: &'static [&'static str]) -> Self {
        Self { code, entries }
    }

    /// Look up `key` in this locale, returning `None` if out of bounds or empty.
    #[inline]
    pub fn get(&self, key: TextKey) -> Option<&'static str> {
        self.entries
            .get(usize::from(key.0))
            .copied()
            .filter(|s| !s.is_empty())
    }

    /// Resolve `key` for `locale` from `locales`, falling back to locale `0` and then `""`.
    pub fn resolve(locales: &[LocaleTable], locale: LocaleId, key: TextKey) -> &'static str {
        if let Some(tbl) = locales.get(usize::from(locale.0))
            && let Some(s) = tbl.get(key)
        {
            return s;
        }
        locales.first().and_then(|tbl| tbl.get(key)).unwrap_or("")
    }

    /// Find the [`LocaleId`] matching `code` (case-insensitive ASCII comparison).
    pub fn find_locale(locales: &[LocaleTable], code: &str) -> Option<LocaleId> {
        locales
            .iter()
            .position(|t| t.code.eq_ignore_ascii_case(code))
            .and_then(|i| u8::try_from(i).ok())
            .map(LocaleId)
    }
}

/// Format `template` containing positional placeholders (`{0}`, `{1}`, ...) into `out`.
///
/// Supports escaped braces (`{{` -> `{`, `}}` -> `}`). Unrecognized or out-of-bounds
/// placeholders are written verbatim so malformed translations never panic on an MCU.
pub fn format_template(
    out: &mut impl fmt::Write,
    template: &str,
    args: &[&dyn fmt::Display],
) -> fmt::Result {
    let bytes = template.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                out.write_char('{')?;
                i += 2;
                continue;
            }
            if let Some(rel_end) = bytes[i + 1..].iter().position(|&b| b == b'}') {
                let inner = &template[i + 1..i + 1 + rel_end];
                if !inner.is_empty()
                    && inner.bytes().all(|b| b.is_ascii_digit())
                    && let Ok(arg_idx) = inner.parse::<usize>()
                    && let Some(val) = args.get(arg_idx)
                {
                    out.write_fmt(format_args!("{}", *val))?;
                    i += rel_end + 2;
                    continue;
                }
            }
        } else if bytes[i] == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}' {
            out.write_char('}')?;
            i += 2;
            continue;
        }
        let ch = template[i..].chars().next().unwrap_or('\0');
        out.write_char(ch)?;
        i += ch.len_utf8();
    }
    Ok(())
}

/// Zero-allocation [`fmt::Display`] wrapper for passing localized templates into `ViewBuilder::text_fmt`.
pub struct TemplateDisplay<'a, 'b> {
    template: &'a str,
    args: &'a [&'b dyn fmt::Display],
}

impl<'a, 'b> TemplateDisplay<'a, 'b> {
    #[inline]
    pub const fn new(template: &'a str, args: &'a [&'b dyn fmt::Display]) -> Self {
        Self { template, args }
    }
}

impl fmt::Display for TemplateDisplay<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_template(f, self.template, self.args)
    }
}
