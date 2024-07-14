#![doc = include_str!("../README.md")]
#![deny(unsafe_code)]
#![warn(missing_docs)]

use aho_corasick::AhoCorasick;

mod int_set;
mod mapper;
mod model;
pub use model::Error as ModelError;

/// Builder for the regexes set
pub struct Builder {
    regexes: Vec<regex::Regex>,
    mapper_builder: mapper::Builder,
}

/// Parser configuration, can be used to tune the regex parsing when
/// adding it to the [`Builder`]. Every option defaults to `false`
/// whether through [`Default`] or [`Options::new`].
///
/// The parser can also be configured via standard [`regex`] inline
/// flags.
#[derive(Default)]
pub struct Options {
    case_insensitive: bool,
    dot_matches_new_line: bool,
    ignore_whitespace: bool,
    multi_line: bool,
    crlf: bool,
}

impl Options {
    /// Create a new options object.
    pub fn new() -> Self {
        Self::default()
    }
    /// Configures case-insensitive matching for the entire pattern.
    pub fn case_insensitive(&mut self, yes: bool) -> &mut Self {
        self.case_insensitive = yes;
        self
    }
    /// Configures `.` to match newline characters, by default `.`
    /// matches everything *except* newline characters.
    pub fn dot_matches_new_line(&mut self, yes: bool) -> &mut Self {
        self.dot_matches_new_line = yes;
        self
    }
    /// Configures ignoring whitespace inside patterns, as well as `#`
    /// line comments ("verbose" mode).
    ///
    /// Verbose mode is useful to break up complex regexes and improve
    /// their documentation.
    pub fn ignore_whitespace(&mut self, yes: bool) -> &mut Self {
        self.ignore_whitespace = yes;
        self
    }
    /// Configures multi-line mode. When enabled, `^` matches at every
    /// start of line and `$` at every end of line, by default they
    /// match only the start and end of the string respectively.ca
    pub fn multi_line(&mut self, yes: bool) -> &mut Self {
        self.multi_line = yes;
        self
    }
    /// Allows `\r` as a line terminator, by default only `\n` is a
    /// line terminator (relevant for [`Self::ignore_whitespace`] and
    /// [`Self::multi_line`]).
    pub fn crlf(&mut self, yes: bool) -> &mut Self {
        self.crlf = yes;
        self
    }
    fn to_regex(&self, pattern: &str) -> Result<regex::Regex, regex::Error> {
        regex::RegexBuilder::new(pattern)
            .case_insensitive(self.case_insensitive)
            .dot_matches_new_line(self.dot_matches_new_line)
            .ignore_whitespace(self.ignore_whitespace)
            .multi_line(self.multi_line)
            .crlf(self.crlf)
            .build()
    }
}
impl From<Options> for regex_syntax::Parser {
    fn from(opt: Options) -> Self {
        Self::from(&opt)
    }
}
impl From<&Options> for regex_syntax::Parser {
    fn from(
        Options {
            case_insensitive,
            dot_matches_new_line,
            ignore_whitespace,
            multi_line,
            crlf,
        }: &Options,
    ) -> Self {
        regex_syntax::ParserBuilder::new()
            .case_insensitive(*case_insensitive)
            .dot_matches_new_line(*dot_matches_new_line)
            .ignore_whitespace(*ignore_whitespace)
            .multi_line(*multi_line)
            .crlf(*crlf)
            .build()
    }
}

/// Parsing error when adding a new regex to the [`Builder`].
#[derive(Debug)]
pub enum ParseError {
    /// An error occurred while parsing the regex or translating it to
    /// HIR.
    SyntaxError(String),
    /// An error occurred while processing the regex for atom
    /// extraction.
    ProcessingError(ModelError),
    /// The regex was too large to compile to the NFA (within the
    /// default limits).
    RegexTooLarge(usize),
}
impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseError::ProcessingError(e) => Some(e),
            ParseError::SyntaxError(_) => None,
            ParseError::RegexTooLarge(_) => None,
        }
    }
}
impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl From<regex_syntax::Error> for ParseError {
    fn from(value: regex_syntax::Error) -> Self {
        Self::SyntaxError(value.to_string())
    }
}
impl From<regex::Error> for ParseError {
    fn from(value: regex::Error) -> Self {
        match value {
            regex::Error::CompiledTooBig(v) => Self::RegexTooLarge(v),
            e => Self::SyntaxError(e.to_string()),
        }
    }
}
impl From<ModelError> for ParseError {
    fn from(value: ModelError) -> Self {
        Self::ProcessingError(value)
    }
}

/// Error while compiling the builder to a prefiltered set.
#[derive(Debug)]
pub enum BuildError {
    /// Error while building the prefilter.
    PrefilterError(aho_corasick::BuildError),
}
impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BuildError::PrefilterError(p) => Some(p),
        }
    }
}
impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl From<aho_corasick::BuildError> for BuildError {
    fn from(value: aho_corasick::BuildError) -> Self {
        Self::PrefilterError(value)
    }
}

impl Builder {
    /// Instantiate a builder with the default metadata configuration:
    ///
    /// - minimum atom length 3
    #[must_use]
    pub fn new() -> Self {
        Self::new_atom_len(3)
    }

    /// Instantiate a builder with a custom minimum atom length.
    /// Increasing the atom length decreases the size and cost of the
    /// prefilter, but may make more regexes impossible to prefilter,
    /// which can increase matching costs.
    #[must_use]
    pub fn new_atom_len(min_atom_len: usize) -> Self {
        Self {
            regexes: Vec::new(),
            mapper_builder: mapper::Builder::new(min_atom_len),
        }
    }

    /// Currently loaded regexes.
    pub fn regexes(&self) -> &[regex::Regex] {
        &self.regexes
    }

    /// Push a single regex into the builder, using the default
    /// parsing options.
    pub fn push(self, s: &str) -> Result<Self, ParseError> {
        self.push_opt(s, &Options::new())
    }

    /// Push a single regex into the builder, using custom parsing
    /// options.
    pub fn push_opt(mut self, regex: &str, opts: &Options) -> Result<Self, ParseError> {
        let hir = regex_syntax::Parser::from(opts).parse(regex)?;
        let pf = model::Model::new(&hir)?;
        self.mapper_builder.push(pf);
        self.regexes.push(opts.to_regex(regex)?);
        Ok(self)
    }

    /// Push a batch of regexes into the builder, using the default
    /// parsing options.
    pub fn push_all<T, I>(self, i: I) -> Result<Self, ParseError>
    where
        T: AsRef<str>,
        I: IntoIterator<Item = T>,
    {
        i.into_iter().try_fold(self, |b, s| b.push(s.as_ref()))
    }

    /// Build the regexes set from the current builder.
    ///
    /// Building a regexes set from no regexes is useless but not an
    /// error.
    pub fn build(self) -> Result<Regexes, BuildError> {
        let Self {
            regexes,
            mapper_builder,
        } = self;
        let (mapper, atoms) = mapper_builder.build();

        // Instead of returning a bunch of atoms for the user to
        // manage, since `regex` depends on aho-corasick by default we
        // can use that directly and not bother the user.
        let prefilter = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .prefilter(true)
            .build(atoms)?;

        Ok(Regexes {
            regexes,
            mapper,
            prefilter,
        })
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

/// Regexes set, allows testing inputs against a *large* number of
/// *non-trivial* regexes.
pub struct Regexes {
    regexes: Vec<regex::Regex>,
    mapper: mapper::Mapper,
    prefilter: AhoCorasick,
}

impl Regexes {
    // TODO:
    // - number of tokens (prefilter.patterns_len())
    // - number of regexes
    // - number of unfiltered regexes (from mapper)
    // - ratio of checked regexes to successes (cfg-gated)
    // - total / prefiltered (- unfiltered?) so atom size can be manipulated
    #[inline]
    fn prefilter<'a>(&'a self, haystack: &'a str) -> impl Iterator<Item = usize> + 'a {
        self.prefilter
            .find_overlapping_iter(haystack)
            .map(|m| m.pattern().as_usize())
    }

    #[inline]
    fn prefiltered(&self, haystack: &str) -> impl Iterator<Item = usize> {
        self.mapper.atom_to_re(self.prefilter(haystack)).into_iter()
    }

    /// Returns *whether* any regex in the set matches the haystack.
    pub fn is_match(&self, haystack: &str) -> bool {
        self.prefiltered(haystack)
            .any(|idx| self.regexes[idx].is_match(haystack))
    }

    /// Yields the regexes matching the haystack along with their
    /// index.
    ///
    /// The results are guaranteed to be returned in ascending order.
    pub fn matching<'a>(
        &'a self,
        haystack: &'a str,
    ) -> impl Iterator<Item = (usize, &regex::Regex)> + 'a {
        self.prefiltered(haystack).filter_map(move |idx| {
            let r = &self.regexes[idx];
            r.is_match(haystack).then_some((idx, r))
        })
    }

    /// Returns a reference to all the regexes in the set.
    pub fn regexes(&self) -> &[regex::Regex] {
        &self.regexes
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use itertools::Itertools;

    #[test]
    fn empty_filter() {
        let f = Builder::new().build().unwrap();
        assert_eq!(f.prefilter("0123").collect_vec(), vec![]);

        assert_eq!(f.matching("foo").count(), 0);
    }

    #[test]
    fn empty_pattern() {
        let f = Builder::new().push("").unwrap().build().unwrap();

        assert_eq!(f.prefilter("0123").collect_vec(), vec![]);

        assert_eq!(
            f.matching("0123").map(|(idx, _)| idx).collect_vec(),
            vec![0]
        );
    }

    #[test]
    fn small_or_test() {
        let f = Builder::new_atom_len(4)
            .push("(foo|bar)")
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(f.prefilter("lemurs bar").collect_vec(), vec![]);

        assert_eq!(
            f.matching("lemurs bar").map(|(idx, _)| idx).collect_vec(),
            vec![0],
        );

        let f = Builder::new().push("(foo|bar)").unwrap().build().unwrap();

        assert_eq!(f.prefilter("lemurs bar").collect_vec(), vec![1]);

        assert_eq!(
            f.matching("lemurs bar").map(|(idx, _)| idx).collect_vec(),
            vec![0],
        );
    }

    #[test]
    fn basic_matches() {
        let f = Builder::new()
            .push("(abc123|abc|defxyz|ghi789|abc1234|xyz).*[x-z]+")
            .unwrap()
            .push("abcd..yyy..yyyzzz")
            .unwrap()
            .push("mnmnpp[a-z]+PPP")
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(
            f.matching("abc121212xyz").map(|(idx, _)| idx).collect_vec(),
            vec![0],
        );

        assert_eq!(
            f.matching("abc12312yyyzzz")
                .map(|(idx, _)| idx)
                .collect_vec(),
            vec![0],
        );

        assert_eq!(
            f.matching("abcd12yyy32yyyzzz")
                .map(|(idx, _)| idx)
                .collect_vec(),
            vec![0, 1],
        );
    }

    #[test]
    fn basics() {
        // In re2 this is the `MoveSemantics` test, which is... so not
        // necessary for us. But it's a pair of extra regexes we can
        // test

        let f = Builder::new().push("foo\\d+").unwrap().build().unwrap();

        assert_eq!(
            f.matching("abc foo1 xyz").map(|(idx, _)| idx).collect_vec(),
            vec![0],
        );
        assert_eq!(
            f.matching("abc bar2 xyz").map(|(idx, _)| idx).collect_vec(),
            vec![],
        );

        let f = Builder::new().push("bar\\d+").unwrap().build().unwrap();

        assert_eq!(
            f.matching("abc foo1 xyz").map(|(idx, _)| idx).collect_vec(),
            vec![],
        );
        assert_eq!(
            f.matching("abc bar2 xyz").map(|(idx, _)| idx).collect_vec(),
            vec![0],
        );
    }

    #[test]
    fn bulk_api() {
        use std::io::BufRead as _;

        Builder::new().push_all(["a", "b"]).unwrap();

        Builder::new()
            .push_all(vec!["a".to_string(), "b".to_string()])
            .unwrap();

        Builder::new().push_all("a\nb\nc\nd\n".lines()).unwrap();

        Builder::new()
            .push_all(b"a\nb\nc\nd\n".lines().map(|l| l.unwrap()))
            .unwrap();
    }
}
