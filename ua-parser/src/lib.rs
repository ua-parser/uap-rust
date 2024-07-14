#![deny(unsafe_code)]
#![warn(missing_docs)]
#![allow(clippy::empty_docs)]
#![doc = include_str!("../README.md")]

use regex::Captures;
use serde::Deserialize;

pub use regex_filtered::{BuildError, ParseError};

mod resolvers;

/// Error returned if the conversion of [`Regexes`] to [`Extractor`]
/// fails.
#[derive(Debug)]
pub enum Error {
    /// Compilation failed because one of the input regexes could not
    /// be parsed or processed.
    ParseError(ParseError),
    /// Compilation failed because one of the prefilters could not be
    /// built.
    BuildError(BuildError),
    /// A replacement template requires a group missing from the regex
    MissingGroup(usize),
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::ParseError(p) => Some(p),
            Error::BuildError(b) => Some(b),
            Error::MissingGroup(_) => None,
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl From<ParseError> for Error {
    fn from(value: ParseError) -> Self {
        Self::ParseError(value)
    }
}
impl From<BuildError> for Error {
    fn from(value: BuildError) -> Self {
        Self::BuildError(value)
    }
}

/// Deserialization target for the parser descriptors, can be used
/// with the relevant serde implementation to load from `regexes.yaml`
/// or a conversion thereof.
///
/// Can then be compiled to a full [`Extractor`], or an individual
/// list of parsers can be converted to the corresponding extractor.
#[allow(missing_docs)]
#[derive(Deserialize)]
pub struct Regexes<'a> {
    pub user_agent_parsers: Vec<user_agent::Parser<'a>>,
    pub os_parsers: Vec<os::Parser<'a>>,
    pub device_parsers: Vec<device::Parser<'a>>,
}

impl<'a> TryFrom<Regexes<'a>> for Extractor<'a> {
    type Error = Error;
    /// Compile parsed regexes to the corresponding full extractor.
    ///
    /// Prefer using individual builder / extractors if you don't need
    /// all three domains extracted, as creating the individual
    /// extractors does have a cost.
    fn try_from(r: Regexes<'a>) -> Result<Self, Error> {
        let ua = r
            .user_agent_parsers
            .into_iter()
            .try_fold(user_agent::Builder::new(), |b, p| b.push(p))?
            .build()?;
        let os = r
            .os_parsers
            .into_iter()
            .try_fold(os::Builder::new(), |b, p| b.push(p))?
            .build()?;
        let dev = r
            .device_parsers
            .into_iter()
            .try_fold(device::Builder::new(), |b, p| b.push(p))?
            .build()?;
        Ok(Extractor { ua, os, dev })
    }
}

/// Full extractor, simply delegates to the underlying individual
/// extractors for the actual job.
#[allow(missing_docs)]
pub struct Extractor<'a> {
    pub ua: user_agent::Extractor<'a>,
    pub os: os::Extractor<'a>,
    pub dev: device::Extractor<'a>,
}
impl<'a> Extractor<'a> {
    /// Performs the extraction on every sub-extractor in sequence.
    pub fn extract(
        &'a self,
        ua: &'a str,
    ) -> (
        Option<user_agent::ValueRef<'a>>,
        Option<os::ValueRef<'a>>,
        Option<device::ValueRef<'a>>,
    ) {
        (
            self.ua.extract(ua),
            self.os.extract(ua),
            self.dev.extract(ua),
        )
    }
}

/// User agent module.
///
/// The user agent is the representation of the browser, in UAP lingo
/// the user agent is composed of a *family* (the browser project) and
/// a *version* of up to 4 segments.
pub mod user_agent {
    use serde::Deserialize;
    use std::borrow::Cow;

    use crate::resolvers::{FallbackResolver, FamilyResolver};
    use regex_filtered::BuildError;

    /// Individual user agent parser description. Plain data which can
    /// be deserialized from serde-compatible storage, or created
    /// literally (e.g. using a conversion or build script).
    #[derive(Deserialize, Default)]
    pub struct Parser<'a> {
        /// Regex to check the UA against, if the regex matches the
        /// parser applies.
        pub regex: Cow<'a, str>,
        /// If set, used for the [`ValueRef::family`] field. If it
        /// contains a `$1` placeholder, that is replaced by the value
        /// of the first match group.
        ///
        /// If unset, the first match group is used directly.
        pub family_replacement: Option<Cow<'a, str>>,
        /// If set, provides the value of the major version number,
        /// otherwise the second match group is used.
        pub v1_replacement: Option<Cow<'a, str>>,
        /// If set, provides the value of the minor version number,
        /// otherwise the third match group is used.
        pub v2_replacement: Option<Cow<'a, str>>,
        /// If set, provides the value of the patch version number,
        /// otherwise the fourth match group is used.
        pub v3_replacement: Option<Cow<'a, str>>,
        /// If set, provides the value of the minor patch version
        /// number, otherwise the fifth match group is used.
        pub v4_replacement: Option<Cow<'a, str>>,
    }

    type Repl<'a> = (
        FamilyResolver<'a>,
        // Per spec, should actually be restrict-templated (same as
        // family but for indexes 2-5 instead of 1).
        FallbackResolver<'a>,
        FallbackResolver<'a>,
        FallbackResolver<'a>,
        FallbackResolver<'a>,
    );

    /// Extractor builder, used to `push` parsers into before building
    /// the extractor.
    #[derive(Default)]
    pub struct Builder<'a> {
        builder: regex_filtered::Builder,
        repl: Vec<Repl<'a>>,
    }
    impl<'a> Builder<'a> {
        /// Initialise an empty builder.
        pub fn new() -> Self {
            Self::default()
        }

        /// Build the extractor, may be called without pushing any
        /// parser in though that is not very useful.
        pub fn build(self) -> Result<Extractor<'a>, BuildError> {
            let Self { builder, repl } = self;

            Ok(Extractor {
                matcher: builder.build()?,
                repl,
            })
        }

        /// Pushes a parser into the builder, may fail if the
        /// [`Parser::regex`] is invalid.
        pub fn push(mut self, ua: Parser<'a>) -> Result<Self, super::Error> {
            self.builder = self.builder.push(&super::rewrite_regex(&ua.regex))?;
            let r = &self.builder.regexes()[self.builder.regexes().len() - 1];
            // number of groups in regex, excluding implicit entire match group
            let groups = r.captures_len() - 1;
            self.repl.push((
                FamilyResolver::new(ua.family_replacement, groups)?,
                FallbackResolver::new(ua.v1_replacement, groups, 2),
                FallbackResolver::new(ua.v2_replacement, groups, 3),
                FallbackResolver::new(ua.v3_replacement, groups, 4),
                FallbackResolver::new(ua.v4_replacement, groups, 5),
            ));
            Ok(self)
        }

        /// Bulk loading of parsers into the builder.
        pub fn push_all<I>(self, ua: I) -> Result<Self, super::Error>
        where
            I: IntoIterator<Item = Parser<'a>>,
        {
            ua.into_iter().try_fold(self, |s, p| s.push(p))
        }
    }

    /// User Agent extractor.
    pub struct Extractor<'a> {
        matcher: regex_filtered::Regexes,
        repl: Vec<Repl<'a>>,
    }
    impl<'a> Extractor<'a> {
        /// Tries the loaded [`Parser`], upon finding the first
        /// matching [`Parser`] performs data extraction following its
        /// replacement directives and returns the result.
        ///
        /// Returns [`None`] if:
        ///
        /// - no matching parser was found
        /// - the match does not have any matching groups *and*
        ///   [`Parser::family_replacement`] is unset
        /// - [`Parser::family_replacement`] has a substitution
        ///   but there is no group in the regex
        pub fn extract(&'a self, ua: &'a str) -> Option<ValueRef<'a>> {
            let (idx, re) = self.matcher.matching(ua).next()?;
            let c = re.captures(ua)?;

            let (f, v1, v2, v3, v4) = &self.repl[idx];

            Some(ValueRef {
                family: f.resolve(&c),
                major: v1.resolve(&c),
                minor: v2.resolve(&c),
                patch: v3.resolve(&c),
                patch_minor: v4.resolve(&c),
            })
        }
    }
    /// Borrowed extracted value, borrows the content of the original
    /// parser or the content of the user agent string, unless a
    /// replacement is performed. (which is only possible for the )
    #[derive(PartialEq, Eq, Default, Debug)]
    pub struct ValueRef<'a> {
        ///
        pub family: Cow<'a, str>,
        ///
        pub major: Option<&'a str>,
        ///
        pub minor: Option<&'a str>,
        ///
        pub patch: Option<&'a str>,
        ///
        pub patch_minor: Option<&'a str>,
    }

    impl ValueRef<'_> {
        /// Converts the borrowed result into an owned one,
        /// independent from both the extractor and the user agent
        /// string.
        pub fn into_owned(self) -> Value {
            Value {
                family: self.family.into_owned(),
                major: self.major.map(|c| c.to_string()),
                minor: self.minor.map(|c| c.to_string()),
                patch: self.patch.map(|c| c.to_string()),
                patch_minor: self.patch_minor.map(|c| c.to_string()),
            }
        }
    }

    /// Owned extracted value, identical to [`ValueRef`] but not
    /// linked to either the UA string or the extractor.
    #[derive(PartialEq, Eq, Default, Debug)]
    pub struct Value {
        ///
        pub family: String,
        ///
        pub major: Option<String>,
        ///
        pub minor: Option<String>,
        ///
        pub patch: Option<String>,
        ///
        pub patch_minor: Option<String>,
    }
}

/// OS extraction module
pub mod os {
    use serde::Deserialize;
    use std::borrow::Cow;

    use regex_filtered::{BuildError, ParseError};

    use crate::resolvers::{OptResolver, Resolver};

    /// OS parser configuration
    #[derive(Deserialize, Default)]
    pub struct Parser<'a> {
        ///
        pub regex: Cow<'a, str>,
        /// Replacement for the [`ValueRef::os`], must be set if there
        /// is no capture in the [`Self::regex`], if there are
        /// captures may be fully templated (with `$n` placeholders
        /// for any group of the [`Self::regex`]).
        pub os_replacement: Option<Cow<'a, str>>,
        /// Replacement for the [`ValueRef::major`], may be fully templated.
        pub os_v1_replacement: Option<Cow<'a, str>>,
        /// Replacement for the [`ValueRef::minor`], may be fully templated.
        pub os_v2_replacement: Option<Cow<'a, str>>,
        /// Replacement for the [`ValueRef::patch`], may be fully templated.
        pub os_v3_replacement: Option<Cow<'a, str>>,
        /// Replacement for the [`ValueRef::patch_minor`], may be fully templated.
        pub os_v4_replacement: Option<Cow<'a, str>>,
    }
    /// Builder for [`Extractor`].
    #[derive(Default)]
    pub struct Builder<'a> {
        builder: regex_filtered::Builder,
        repl: Vec<(
            Resolver<'a>,
            OptResolver<'a>,
            OptResolver<'a>,
            OptResolver<'a>,
            OptResolver<'a>,
        )>,
    }
    impl<'a> Builder<'a> {
        ///
        pub fn new() -> Self {
            Self::default()
        }

        /// Builds the [`Extractor`], may fail if building the
        /// prefilter fails.
        pub fn build(self) -> Result<Extractor<'a>, BuildError> {
            let Self { builder, repl } = self;

            Ok(Extractor {
                matcher: builder.build()?,
                repl,
            })
        }

        /// Add a [`Parser`] configuration, fails if the regex can not
        /// be parsed, or if [`Parser::os_replacement`] is missing and
        /// the regex has no groups.
        pub fn push(mut self, os: Parser<'a>) -> Result<Self, ParseError> {
            self.builder = self.builder.push(&super::rewrite_regex(&os.regex))?;
            let r = &self.builder.regexes()[self.builder.regexes().len() - 1];
            // number of groups in regex, excluding implicit entire match group
            let groups = r.captures_len() - 1;
            self.repl.push((
                Resolver::new(os.os_replacement, groups, 1),
                OptResolver::new(os.os_v1_replacement, groups, 2),
                OptResolver::new(os.os_v2_replacement, groups, 3),
                OptResolver::new(os.os_v3_replacement, groups, 4),
                OptResolver::new(os.os_v4_replacement, groups, 5),
            ));
            Ok(self)
        }

        /// Bulk loading of parsers into the builder.
        pub fn push_all<I>(self, ua: I) -> Result<Self, ParseError>
        where
            I: IntoIterator<Item = Parser<'a>>,
        {
            ua.into_iter().try_fold(self, |s, p| s.push(p))
        }
    }

    /// OS extractor structure
    pub struct Extractor<'a> {
        matcher: regex_filtered::Regexes,
        repl: Vec<(
            Resolver<'a>,
            OptResolver<'a>,
            OptResolver<'a>,
            OptResolver<'a>,
            OptResolver<'a>,
        )>,
    }
    impl<'a> Extractor<'a> {
        /// Matches & extracts the OS data for this user agent,
        /// returns `None` if the UA string could not be matched.
        pub fn extract(&'a self, ua: &'a str) -> Option<ValueRef<'a>> {
            let (idx, re) = self.matcher.matching(ua).next()?;
            let c = re.captures(ua)?;

            let (o, v1, v2, v3, v4) = &self.repl[idx];

            Some(ValueRef {
                os: o.resolve(&c),
                major: v1.resolve(&c),
                minor: v2.resolve(&c),
                patch: v3.resolve(&c),
                patch_minor: v4.resolve(&c),
            })
        }
    }

    /// An OS extraction result.
    #[derive(PartialEq, Eq, Default, Debug)]
    pub struct ValueRef<'a> {
        ///
        pub os: Cow<'a, str>,
        ///
        pub major: Option<Cow<'a, str>>,
        ///
        pub minor: Option<Cow<'a, str>>,
        ///
        pub patch: Option<Cow<'a, str>>,
        ///
        pub patch_minor: Option<Cow<'a, str>>,
    }

    impl ValueRef<'_> {
        /// Converts a [`ValueRef`] into a [`Value`] to avoid lifetime
        /// concerns, may need to allocate and copy any data currently
        /// borrowed from a [`Parser`] or user agent string.
        pub fn into_owned(self) -> Value {
            Value {
                os: self.os.into_owned(),
                major: self.major.map(|c| c.into_owned()),
                minor: self.minor.map(|c| c.into_owned()),
                patch: self.patch.map(|c| c.into_owned()),
                patch_minor: self.patch_minor.map(|c| c.into_owned()),
            }
        }
    }

    /// Owned version of [`ValueRef`].
    #[derive(PartialEq, Eq, Default, Debug)]
    pub struct Value {
        ///
        pub os: String,
        ///
        pub major: Option<String>,
        ///
        pub minor: Option<String>,
        ///
        pub patch: Option<String>,
        ///
        pub patch_minor: Option<String>,
    }
}

/// Extraction module for the device data of the user agent string.
pub mod device {
    use serde::Deserialize;
    use std::borrow::Cow;

    use regex_filtered::{BuildError, ParseError};

    use crate::resolvers::{OptResolver, Resolver};

    /// regex flags
    #[derive(Deserialize, PartialEq, Eq)]
    pub enum Flag {
        /// Enables case-insensitive regex matching, deserializes from
        /// the string `"i"`
        #[serde(rename = "i")]
        IgnoreCase,
    }
    /// Device parser description.
    #[derive(Deserialize, Default)]
    pub struct Parser<'a> {
        /// Regex pattern to use for matching and data extraction.
        pub regex: Cow<'a, str>,
        /// Configuration flags for the regex, if any.
        pub regex_flag: Option<Flag>,
        /// Device replacement data, fully templated, must be present
        /// *or* the regex must have at least one group, which will be
        /// used instead.
        pub device_replacement: Option<Cow<'a, str>>,
        /// Brand replacement data, fully templated, optional, if
        /// missing there is no fallback.
        pub brand_replacement: Option<Cow<'a, str>>,
        /// Model replacement data, fully templated, optional, if
        /// missing will be replaced by the first group if the regex
        /// has one.
        pub model_replacement: Option<Cow<'a, str>>,
    }

    /// Extractor builder.
    #[derive(Default)]
    pub struct Builder<'a> {
        builder: regex_filtered::Builder,
        repl: Vec<(Resolver<'a>, OptResolver<'a>, OptResolver<'a>)>,
    }
    impl<'a> Builder<'a> {
        /// Creates a builder in the default configurtion, which is
        /// the only configuration.
        pub fn new() -> Self {
            Self::default()
        }

        /// Builds an Extractor, may fail if compiling the prefilter fails.
        pub fn build(self) -> Result<Extractor<'a>, BuildError> {
            let Self { builder, repl } = self;

            Ok(Extractor {
                matcher: builder.build()?,
                repl,
            })
        }

        /// Add a parser to the set, may fail if parsing the regex
        /// fails *or* if [`Parser::device_replacement`] is unset and
        /// [`Parser::regex`] does not have at least one group, or a
        /// templated [`Parser::device_replacement`] requests groups
        /// which [`Parser::regex`] is missing.
        pub fn push(mut self, device: Parser<'a>) -> Result<Self, ParseError> {
            self.builder = self.builder.push_opt(
                &super::rewrite_regex(&device.regex),
                regex_filtered::Options::new()
                    .case_insensitive(device.regex_flag == Some(Flag::IgnoreCase)),
            )?;
            let r = &self.builder.regexes()[self.builder.regexes().len() - 1];
            // number of groups in regex, excluding implicit entire match group
            let groups = r.captures_len() - 1;
            self.repl.push((
                Resolver::new(device.device_replacement, groups, 1),
                OptResolver::new(device.brand_replacement, 0, 999),
                OptResolver::new(device.model_replacement, groups, 1),
            ));
            Ok(self)
        }

        /// Bulk loading of parsers into the builder.
        pub fn push_all<I>(self, ua: I) -> Result<Self, ParseError>
        where
            I: IntoIterator<Item = Parser<'a>>,
        {
            ua.into_iter().try_fold(self, |s, p| s.push(p))
        }
    }

    /// Device extractor object.
    pub struct Extractor<'a> {
        matcher: regex_filtered::Regexes,
        repl: Vec<(Resolver<'a>, OptResolver<'a>, OptResolver<'a>)>,
    }
    impl<'a> Extractor<'a> {
        /// Perform data extraction from the user agent string,
        /// returns `None` if no regex in the [`Extractor`] matches
        /// the input.
        pub fn extract(&'a self, ua: &'a str) -> Option<ValueRef<'a>> {
            let (idx, re) = self.matcher.matching(ua).next()?;
            let c = re.captures(ua)?;

            let (d, v1, v2) = &self.repl[idx];

            Some(ValueRef {
                device: d.resolve(&c),
                brand: v1.resolve(&c),
                model: v2.resolve(&c),
            })
        }
    }

    /// Extracted device content, may borrow from one of the
    /// [`Parser`] or from the user agent string.
    #[derive(PartialEq, Eq, Default, Debug)]
    pub struct ValueRef<'a> {
        ///
        pub device: Cow<'a, str>,
        ///
        pub brand: Option<Cow<'a, str>>,
        ///
        pub model: Option<Cow<'a, str>>,
    }

    impl ValueRef<'_> {
        /// Converts [`Self`] to an owned [`Value`] getting rid of
        /// borrowing concerns, may need to allocate and copy if any
        /// of the attributes actually borrows from a [`Parser`] or
        /// the user agent string.
        pub fn into_owned(self) -> Value {
            Value {
                device: self.device.into_owned(),
                brand: self.brand.map(|c| c.into_owned()),
                model: self.model.map(|c| c.into_owned()),
            }
        }
    }

    /// Owned version of [`ValueRef`].
    #[derive(PartialEq, Eq, Default, Debug)]
    pub struct Value {
        ///
        pub device: String,
        ///
        pub brand: Option<String>,
        ///
        pub model: Option<String>,
    }
}

/// Rewrites a regex's character classes to ascii and bounded
/// repetitions to unbounded, the second to reduce regex memory
/// requirements, and the first for both that and to better match the
/// (inferred) semantics intended for ua-parser.
fn rewrite_regex(re: &str) -> std::borrow::Cow<'_, str> {
    let mut from = 0;
    let mut out = String::new();

    let mut it = re.char_indices();
    let mut escape = false;
    let mut inclass = 0;
    'main: while let Some((idx, c)) = it.next() {
        match c {
            '\\' if !escape => {
                escape = true;
                continue;
            }
            '{' if !escape && inclass == 0 => {
                if idx == 0 {
                    // we're repeating nothing, this regex is broken, bail
                    return re.into();
                }
                // we don't need to loop, we only want to replace {0, ...} and {1, ...}
                let Some((_, start)) = it.next() else {
                    continue;
                };
                if start != '0' && start != '1' {
                    continue;
                }

                if !matches!(it.next(), Some((_, ','))) {
                    continue;
                }

                let mut digits = 0;
                for (ri, rc) in it.by_ref() {
                    match rc {
                        '}' if digits > 2 => {
                            // here idx is the index of the start of
                            // the range and ri is the end of range
                            out.push_str(&re[from..idx]);
                            from = ri + 1;
                            out.push_str(if start == '0' { "*" } else { "+" });
                            break;
                        }
                        c if c.is_ascii_digit() => {
                            digits += 1;
                        }
                        _ => continue 'main,
                    }
                }
            }
            '[' if !escape => {
                inclass += 1;
            }
            ']' if !escape => {
                inclass += 1;
            }
            // no need for special cases because regex allows nesting
            // character classes, whereas js or python don't \o/
            'd' if escape => {
                // idx is d so idx-1 is \\, and we want to exclude it
                out.push_str(&re[from..idx - 1]);
                from = idx + 1;
                out.push_str("[0-9]");
            }
            'D' if escape => {
                out.push_str(&re[from..idx - 1]);
                from = idx + 1;
                out.push_str("[^0-9]");
            }
            'w' if escape => {
                out.push_str(&re[from..idx - 1]);
                from = idx + 1;
                out.push_str("[A-Za-z0-9_]");
            }
            'W' if escape => {
                out.push_str(&re[from..idx - 1]);
                from = idx + 1;
                out.push_str("[^A-Za-z0-9_]");
            }
            _ => (),
        }
        escape = false;
    }

    if from == 0 {
        re.into()
    } else {
        out.push_str(&re[from..]);
        out.into()
    }
}

#[cfg(test)]
mod test_rewrite_regex {
    use super::rewrite_regex as rewrite;

    #[test]
    fn ignore_small_repetition() {
        assert_eq!(rewrite(".{0,2}x"), ".{0,2}x");
        assert_eq!(rewrite(".{0,}"), ".{0,}");
        assert_eq!(rewrite(".{1,}"), ".{1,}");
    }

    #[test]
    fn rewrite_large_repetitions() {
        assert_eq!(rewrite(".{0,20}x"), ".{0,20}x");
        assert_eq!(rewrite("(.{0,100})"), "(.*)");
        assert_eq!(rewrite("(.{1,50})"), "(.{1,50})");
        assert_eq!(rewrite(".{1,300}x"), ".+x");
    }

    #[test]
    fn ignore_non_repetitions() {
        assert_eq!(
            rewrite(r"\{1,2}"),
            r"\{1,2}",
            "if the opening brace is escaped it's not a repetition"
        );
        assert_eq!(
            rewrite("[.{1,100}]"),
            "[.{1,100}]",
            "inside a set it's not a repetition"
        );
    }

    #[test]
    fn rewrite_classes() {
        assert_eq!(rewrite(r"\dx"), "[0-9]x");
        assert_eq!(rewrite(r"\wx"), "[A-Za-z0-9_]x");
        assert_eq!(rewrite(r"[\d]x"), r"[[0-9]]x");
    }
}
