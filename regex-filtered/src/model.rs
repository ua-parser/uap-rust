use itertools::iproduct;
use regex_syntax::hir::{self, visit, Hir, HirKind, Visitor};
use std::cell::Cell;
use std::fmt::{Display, Formatter, Write};
use std::str::Utf8Error;
use std::{collections::BTreeSet, ops::Deref};

#[derive(Clone, Debug)]
pub enum Model {
    /// Everything matches.
    All(Cell<usize>),
    /// Nothing matches.
    None(Cell<usize>),
    /// The string matches.
    Atom(Cell<usize>, String),
    /// All sub-filters must match.
    And(Cell<usize>, Vec<Model>),
    /// One sub-filter must match.
    Or(Cell<usize>, Vec<Model>),
}
use Model::{All, And, Atom, None, Or};

impl std::hash::Hash for Model {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u8(self.op());
        match self {
            All(_) | None(_) => (),
            Atom(_, s) => s.hash(state),
            And(_, ps) | Or(_, ps) => {
                state.write_usize(ps.len());
                for p in ps {
                    state.write_usize(p.unique_id());
                }
            }
        }
    }
}

impl std::cmp::PartialEq for Model {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (All(_), All(_)) | (None(_), None(_)) => true,
            (Atom(_, a), Atom(_, b)) => a == b,
            (And(_, va), And(_, vb)) | (Or(_, va), Or(_, vb)) => {
                va.len() == vb.len()
                    && std::iter::zip(va, vb).all(|(a, b)| a.unique_id() == b.unique_id())
            }
            _ => false,
        }
    }
}
impl Eq for Model {}

impl From<String> for Model {
    fn from(s: String) -> Self {
        Atom(Cell::new(usize::MAX), s)
    }
}

impl Display for Model {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self {
            All(_) => f.write_str(""),
            None(_) => f.write_str("*no-matches*"),
            Atom(_, s) => f.write_str(s),
            And(_, subs) => {
                for (i, s) in subs.iter().enumerate() {
                    if i != 0 {
                        f.write_char(' ')?;
                    }
                    write!(f, "{s}")?;
                }
                Ok(())
            }
            Or(_, subs) => {
                f.write_char('(')?;
                for (i, s) in subs.iter().enumerate() {
                    if i != 0 {
                        f.write_char('|')?;
                    }
                    write!(f, "{s}")?;
                }
                f.write_char(')')
            }
        }
    }
}

/// Processing errors
#[derive(Debug)]
pub enum Error {
    /// Processing missed or exceeded some of the stack
    FinalizationError,
    /// Processing reached HIR nodes limit
    EarlyStop,
    /// Literal was not a valid string
    DecodeError(Utf8Error),
    /// Non-decodable character class
    ClassError(hir::ClassBytes),
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<Utf8Error> for Error {
    fn from(value: Utf8Error) -> Self {
        Error::DecodeError(value)
    }
}

impl Model {
    pub fn new(r: &Hir) -> Result<Self, Error> {
        visit(r, InfoVisitor::default())
    }

    pub fn unique_id(&self) -> usize {
        match self {
            All(id) | None(id) | Atom(id, _) | And(id, _) | Or(id, _) => id.get(),
        }
    }
    pub fn set_unique_id(&self, value: usize) {
        match self {
            All(id) | None(id) | Atom(id, _) | And(id, _) | Or(id, _) => id.set(value),
        }
    }

    pub fn all() -> Self {
        All(Cell::new(usize::MAX))
    }

    pub fn none() -> Self {
        None(Cell::new(usize::MAX))
    }

    fn or_strings(strings: SSet) -> Self {
        Model::Or(
            Cell::new(usize::MAX),
            simplify_string_set(strings).map(From::from).collect(),
        )
    }

    fn op(&self) -> u8 {
        match self {
            All(_) => 0,
            None(_) => 1,
            Atom(_, _) => 2,
            And(_, _) => 3,
            Or(_, _) => 4,
        }
    }

    /// Simplifies And and Or nodes
    fn simplify(self) -> Self {
        match self {
            And(uid, v) if v.is_empty() => All(uid),
            Or(uid, v) if v.is_empty() => None(uid),
            And(_, mut v) | Or(_, mut v) if v.len() == 1 => {
                v.pop().expect("we checked the length").simplify()
            }
            s => s,
        }
    }

    // re2 merges those into separate functions but it only saves on
    // the header and increases the branching complexity of the rest
    // so y?
    fn and(self, mut b: Self) -> Self {
        let mut a = self.simplify();
        b = b.simplify();

        // Canonicalize: a->op <= b->op.
        if a.op() > b.op() {
            std::mem::swap(&mut a, &mut b);
        }

        // ALL and NONE are smallest opcodes.
        a = match a {
            // ALL and b = b
            All(..) => return b,
            // NONE and b = None
            None(uid) => return None(uid),
            a => a,
        };

        match (a, b) {
            // If a and b match op, merge their contents.
            (And(unique_id, mut va), And(_, vb)) => {
                va.extend(vb);
                And(unique_id, va)
            }
            // If a or b matches the operation, merge the other one in
            (And(unique_id, mut v), vv) | (vv, And(unique_id, mut v)) => {
                v.push(vv);
                And(unique_id, v)
            }
            (a, b) => And(Cell::new(usize::MAX), vec![a, b]),
        }
    }

    fn or(self, mut b: Self) -> Self {
        let mut a = self.simplify();
        b = b.simplify();

        // Canonicalize: a->op <= b->op.
        if a.op() > b.op() {
            std::mem::swap(&mut a, &mut b);
        }

        a = match a {
            // NONE or b = b
            None(..) => return b,
            // ALL or b = ALL
            All(uid) => return All(uid),
            a => a,
        };

        match (a, b) {
            // If a and b match op, merge their contents.
            (Or(unique_id, mut va), Or(_, vb)) => {
                va.extend(vb);
                Or(unique_id, va)
            }
            // If a or b matches the operation, merge the other one in
            (Or(unique_id, mut v), vv) | (vv, Or(unique_id, mut v)) => {
                v.push(vv);
                Or(unique_id, v)
            }
            (a, b) => Or(Cell::new(usize::MAX), vec![a, b]),
        }
    }
}

// Necessary for simplify_string_set to work: the simplification
// consists of removing every "superset" of an other string of the
// set, that is any strings which contains an other (non-empty) string
// of the set, because the smaller atom will already indicate that the
// pattern is a candidate, so also matching the larger atom is useless
//
// In order to make the implementation simpler and more efficient,
// visit the smaller strings first that way we only need to visit the
// following siblings (larger strings which *might* contain the
// current one).
#[derive(PartialEq, Eq, Debug, Clone)]
struct LengthThenLex(pub String);
impl Deref for LengthThenLex {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Ord for LengthThenLex {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.cmp(&other.0))
    }
}
impl PartialOrd for LengthThenLex {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
type SSet = BTreeSet<LengthThenLex>;
fn simplify_string_set(strings: SSet) -> impl Iterator<Item = String> {
    let mut to_keep = vec![true; strings.len()];
    let mut e = strings.iter().enumerate();
    while let Some((i, s)) = e.next() {
        if s.is_empty() || !to_keep[i] {
            continue;
        }

        for (keep, (_, s2)) in to_keep[i..].iter_mut().skip(1).zip(e.clone()) {
            if *keep && s2.len() > s.len() && s2.0.contains(&s.0) {
                *keep = false;
            }
        }
    }

    std::iter::zip(to_keep, strings)
        .filter(|v| v.0)
        .map(|v| v.1 .0)
}

/// Intermediate information about the set of strings a regex matches,
/// used for the computation of a prefilter.
#[derive(Debug)]
enum Info {
    Match(Model),
    Exact(SSet),
}
impl Info {
    fn take_match(self) -> Model {
        match self {
            Self::Match(p) => p,
            Self::Exact(s) => Model::or_strings(s),
        }
    }

    fn into_exact(self) -> Option<SSet> {
        match self {
            Self::Exact(s) => Some(s),
            Self::Match(_) => Option::None,
        }
    }
}

struct InfoVisitor {
    stack: Vec<Info>,
    max_visits: usize,
}
impl Default for InfoVisitor {
    fn default() -> Self {
        Self {
            max_visits: 100_000,
            stack: Vec::new(),
        }
    }
}

// [`regex_syntax::hir::Visitor`] works pretty differently than
// `re2::Regexp::Walker` as it does not return / merge anything, so we
// need to merge down into the stack on post.
impl Visitor for InfoVisitor {
    type Output = Model;
    type Err = Error;

    fn finish(mut self) -> Result<Self::Output, Self::Err> {
        (self.stack.len() == 1)
            .then_some(&mut self.stack)
            .and_then(|s| s.pop())
            .map(Info::take_match)
            .ok_or(Error::FinalizationError)
    }

    fn visit_pre(&mut self, _hir: &Hir) -> Result<(), Self::Err> {
        // re2 sets `stopped_early` and calls `ShortVisit` but keeps
        // on keeping on, not clear why & ultimately BuildInfo only
        // cares about having stopped early
        self.max_visits = self.max_visits.checked_sub(1).ok_or(Error::EarlyStop)?;

        Ok(())
    }

    fn visit_post(&mut self, hir: &Hir) -> Result<(), Self::Err> {
        match hir.kind() {
            HirKind::Empty | HirKind::Look(_) => {
                self.stack
                    .push(Info::Exact([LengthThenLex(String::new())].into()));
            }
            HirKind::Literal(hir::Literal(data)) => {
                if data.is_empty() {
                    // NoMatch
                    self.stack.push(Info::Match(Model::none()));
                } else {
                    // re2 does this weird as it performs a cross
                    // product of individual characters, but as far as
                    // I understand that's just a complicated way to
                    // build a singleton set of the payload?
                    self.stack.push(Info::Exact(
                        [LengthThenLex(std::str::from_utf8(data)?.to_lowercase())].into(),
                    ));
                }
            }
            HirKind::Class(cls) => {
                let uc;
                let c = match cls {
                    hir::Class::Unicode(c) => c,
                    hir::Class::Bytes(b) => {
                        uc = b
                            .to_unicode_class()
                            .ok_or_else(|| Error::ClassError(b.clone()))?;
                        &uc
                    }
                };
                self.stack
                    .push(if c.iter().map(|r| r.len()).sum::<usize>() > 10 {
                        Info::Match(Model::all())
                    } else {
                        Info::Exact(
                            c.iter()
                                .flat_map(|r| (r.start()..=r.end()))
                                .map(char::to_lowercase)
                                .map(String::from_iter)
                                .map(LengthThenLex)
                                .collect(),
                        )
                    });
            }
            // Apparently re2 and regex have inverse choices, re2
            // normalises repetitions to */+/?, regex normalises
            // everything to {a, b}, so this may or may make any sense
            HirKind::Repetition(hir::Repetition { min, .. }) => {
                if *min == 0 {
                    // corresponds to */? (star/quest)
                    self.stack.pop();
                    self.stack.push(Info::Match(Model::all()));
                } else {
                    // corresponds to +
                    let arg = self
                        .stack
                        .pop()
                        .expect("a repetition to be associated with a pattern to repeat")
                        .take_match();
                    self.stack.push(Info::Match(arg));
                }
            }
            // should just leave its child on the stack for whoever
            // lives up
            HirKind::Capture(_) => (),
            HirKind::Alternation(alt) => {
                // needs to pop alt.len() items from the stack, and if
                // they're ``exact`` then just merge them, otherwise
                // ``Prefilter::Or`` them

                // sort the topn to have the exacts at the top, largest top
                let topn = self.stack.len() - alt.len()..;
                let infos = &mut self.stack[topn.clone()];

                let matches =
                    topn.start + infos.iter().filter(|v| matches!(v, Info::Match(_))).count();
                // I think we can do that because we don't actually
                // regex match so order should not matter question
                // mark
                infos.sort_unstable_by_key(|v| match v {
                    Info::Match(_) => (false, 0),
                    Info::Exact(s) => (true, s.len()),
                });
                // there are exact matches, merge them
                let exacts = self
                    .stack
                    .drain(matches..)
                    .rev()
                    .fold(BTreeSet::new(), |mut s, i| {
                        s.append(
                            &mut i
                                .into_exact()
                                .expect("the top `matches` records should be exacts"),
                        );
                        s
                    });
                let mut matches = self
                    .stack
                    .drain(topn)
                    .map(Info::take_match)
                    .collect::<Vec<_>>();
                self.stack.push(if matches.is_empty() {
                    Info::Exact(exacts)
                } else {
                    if !exacts.is_empty() {
                        matches.push(Model::or_strings(exacts));
                    }
                    Info::Match(
                        matches
                            .into_iter()
                            .map(From::from)
                            .fold(Model::none(), Model::or),
                    )
                });
            }
            // and this one gets really painful, like above we need to
            // take the topn but unlike the above we can't reorder all
            // our stuff around
            HirKind::Concat(c) => {
                let topn = self.stack.len() - c.len()..;

                // ALL is the identity element of AND
                let mut result = Info::Match(Model::all());
                let mut exacts = BTreeSet::new();
                for info in self.stack.drain(topn) {
                    match info {
                        Info::Exact(set) if exacts.is_empty() => {
                            exacts = set;
                        }
                        Info::Exact(set) if set.len() * exacts.len() <= 16 => {
                            // Not useful to consume the existing
                            // `exacts` up-front, as each item has to
                            // be splatted over `set`.
                            exacts = iproduct!(&exacts, &set)
                                .map(|(s, ss)| {
                                    let mut r = String::with_capacity(s.len() + ss.len());
                                    r.push_str(s);
                                    r.push_str(ss);
                                    LengthThenLex(r)
                                })
                                .collect();
                        }
                        i => {
                            // here AND the combination of info,
                            // exact, and the existing garbage
                            let mut p = result.take_match();
                            if !exacts.is_empty() {
                                p = Model::and(p, Model::or_strings(std::mem::take(&mut exacts)));
                            }
                            p = Model::and(p, i.take_match());
                            result = Info::Match(p);
                        }
                    }
                }

                if exacts.is_empty() {
                    self.stack.push(result);
                } else {
                    self.stack.push(Info::Match(Model::and(
                        result.take_match(),
                        Model::or_strings(exacts),
                    )));
                }
            }
        }
        Ok(())
    }
}
