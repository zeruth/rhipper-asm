// stl.cpp: the MSVC C++ library a game links in: strings (the small-string buffer and the heap),
// vectors growing and erasing, ordered and hashed maps, sort with a functor and a lambda, function
// objects, shared_ptr's interlocked count and weak_ptr, unique_ptr with a deleter, RTTI
// (dynamic_cast across a hierarchy, typeid's names), string streams' formatting, list splicing,
// algorithms over iterators. Written for both toolsets (VS2010: no range-for, no initializer lists).
#include <algorithm>
#include <functional>
#include <list>
#include <map>
#include <memory>
#include <numeric>
#include <sstream>
#include <string>
#include <typeinfo>
#include <unordered_map>
#include <vector>
#include <string.h>
#include "rhtest.h"

static u32 fnv(const char *s, size_t n)
{
    u32 h = 2166136261u;
    for (size_t i = 0; i < n; i++) h = (h ^ (u8)s[i]) * 16777619u;
    return h;
}

static u32 fnv(const std::string &s) { return fnv(s.data(), s.size()); }

NOINLINE static void strings()
{
    std::string a("short");                        // (in the object's own buffer)
    std::string b(40 + seed(), 'x');               // (on the heap)
    a += "-";
    a += std::string(3, 'y');
    a.insert(0, "pre:");
    b.replace(5, 10, "middle");
    b.erase(b.size() - 4);
    check("str a", fnv(a));
    check("str a len", (u32)a.size());
    check("str b", fnv(b));
    check("str b len", (u32)b.size());
    check("str find", (u32)b.find("middle"));
    check("str rfind", (u32)a.rfind('y'));
    check("str substr", fnv(a.substr(4, 5)));
    check("str compare", (u32)(a.compare("pre:short-yyy") == 0) + (u32)(a < b) * 2);
    std::string c = a;
    c.swap(b);
    check("str swapped", fnv(c) == fnv(std::string(c)) ? (u32)c.size() : 0u);
}

NOINLINE static void vectors()
{
    std::vector<u32> v;
    for (u32 i = 0; i < 100 + seed(); i++) v.push_back(i * 2654435761u);
    v.erase(v.begin() + 10, v.begin() + 20);
    v.insert(v.begin() + 3, 5, 77u);
    std::vector<u32> w(v.rbegin(), v.rend());
    check("vec size", (u32)v.size());
    check("vec sum", std::accumulate(v.begin(), v.end(), 0u));
    check("vec back", w.back());
    v.resize(30);
    v.shrink_to_fit();
    check("vec resized", (u32)v.size() + (u32)(v.capacity() >= 30) * 1000);
    std::vector<std::string> names;
    names.push_back("delta");
    names.push_back("alpha");
    names.push_back("charlie");
    names.push_back("bravo");
    std::sort(names.begin(), names.end());
    std::string joined;
    for (size_t i = 0; i < names.size(); i++) joined += names[i] + ",";
    check("vec strings", fnv(joined));
}

struct ByMod {
    u32 m;
    explicit ByMod(u32 mod) : m(mod) {}
    bool operator()(u32 a, u32 b) const { return a % m != b % m ? a % m < b % m : a < b; }
};

NOINLINE static void sorting()
{
    std::vector<u32> v;
    u32 x = 12345 + seed();
    for (int i = 0; i < 200; i++) {
        x = x * 1103515245u + 12345u;
        v.push_back(x >> 8);
    }
    std::vector<u32> a = v, b = v;
    std::sort(a.begin(), a.end(), ByMod(7));
    std::stable_sort(b.begin(), b.end(), [](u32 p, u32 q) { return (p & 0xff) < (q & 0xff); });
    u32 ha = 0, hb = 0;
    for (size_t i = 0; i < a.size(); i++) ha = ha * 31 + a[i];
    for (size_t i = 0; i < b.size(); i++) hb = hb * 31 + b[i];
    check("sort functor", ha);
    check("sort lambda stable", hb);
    std::vector<u32>::iterator it = std::lower_bound(a.begin(), a.end(), a[57], ByMod(7));
    check("lower bound", (u32)(it - a.begin()));
    std::nth_element(v.begin(), v.begin() + 100, v.end());
    check("nth", v[100]);
    check("minmax", *std::max_element(v.begin(), v.end()) - *std::min_element(v.begin(), v.end()));
}

NOINLINE static void maps()
{
    std::map<std::string, int> m;
    std::unordered_map<u32, u32> h;
    const char *words[] = {"one", "two", "three", "four", "five", "six", "seven"};
    for (int i = 0; i < 7; i++) m[words[i]] = i + (int)seed();
    m.erase("four");
    std::string order;
    for (std::map<std::string, int>::iterator it = m.begin(); it != m.end(); ++it)
        order += it->first.substr(0, 2);
    check("map order", fnv(order));
    check("map find", (u32)m.find("six")->second);
    check("map count", (u32)m.count("four") + (u32)m.size() * 10);
    for (u32 i = 0; i < 500; i++) h[i * 7919u] = i;
    u32 sum = 0;
    for (u32 i = 0; i < 500; i += 3) sum += h[i * 7919u];
    check("hash sum", sum);
    check("hash size", (u32)h.size());
    h.erase(7919u * 3);
    check("hash erased", (u32)h.count(7919u * 3) + (u32)h.size());
}

class Shape {
public:
    virtual ~Shape() {}
    virtual u32 area() const = 0;
};
class Rect : public Shape {
public:
    Rect(u32 w, u32 h) : w_(w), h_(h) {}
    u32 area() const override { return w_ * h_; }
private:
    u32 w_, h_;
};
class Square : public Rect {
public:
    explicit Square(u32 s) : Rect(s, s) {}
};
class Circle : public Shape {
public:
    explicit Circle(u32 r) : r_(r) {}
    u32 area() const override { return 3 * r_ * r_; }
private:
    u32 r_;
};

static int g_deleted;
struct Deleter {
    void operator()(Shape *s) const { g_deleted += (int)s->area(); delete s; }
};

NOINLINE static void pointers_and_rtti()
{
    std::shared_ptr<Shape> a(new Square(4 + seed()));
    std::shared_ptr<Shape> b = a;
    std::weak_ptr<Shape> w = a;
    check("shared count", (u32)a.use_count());
    b.reset();
    check("shared after reset", (u32)a.use_count());
    std::shared_ptr<Shape> locked = w.lock();
    check("weak locked", locked ? locked->area() : 0u);
    locked.reset();
    a.reset();
    check("weak expired", (u32)w.expired());
    {
        std::unique_ptr<Shape, Deleter> u(new Circle(3));
        std::unique_ptr<Shape, Deleter> v(std::move(u));
        check("unique moved", u.get() == NULL);
    }
    check("deleter ran", (u32)g_deleted);
    std::vector<Shape *> shapes;
    shapes.push_back(new Rect(2, 3));
    shapes.push_back(new Square(5));
    shapes.push_back(new Circle(2));
    u32 rects = 0, squares = 0;
    std::string names;
    for (size_t i = 0; i < shapes.size(); i++) {
        rects += dynamic_cast<Rect *>(shapes[i]) != NULL;
        squares += dynamic_cast<Square *>(shapes[i]) != NULL;
        names += typeid(*shapes[i]).name();
        delete shapes[i];
    }
    check("rtti rects", rects * 10 + squares);
    check("rtti names", fnv(names));
}

NOINLINE static void functions()
{
    u32 base = 10 + seed();
    std::function<u32(u32)> add = [base](u32 x) { return x + base; };
    std::function<u32(u32)> twice = [add](u32 x) { return add(add(x)); };
    std::function<u32(u32, u32)> mul = std::multiplies<u32>();
    std::vector<std::function<u32(u32)> > chain;
    chain.push_back(add);
    chain.push_back(twice);
    chain.push_back(std::bind(mul, std::placeholders::_1, 3u));
    u32 v = 1;
    for (size_t i = 0; i < chain.size(); i++) v = chain[i](v);
    check("functions", v);
    std::list<u32> l1, l2;
    for (u32 i = 0; i < 5; i++) { l1.push_back(i); l2.push_front(i * 10); }
    std::list<u32>::iterator at = l1.begin();
    std::advance(at, 2);
    l1.splice(at, l2);
    l1.remove_if([](u32 x) { return x % 20 == 0 && x != 0; });
    u32 h = 0;
    for (std::list<u32>::iterator it = l1.begin(); it != l1.end(); ++it) h = h * 37 + *it;
    check("list splice", h + (u32)l1.size() * 1000);
}

NOINLINE static void streams()
{
    std::ostringstream o;
    o << "n=" << 42 + (int)seed() << " h=" << std::hex << 0xbeefu << std::dec << " f=" << 2.5 << " s=" << std::string("x");
    o.width(6);
    o.fill('*');
    o << 17;
    check("ostream", fnv(o.str()));
    std::istringstream in("12 34 abc 5.25");
    int a = 0, b = 0;
    std::string word;
    double d = 0;
    in >> a >> b >> word >> d;
    check("istream", (u32)(a * 100 + b) + fnv(word) + (u32)(int)(d * 4));
}

extern "C" void rhtest_run(void)
{
    strings();
    vectors();
    sorting();
    maps();
    pointers_and_rtti();
    functions();
    streams();
}
