use criterion::{criterion_group, criterion_main, Criterion};

use regex::Regex;

/// On this trivial syntetic test, the results on an M1P are:
///
/// * 18ns for a match failure
/// * 33ns for a match success
/// * 44ns for a capture failure
/// * 111ns for a capture success
///
/// Cutoff is at n=1.27 failures average. So really depends how
/// selective the prefilter is...
fn bench_regex(c: &mut Criterion) {
    let r = Regex::new(r"(foo|bar)baz/(\d+)\.(\d+)").unwrap();

    c.bench_function("has match - success", |b| {
        b.iter(|| r.is_match("foobaz/1.2"))
    });
    c.bench_function("has match - failure", |b| {
        b.iter(|| r.is_match("fooxbaz/1.2"))
    });

    c.bench_function("match - success", |b| b.iter(|| r.find("foobaz/1.2")));
    c.bench_function("match - failure", |b| b.iter(|| r.find("fooxbaz/1.2")));

    c.bench_function("capture - success", |b| b.iter(|| r.captures("foobaz/1.2")));
    c.bench_function("capture - failure", |b| {
        b.iter(|| r.captures("fooxbaz/1.2"))
    });
}

criterion_group!(benches, bench_regex);
criterion_main!(benches);
