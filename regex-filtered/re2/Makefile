CXXFLAGS += -std=c++20 -Wall -Werror -g -fPIC -O3
LDFLAGS += -lre2

.PHONY: clean

bench: bench.cpp
	$(CXX) $(CXXFLAGS) $(LDFLAGS) $^ -o $@

clean:
	@rm bench
