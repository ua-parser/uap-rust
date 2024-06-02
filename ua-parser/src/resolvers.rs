// TODO: what happens in case of optional groups?
//
// Sadly regex offers no way to actually query that nicely: via
// static_captures_len it only specifies whether all groups are
// required, if any group is optional that returns `None`.

use crate::Error;
use regex::Captures;
use std::borrow::Cow;

fn get<'s>(c: &Captures<'s>, group: usize) -> Option<&'s str> {
    c.get(group).map(|g| g.as_str()).filter(|s| !s.is_empty())
}

// TODO:
// - memchr?
// - u16 checks against u16 buffer (check all positions)?
// - svar/simd?
fn has_substitution(s: &str) -> bool {
    debug_assert!(!s.is_empty());
    std::iter::zip(s.as_bytes(), &s.as_bytes()[1..]).any(|(&d, n)| d == b'$' && n.is_ascii_digit())
}

/// Resolver with full templating: the template string can contain
/// $1-9 markers which get replaced by the corresponding regex string.
///
/// - if there is a non-null replacement pattern, then it must be used with
///   match groups as template parameters (at indices 1+)
///   - the result is stripped
///   - if it is an empty string, then it's replaced by a null
/// - otherwise fallback to a (possibly optional) match group
/// - or null (device brand has no fallback)
pub(crate) enum Resolver<'a> {
    Replacement(Cow<'a, str>),
    Capture(usize),
    Template(Cow<'a, str>),
}
impl<'a> Resolver<'a> {
    pub(crate) fn new(repl: Option<Cow<'a, str>>, groups: usize, idx: usize) -> Self {
        if let Some(s) = repl.filter(|s| !s.trim().is_empty()) {
            if has_substitution(&s) {
                Self::Template(s)
            } else {
                Self::Replacement(s)
            }
        } else if groups >= idx {
            Self::Capture(idx)
        } else {
            Self::Replacement("".into())
        }
    }

    pub(crate) fn resolve(&'a self, c: &Captures<'a>) -> Cow<'a, str> {
        match self {
            Self::Replacement(s) => (**s).into(),
            Self::Capture(i) => get(c, *i).unwrap_or("").into(),
            Self::Template(t) => {
                let mut r = String::new();
                c.expand(t, &mut r);
                let trimmed = r.trim();
                if r.len() == trimmed.len() {
                    r.into()
                } else {
                    trimmed.to_string().into()
                }
            }
        }
    }
}

/// Similar to [`Resolver`] but allows a [`None`] aka no resolution.
pub(crate) enum OptResolver<'a> {
    None,
    Replacement(Cow<'a, str>),
    Capture(usize),
    Template(Cow<'a, str>),
}
impl<'a> OptResolver<'a> {
    pub(crate) fn new(repl: Option<Cow<'a, str>>, groups: usize, idx: usize) -> Self {
        if let Some(s) = repl.filter(|s| !s.trim().is_empty()) {
            if has_substitution(&s) {
                Self::Template(s)
            } else {
                Self::Replacement(s)
            }
        } else if groups >= idx {
            Self::Capture(idx)
        } else {
            Self::None
        }
    }

    pub(crate) fn resolve(&'a self, c: &Captures<'a>) -> Option<Cow<'a, str>> {
        match self {
            Self::None => None,
            Self::Replacement(s) => Some((**s).into()),
            Self::Capture(i) => get(c, *i).map(From::from),
            Self::Template(t) => {
                let mut r = String::new();
                c.expand(t, &mut r);
                let trimmed = r.trim();
                if trimmed.is_empty() {
                    None
                } else if r.len() == trimmed.len() {
                    Some(r.into())
                } else {
                    Some(trimmed.to_string().into())
                }
            }
        }
    }
}

/// Dedicated restrict-templated resolver for UserAgent#family:
/// supports templating in the replacement, but only for the `$1`
/// value / group.
pub(crate) enum FamilyResolver<'a> {
    Capture,
    Replacement(Cow<'a, str>),
    Template(Cow<'a, str>),
}
impl<'a> FamilyResolver<'a> {
    pub(crate) fn new(repl: Option<Cow<'a, str>>, groups: usize) -> Result<Self, Error> {
        match repl {
            Some(s) if s.contains("$1") => {
                if groups < 1 {
                    Err(Error::MissingGroup(1))
                } else {
                    Ok(FamilyResolver::Template(s))
                }
            }
            Some(s) if !s.is_empty() => Ok(FamilyResolver::Replacement(s)),
            _ if groups >= 1 => Ok(FamilyResolver::Capture),
            _ => Ok(FamilyResolver::Replacement("".into())),
        }
    }

    pub(crate) fn resolve(&'a self, c: &super::Captures<'a>) -> Cow<'a, str> {
        match self {
            FamilyResolver::Capture => get(c, 1).unwrap_or("").into(),
            FamilyResolver::Replacement(s) => (**s).into(),
            FamilyResolver::Template(t) => t.replace("$1", get(c, 1).unwrap_or("")).into(),
        }
    }
}

/// Untemplated resolver, the replacement value is used as-is if
/// present.
pub(crate) enum FallbackResolver<'a> {
    None,
    Capture(usize),
    Replacement(Cow<'a, str>),
}
impl<'a> FallbackResolver<'a> {
    pub(crate) fn new(repl: Option<Cow<'a, str>>, groups: usize, idx: usize) -> Self {
        if let Some(s) = repl.filter(|s| !s.is_empty()) {
            Self::Replacement(s)
        } else if groups >= idx {
            Self::Capture(idx)
        } else {
            Self::None
        }
    }
    pub(crate) fn resolve(&'a self, c: &super::Captures<'a>) -> Option<&'a str> {
        match self {
            FallbackResolver::None => None,
            FallbackResolver::Capture(n) => get(c, *n),
            FallbackResolver::Replacement(r) => Some(r),
        }
    }
}
