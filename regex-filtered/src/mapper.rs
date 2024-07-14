use std::fmt::Display;
use std::fmt::Formatter;

use super::model::Model;
use crate::int_set::IntSet;

pub struct Builder {
    min_atom_len: usize,
    models: Vec<Model>,
    unfiltered: Vec<usize>,
}
impl Builder {
    pub fn new(min_atom_len: usize) -> Self {
        Self {
            min_atom_len,
            models: Vec::new(),
            unfiltered: Vec::new(),
        }
    }

    pub fn push(&mut self, mut pf: Model) {
        if !self.keep_node(&mut pf) {
            self.unfiltered.push(self.models.len());
            // these go into unfiltered: regexes which always pass
            // through the filter
            // re2 uses nulls here but that's not us
            pf = Model::all();
        }
        self.models.push(pf);
    }
    fn keep_node(&self, pf: &mut Model) -> bool {
        match pf {
            Model::All(_) | Model::None(_) => false,
            Model::Atom(_, s) => s.len() >= self.min_atom_len,
            Model::And(_, subs) => {
                subs.retain_mut(|p| self.keep_node(p));
                !subs.is_empty()
            }
            Model::Or(_, subs) => subs.iter_mut().all(|p| self.keep_node(p)),
        }
    }

    pub fn build(self) -> (Mapper, Vec<String>) {
        // inlined `assign_unique_ids` because it doesn't seem super useful... to us
        let mut atoms = Vec::new();
        let mut atom_index_to_id = Vec::new();
        // Build vector of all filter nodes, sorted topologically,
        // from top to bottom in v add the top-level node of each
        // regexp model
        let mut v = self.models.iter().collect::<Vec<_>>();

        // now add all the descendant nodes, this has to be a `while` because we unroll the source
        let mut i = 0;
        while i < v.len() {
            let p = &v[i];
            i += 1;

            if let Model::And(_, s) | Model::Or(_, s) = &p {
                v.extend(s.iter());
            }
        }
        #[allow(clippy::mutable_key_type)]
        let mut nodes = NodeSet::with_capacity(v.len());

        let mut unique_id = 0..;
        // identify unique nodes
        for node in v.iter().rev() {
            if let Some(canonical) = nodes.get(node) {
                node.set_unique_id(canonical.unique_id());
            } else {
                let uid = unique_id.next().expect("infinite");
                node.set_unique_id(uid);
                if let Model::Atom(_, s) = &node {
                    atoms.push(s.to_string());
                    atom_index_to_id.push(uid);
                }
                nodes.insert(node);
            }
        }

        let mut entries = vec![Entry::default(); unique_id.next().expect("infinite(ish) sequence")];
        // Fill the entries
        for model in &nodes {
            match model {
                Model::None(_) => unreachable!("no idea why this is an error"),
                // We replace excluded models by All rather than null,
                // so those are not unreachable.
                Model::All(_) => (),
                Model::Atom(_, _) => {
                    let id = model.unique_id();
                    entries[id].propagate_up_at_count = 1;
                }
                // For each child, we append our id to the child's
                // list of parent ids... unless we happen to have done
                // so already. The number of appends is the number of
                // unique children, which allows correct upward
                // propagation from AND nodes.
                Model::And(_, s) | Model::Or(_, s) => {
                    let id = model.unique_id();
                    let mut up_count = 0;
                    for child_id in s.iter().map(|c| c.unique_id()) {
                        let parents = &mut entries[child_id].parents;
                        if parents.last() != Some(&id) {
                            parents.push(id);
                            up_count += 1;
                        }
                    }

                    entries[id].propagate_up_at_count = if matches!(&model, Model::And(..)) {
                        up_count
                    } else {
                        1
                    };
                }
            }
        }

        // For top level nodes, populate regexp id
        for (i, tl) in v[..self.models.len()].iter().enumerate() {
            if let Some(p) = nodes.get(tl) {
                entries[p.unique_id()].regexps.push(i);
            }
        }

        // Lastly, using probability-based heuristics, we identify nodes
        // that trigger too many parents and then we try to prune edges.
        // We use logarithms below to avoid the likelihood of underflow.
        let log_num_regexps = ((self.models.len() - self.unfiltered.len()) as f64).ln();
        // Hoisted this above the loop so that we don't thrash the heap. (???)
        let mut entries_by_num_edges = Vec::<(usize, usize)>::new();
        for model in &nodes {
            let Model::And(_, s) = &model else {
                continue;
            };

            // Sort the current node's children by the numbers of parents.
            for child_id in s.iter().map(Model::unique_id) {
                entries_by_num_edges.push((entries[child_id].parents.len(), child_id));
            }
            entries_by_num_edges.sort_unstable();

            // A running estimate of how many regexps will be
            // triggered by pruning the remaining children's edges to
            // the current node. Our nominal target is one, so the
            // threshold is log(1) == 0; pruning occurs iff the child
            // has more than nine edges left.
            let mut log_num_triggered = log_num_regexps;
            for (_, child_id) in entries_by_num_edges.drain(..) {
                let parents = &mut entries[child_id].parents;
                if log_num_triggered > 0. {
                    log_num_triggered += (parents.len() as f64).ln();
                    log_num_triggered -= log_num_regexps;
                } else if parents.len() > 9 {
                    let id = model.unique_id();
                    if let Some(idx) = parents.iter().position(|&p| p == id) {
                        parents.swap_remove(idx);
                        // re2 uses an `int`, which can go negative,
                        // we use a usize (because it's based on the
                        // number of children or sth though it's
                        // probably unnecessary) but that means we
                        // can't keep decrementing below 0
                        entries[id].propagate_up_at_count =
                            entries[id].propagate_up_at_count.saturating_sub(1);
                    }
                }
            }
        }

        (
            Mapper {
                entries,
                unfiltered: self.unfiltered,
                atom_to_entry: atom_index_to_id,
                regexp_count: self.models.len(),
            },
            atoms,
        )
    }
}

impl Display for Mapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "#Unique Atoms: {}", self.atom_to_entry.len())?;
        for (i, e) in self.atom_to_entry.iter().copied().enumerate() {
            writeln!(f, "\tatom {i} -> entry {e}")?;
            let mut s = IntSet::new(self.entries.len());
            s.insert(e);
            for r in self.propagate_match(&mut s).into_vec() {
                writeln!(f, "\t\tregex {r}")?;
            }
        }

        writeln!(f, "#Unique Entries: {}", self.entries.len())?;
        for (i, entry) in self.entries.iter().enumerate() {
            writeln!(
                f,
                "\tEntry: {i} Regexps: {} Threshold: {}",
                entry.regexps.len(),
                entry.propagate_up_at_count,
            )?;
            for parent in &entry.parents {
                writeln!(f, "\t\tParent {parent}")?;
            }
        }
        Ok(())
    }
}

type NodeSet<'a> = std::collections::HashSet<&'a Model>;

/// Each unique node has a corresponding Entry that helps in passing
/// the matching trigger information along the tree.
#[derive(Default, Clone, Debug)]
struct Entry {
    /// How many children should match before this node triggers the
    /// parent. For an atom and an OR node, this is 1 and for an AND
    /// node, it is the number of unique children.
    propagate_up_at_count: usize,

    /// When this node is ready to trigger the parent, what are the indices
    /// of the parent nodes to trigger. The reason there may be more than
    /// one is because of sharing. For example (abc | def) and (xyz | def)
    /// are two different nodes, but they share the atom 'def'. So when
    /// 'def' matches, it triggers two parents, corresponding to the two
    /// different OR nodes.
    parents: Vec<usize>,

    /// When this node is ready to trigger the parent, what are the
    /// regexps that are triggered.
    regexps: Vec<usize>,
}

pub struct Mapper {
    /// Number of regexes covered by the mapper
    regexp_count: usize,
    /// Nodes formed by build, there is one node for each unique atom
    /// and each unique and/or node
    entries: Vec<Entry>,
    /// Indices of regexp which always make it through the filter
    /// (didn't find distinguishing literals in them)
    unfiltered: Vec<usize>,
    /// Atom index to entry id mapping
    atom_to_entry: Vec<usize>,
}
impl Mapper {
    // name is shit and also needs to see if we can generate stuff on the fly
    pub fn atom_to_re(&self, atoms: impl IntoIterator<Item = usize>) -> Vec<usize> {
        let mut matched_atom_ids = IntSet::new(self.entries.len());
        matched_atom_ids.extend(atoms.into_iter().map(|idx| self.atom_to_entry[idx]));

        let mut regexps = self.propagate_match(&mut matched_atom_ids).into_vec();

        regexps.extend(&self.unfiltered);

        regexps.sort_unstable();
        regexps
    }

    fn propagate_match(&self, work: &mut IntSet) -> IntSet {
        let mut count = vec![0; self.entries.len()];

        let mut regexps = IntSet::new(self.regexp_count);

        let mut i = 0;
        while i < work.len() {
            let idx = work[i];
            i += 1;

            let entry = &self.entries[idx];
            // record regexps triggered
            regexps.extend(&entry.regexps);
            // pass trigger up to parents
            for &j in &entry.parents {
                let parent = &self.entries[j];
                // Delay until all the children have succeeded.
                if parent.propagate_up_at_count > 1 {
                    let c = &mut count[j];
                    *c += 1;
                    if *c < parent.propagate_up_at_count {
                        continue;
                    }
                }
                work.insert(j);
            }
        }

        regexps
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::model::Model;
    use regex_syntax::parse;

    #[test]
    fn empty_matcher() {
        let (m, atoms) = Builder::new(3).build();
        assert_eq!(atoms.len(), 0);
        assert_eq!(&m.unfiltered, &[]);
    }

    #[test]
    fn empty_pattern() {
        let mut b = Builder::new(3);
        b.push(Model::new(&parse("").unwrap()).unwrap());
        let (m, atoms) = b.build();
        assert_eq!(atoms.len(), 0);
        assert_eq!(&m.unfiltered, &[0]);
    }

    #[test]
    fn small_or_test() {
        let mut b = Builder::new(4);
        b.push(Model::new(&parse("(foo|bar)").unwrap()).unwrap());
        let (m, atoms) = b.build();
        assert_eq!(atoms.len(), 0);
        assert_eq!(&m.unfiltered, &[0]);
        assert_eq!(&m.atom_to_entry, &[])
    }

    #[test]
    fn reverse_index() {
        let mut b = Builder::new(3);
        b.push(Model::new(&parse("(foo|bar)").unwrap()).unwrap());
        let (m, _) = b.build();

        assert_eq!(m.entries.len(), 3);
        assert_eq!(&m.atom_to_entry, &[0, 1]);
        let mut s = IntSet::new(3);
        s.insert(0);
        assert_eq!(m.propagate_match(&mut s).into_vec(), vec![0]);
        let mut s = IntSet::new(3);
        s.insert(1);
        assert_eq!(m.propagate_match(&mut s).into_vec(), vec![0]);
    }

    fn check_patterns(patterns: &'static [&'static str], expected: &'static [&'static str]) {
        let mut b = Builder::new(3);
        for pattern in patterns {
            b.push(Model::new(&parse(pattern).unwrap()).unwrap());
        }
        let (_, mut atoms) = b.build();

        atoms.sort();
        let mut sortspected = expected.to_vec();
        sortspected.sort();
        assert_eq!(atoms, sortspected);
    }

    #[test]
    fn empty_patterns_are_allowed() {
        check_patterns(&[""], &[]);
    }

    #[test]
    fn all_atoms_greater_than_minlength_are_found_and_none_smaller() {
        check_patterns(
            &[
                "(abc123|def456|ghi789).*mnop[x-z]+",
                "abc..yyy..zz",
                "mnmnpp[a-z]+PPP",
            ],
            &[
                "abc123", "def456", "ghi789", "mnop", "abc", "yyy", "mnmnpp", "ppp",
            ],
        );
    }
    #[test]
    fn shortest_substrings_are_kept() {
        check_patterns(
            &[
                "(abc123|abc|defxyz|ghi789|abc1234|xyz).*[x-z]+",
                "abcd..yyy..yyyzzz",
                "mnmnpp[a-z]+PPP",
            ],
            &[
                "abc", "ghi789", "xyz", "abcd", "yyy", "yyyzzz", "mnmnpp", "ppp",
            ],
        );
    }

    #[test]
    fn character_class_expansion() {
        check_patterns(
            &["m[a-c][d-f]n.*[x-z]+", "[x-y]bcde[ab]"],
            &[
                "madn", "maen", "mafn", "mbdn", "mben", "mbfn", "mcdn", "mcen", "mcfn", "xbcdea",
                "xbcdeb", "ybcdea", "ybcdeb",
            ],
        );
    }
    #[test]
    fn non_ascii_casefolding() {
        check_patterns(
            &[
                // re2 apparently does some sort of strange normalisation
                // pass which regex does not and which does not seem
                // entirely kosher (might be a unicode-aware but
                // per-character upper then lower since it gets the final
                // position sigma "wrong")
                //"(?i)ΔδΠϖπΣςσ",
                "ΛΜΝΟΠ",
                "ψρστυ",
            ],
            &[
                //"δδπππσσσ",
                "λμνοπ",
                "ψρστυ",
            ],
        );
    }

    #[test]
    fn test_empty_string_in_string_set() {
        let mut b = Builder::new(0);
        b.push(Model::new(&parse("-R.+(|ADD=;AA){12}}").unwrap()).unwrap());
        let (_, mut atoms) = b.build();
        atoms.sort();

        assert_eq!(atoms, vec!["", "-r", "add=;aa", "}"],);
    }
}
