# regex-filtered: FilteredRE2 for rust-regex

This crate implements the logic behind [`FilteredRE2`] on top of
[`regex`].

The purpose is to allow efficient selection of one or more regexes
matching an input from a *large* set without having to check every
regex linearly, by prefiltering candidate regexes and only matching
those against the input.

This should be preferred to [`regex::RegexSet`] if the regexes are
non-trivial (e.g. non-literal), as [`regex::RegexSet`] constructs a
single state machine which quickly grows huge and slow.

Linear matching does not have *that* issue and works fine with complex
regexes, but doesn't scale as the number of regexes increases and
match failures quickly get very expensive (as they require traversing
the entire set every time).

## Usage

``` rust
let matcher = regex_filtered::Builder::new()
    .push("foo")?
    .push("bar")?
    .push("baz")?
    .push("quux")?
    .build()?;

assert!(matcher.is_match("bar"));
assert_eq!(matcher.matching("baz").count(), 1);
assert_eq!(matcher.matching("foo quux").count(), 2);
# Ok::<(), Box<dyn std::error::Error>>(())
```

[`Regexes::is_match`] returns whether *any* pattern in the set matches
the haystack. It is essentially equivalent to
`matcher.matching(...).next().is_some()`.

[`Regexes::matching`] returns an iterator of matching [`regex::Regex`]
and corresponding index. The index can be used to look up ancillary
data (e.g. replacement content), and the [`regex::Regex`] can be used
to [`regex::Regex::find`] or [`regex::Regex::captures`] data out of
the haystack.

## Notes

`regex-filtered` only returns the matching regexes (and their index)
as capturing especially is *significantly* more expensive than
checking for a match, this slightly pessimises situations where the
prefilter prunes perfectly but it is a large gain as soon as that's
not the case and the prefilter has to be post-filtered.

## Concepts

From a large set of regexes, extract distinguishing literal tokens,
match the tokens against the input, reverse-lookup which regexes the
matching tokens correspond to, and only run the corresponding regexes
on the input.

This extraction is done by gathering literal items, converting them to
content sets, then symbolically executing concatenations and
alternations (`|`) in order to find out what literal items *need* to
be present in the haystack for this regex to match. A reverse index is
then built from literal items to regexes.

At match time, a prefilter is run checking which literals are present
in the haystack then find out what regexes that corresponds to,
following which the regexes themselves are matched against the
haystack to only return actual matching regexes.

## Divergences

While [`FilteredRE2`] requires the user to perform prefiltering,
`regex-filtered` handles this internally: [`aho-corasick`] is pretty
much ideal for that task and already a dependency of [`regex`] which
`regex-filtered` based on.

## TODO

- add a stats feature to report various build-size infos e.g.

  - number of tokens
  - number of regexes
  - number of unfiltered regexes, this would be useful to know if
    prefiltering will be done or a naive sequential application would
    be a better idea.
  - ratio of checked regexes to successes (how does it work with lazy
    iterators?)
  - total / prefiltered (- unfiltered) so atom size impact can be
    evaluated
  - also maybe mapper stats on the pruning stuff and whatever
  
[`aho-corasick`]: https://docs.rs/aho-corasick/
[`FilteredRE2`]: https://github.com/google/re2/blob/main/re2/filtered_re2.h
[`regex`]: https://docs.rs/regex/
[`regex-syntax`]: https://docs.rs/regex-syntax/
