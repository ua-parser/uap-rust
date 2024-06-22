# User Agent Parser

This module implements the [browserscope / uap
standard](https://github.com/ua-parser/uap-core) for rust, allowing
the extraction of various metadata from user agents.

The browserscope standard is data-oriented, with [`regexes.yaml`]
specifying the matching and extraction from user-agent strings. This
library implements the maching protocols and provides various types to
make loading the dataset easier, however it does *not* provide the
data itself, to avoid dependencies on serialization libraries or
constrain loading.

## Dataset loading

The crate does not provide any sort of precompiled data file, or
dedicated loader, however [`Regexes`] implements
[`serde::Deserialize`] and can load a [`regexes.yaml`] file or any
format-preserving conversion thereof (e.g. loading from json or cbor
might be preferred if the application already depends on one of
those):

```no_run
# let ua_str = "";
let f = std::fs::File::open("regexes.yaml")?;
let regexes: ua_parser::Regexes = serde_yaml::from_reader(f)?;
let extractor = ua_parser::Extractor::try_from(regexes)?;

# Ok::<(), Box<dyn std::error::Error>>(())
```

All the data-description structures are also Plain Old Data, so they
can be embedded in the application directly e.g. via a build script:

``` rust
let parsers = vec![
    ua_parser::user_agent::Parser {
        regex: "foo".into(),
        family_replacement: Some("bar".into()),
        ..Default::default()
    }
];
```
## Extraction

The crate provides the ability to either extract individual
information sets (user agent — browser, OS, and device) or extract all
three in a single call.

The three infosets are are independent and non-overlapping so while
the full extractor may be convenient if only one is needed a complete
extraction is unnecessary overhead, and the extractors themselves are
somewhat costly to create and take up memory.

### Complete Extractor

For the complete extractor, it is simply converted from the
[`Regexes`] structure. The resulting [`Extractor`] embeds all three
module-level extractors as attributes, and [`Extractor::extract`]-s
into a 3-uple of `ValueRef`s.


### Individual Extractors

The individual extractors are in the [`user_agent`], [`os`], and
[`device`] modules, the three modules follow the exact same model:

- a `Parser` struct which specifies individual parser configurations,
  used as inputs to the `Builder`
- a `Builder`, into which the relevant parsers can be `push`-ed
- an `Extractor` created from the `Builder`, from which the user can
  `extract` a `ValueRef`
- the `ValueRef` result of data extraction, which may borrow from (and
  is thus lifetime-bound to) the `Parser` substitution data and the
  user agent string it was extracted from
- for convenience, an owned `Value` variant of the `ValueRef`

``` rust
use ua_parser::os::{Builder, Parser, ValueRef};

let e = Builder::new()
    .push(Parser {
        regex: r"(Android)[ \-/](\d+)(?:\.(\d+)|)(?:[.\-]([a-z0-9]+)|)".into(),
        ..Default::default()
    })?
    .push(Parser {
        regex: r"(Android) Donut".into(),
        os_v1_replacement: Some("1".into()),
        os_v2_replacement: Some("2".into()),
        ..Default::default()
    })?
    .push(Parser {
        regex: r"(Android) Eclair".into(),
        os_v1_replacement: Some("2".into()),
        os_v2_replacement: Some("1".into()),
        ..Default::default()
    })?
    .push(Parser {
        regex: r"(Android) Froyo".into(),
        os_v1_replacement: Some("2".into()),
        os_v2_replacement: Some("2".into()),
        ..Default::default()
    })?
    .push(Parser {
        regex: r"(Android) Gingerbread".into(),
        os_v1_replacement: Some("2".into()),
        os_v2_replacement: Some("3".into()),
        ..Default::default()
    })?
    .push(Parser {
        regex: r"(Android) Honeycomb".into(),
        os_v1_replacement: Some("3".into()),
       ..Default::default()
    })?
    .push(Parser {
        regex: r"(Android) (\d+);".into(),
        ..Default::default()
    })?
    .build()?;

assert_eq!(
    e.extract("Android Donut"),
    Some(ValueRef {
        os: "Android".into(),
        major: Some("1".into()),
        minor: Some("2".into()),
        ..Default::default()
    }),
);
assert_eq!(
    e.extract("Android 15"),
    Some(ValueRef { os: "Android".into(), major: Some("15".into()), ..Default::default()}),
);
assert_eq!(
    e.extract("ZuneWP7"),
    None,
);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Performances

The package has not been profiled or optimised yet, but it seems
rather competitive with uap-cpp (tested on an M1 Pro MBP):

```sh
> ./UaParserBench ../uap-core/regexes.yaml benchmarks/useragents.txt 10
   25.13s user 0.07s system 99% cpu 25.279 total
> ./UaParserBench ../uap-core/regexes.yaml ../uap-python/samples/useragents.txt 100
  246.49s user 0.47s system 99% cpu 4:07.55 total
```

```sh
> target/release/examples/bench -r 10 ../uap-core/regexes.yaml ../uap-cpp/benchmarks/useragents.txt
   10.10s user 0.04s system 99% cpu 10.169 total

> target/release/examples/bench -r 100 ../uap-core/regexes.yaml ../uap-python/samples/useragents.txt
   98.46s user 0.04s system 99% cpu 1:38.73 total
```

[`regexes.yaml`]: https://github.com/ua-parser/uap-core/blob/master/regexes.yaml
