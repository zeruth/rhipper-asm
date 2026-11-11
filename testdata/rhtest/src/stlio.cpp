// stlio.cpp: stl.cpp's string streams alone, built /MT as it is: the static C++ library's
// formatting and parsing (num_put, num_get, the locale's facets, operator<< of a double), apart
// from the rest so a fix to them is tried on a program a fraction of stl's size.
#include <sstream>
#include <string>
#include "rhtest.h"

static u32 fnv(const std::string &s)
{
    u32 h = 2166136261u;
    for (size_t i = 0; i < s.size(); i++) h = (h ^ (u8)s[i]) * 16777619u;
    return h;
}

NOINLINE static void ostreams()
{
    std::ostringstream o;
    o << "n=" << 42 + (int)seed() << " h=" << std::hex << 0xbeefu << std::dec << " f=" << 2.5 << " s=" << std::string("x");
    o.width(6);
    o.fill('*');
    o << 17;
    check("ostream", fnv(o.str()));
}

NOINLINE static void istreams()
{
    std::istringstream in("12 34 abc 5.25");
    int a = 0, b = 0;
    std::string word;
    double d = 0;
    in >> a >> b >> word >> d;
    check("istream", (u32)(a * 100 + b) + fnv(word) + (u32)(int)(d * 4));
}

extern "C" void rhtest_run(void)
{
    ostreams();
    istreams();
}
