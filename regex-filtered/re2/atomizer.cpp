#include <cassert>
#include <iostream>
#include <re2/filtered_re2.h>
#include <re2/re2.h>
#include <re2/set.h>

using namespace std::chrono;
using namespace std::literals;

int main(const int argc, const char* argv[]) {
  if (argc != 3) {
    std::cerr << "error ./atomizer atomzise regex" << std::endl;
    return 1;
  }

  re2::RE2::Options opt;
  re2::FilteredRE2 f(std::atoi(argv[1]));
  int id;

  re2::RE2::ErrorCode c;
  if ((c = f.Add(argv[2], opt, &id))) {
    std::cerr << "invalid regex " << argv[2] << std::endl;
    return 1;
  }

  std::vector<std::string> atoms;
  f.Compile(&atoms);

  //std::cout << atoms.size() << std::endl;
  for(std::string atom: atoms) {
    std::cout << atom << std::endl;
  }
  return 0;
}
