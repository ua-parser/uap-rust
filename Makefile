CXXFLAGS += -std=c++20 -Wall -Werror -g -fPIC -O3
LDFLAGS += -lre2

.PHONY: bench

bench: target/bench_re2 target/devices.regexes target/release/examples/bench_regex
	/usr/bin/time -l target/bench_re2 \
		target/devices.regexes regex-filtered/samples/useragents.txt 100 -q
	/usr/bin/time -l target/release/examples/bench_regex \
		target/devices.regexes regex-filtered/samples/useragents.txt -r 100 -q

target/bench_re2: regex-filtered/re2/bench.cpp
	# build re2 bench, requires re2 to be LD-able, can `nix develop` for setup
	@mkdir -p target
	$(CXX) $(CXXFLAGS) $^ -o $@ $(LDFLAGS)

target/release/examples/bench_regex: regex-filtered/examples/bench_regex.rs regex-filtered/src/*
	# build regex bench
	cargo build --release --example bench_regex -q

target/devices.regexes: scripts/devices ua-parser/uap-core/regexes.yaml
	# compiles regexe.yaml to a list of just the device regex (with embedded flags)
	@mkdir -p target
	uv run --script $^ > $@
