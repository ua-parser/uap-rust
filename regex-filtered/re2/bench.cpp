#include <cassert>
#include <chrono>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <re2/filtered_re2.h>
#include <re2/re2.h>
#include <re2/set.h>

using namespace std::chrono;
using namespace std::literals;

template<typename T>
std::ostream& operator<< (std::ostream& out, const std::vector<T>& v) {
    out << "[";
    bool first = true;
    for (T const &t: v) {
      if (first) {
        first = false;
      } else {
        out << ", ";
      }
      out << t;
    }
    out << "]";
    return out;
}

int main(const int argc, const char* argv[]) {
  if (argc < 4) {
    std::cerr << "error: ./bench regexes user_agents repetitions [quiet]" << std::endl;
    return 1;
  }
  bool quiet = argc == 5;

  std::ifstream regexes_f(argv[1]);

  re2::RE2::Options opt;
  re2::FilteredRE2 f(3);
  int id;

  std::string line;

  auto start = steady_clock::now();
  while(std::getline(regexes_f, line)) {
    re2::RE2::ErrorCode c;
    if((c = f.Add(line, opt, &id))) {
      std::cerr << "invalid regex " << line << std::endl;
      return 1;
    }
  }
  std::vector<std::string> to_match;
  f.Compile(&to_match);
  std::chrono::duration<float> diff = steady_clock::now() - start;
  std::cerr << f.NumRegexps() << " regexes "
            << to_match.size() << " atoms"
            << " in " << diff.count() << "s"
            << std::endl;

  opt.set_literal(true);
  opt.set_case_sensitive(false);
  start = steady_clock::now();
  re2::RE2::Set s(opt, RE2::UNANCHORED);
  for(auto const &atom: to_match) {
    // can't fail since literals
    assert(s.Add(atom, NULL) != -1);
  }
  assert(s.Compile());
  diff = steady_clock::now() - start;
  std::cerr << "\tprefilter built in " << diff.count() << "s" << std::endl;

  start = steady_clock::now();
  std::vector<std::string> user_agents;
  std::ifstream user_agents_f(argv[2]);
  while(std::getline(user_agents_f, line)) {
    user_agents.push_back(line);
  }
  diff = steady_clock::now() - start;
  std::cerr << user_agents.size()
            << " user agents in "
            << diff.count() << "s"
            << std::endl;

  int repetitions = std::stoi(argv[3]);
  std::vector<int> matching;
  for(int x = 0; x < repetitions; ++x) {
    for(size_t i = 0; i < user_agents.size(); ++i) {
      auto& ua = user_agents[i];
      matching.clear();
      int n = s.Match(ua, &matching);
      if (n) {
        n = f.FirstMatch(ua, matching);
      } else {
        n = -1;
      }
      if (!quiet) {
        if (n != -1) {
          std::cout << std::setw(3) << n;
        }
        std::cout << std::endl;
      }
    }
  }

  return 0;
}
