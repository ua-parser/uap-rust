CXXFLAGS += -std=c++20 -Wall -Werror -g -fPIC -O3
LDFLAGS += -lre2

.PHONY: bench atomizer clean

bench: target/bench_re2 target/devices.regexes target/release/examples/bench_regex
	/usr/bin/time -l target/bench_re2 \
		target/devices.regexes regex-filtered/samples/useragents.txt 100 -q
	/usr/bin/time -l target/release/examples/bench_regex \
		target/devices.regexes regex-filtered/samples/useragents.txt -r 100 -q

atomizer: target/devices.regexes target/atomizer target/release/examples/atomizer
	@while IFS= read -r p; do \
	    CPP=$$(target/atomizer 3 "$$p" | sort); \
	    RS=$$(target/release/examples/atomizer 3 "$$p" | sort); \
	    if [ "$$CPP" != "$$RS" ]; then \
		tmp=$$(mktemp -d); \
		mkfifo "$$tmp/cpp" "$$tmp/rs"; \
		echo "$$CPP" > "$$tmp/cpp" & \
		echo "$$RS" > "$$tmp/rs" & \
		printf '%s\n' "$$p"; \
		diff --color=always -U 50 "$$tmp/cpp" "$$tmp/rs"; \
		rm -rf "$$tmp" ; \
	    fi; \
	done < target/devices.regexes

clean:
	@rm target/devices.regexes
	@rm target/bench_re2
	@rm target/atomizer
	@rm target/release/examples/atomizer
	@rm target/release/examples/bench_regex

target/atomizer: regex-filtered/re2/atomizer.cpp
	@mkdir -p target
	$(CXX) $(CXXFLAGS) $^ -o $@ $(LDFLAGS)

target/release/examples/atomizer: regex-filtered/examples/atomizer.rs regex-filtered/src/*
	cargo build --release --example atomizer -q

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
