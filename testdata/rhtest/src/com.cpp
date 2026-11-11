// com.cpp: COM both ways. Objects of the program's own, __stdcall methods behind several interfaces
// (QueryInterface through adjustor thunks, AddRef/Release counts), handed to code that calls them
// by their vtables; and the system's objects called from the program: an IStream on an HGLOBAL,
// the task allocator's IMalloc, BSTRs and VARIANTs (oleaut32), a SAFEARRAY (pointer fields), GUIDs
// to and from strings, CoCreateInstance of the shell's link object and its IPersist.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <objbase.h>
#include <oleauto.h>
#include <shlobj.h>
#include <string.h>
#include "rhtest.h"

#pragma comment(lib, "ole32.lib")
#pragma comment(lib, "oleaut32.lib")
#pragma comment(lib, "uuid.lib")

static u32 fnvw(const wchar_t *s)
{
    u32 h = 2166136261u;
    while (*s) h = (h ^ (u16)*s++) * 16777619u;
    return h;
}

// ---- the program's own objects -----------------------------------------------------------------

// {5D7A3F10-1C2B-4E5D-9A8B-0C1D2E3F4A5B}, {5D7A3F11-...}
static const GUID IID_ICounter = {0x5d7a3f10, 0x1c2b, 0x4e5d, {0x9a, 0x8b, 0x0c, 0x1d, 0x2e, 0x3f, 0x4a, 0x5b}};
static const GUID IID_INamer = {0x5d7a3f11, 0x1c2b, 0x4e5d, {0x9a, 0x8b, 0x0c, 0x1d, 0x2e, 0x3f, 0x4a, 0x5b}};

struct ICounter : public IUnknown {
    virtual HRESULT STDMETHODCALLTYPE Add(LONG by, LONG *now) = 0;
    virtual LONG STDMETHODCALLTYPE Get() = 0;
};

struct INamer : public IUnknown {
    virtual HRESULT STDMETHODCALLTYPE Name(wchar_t *out, UINT size) = 0;
    virtual double STDMETHODCALLTYPE Ratio(double a, float b) = 0;
};

class Thing : public ICounter, public INamer {
public:
    Thing() : refs(1), count(0) {}
    // (IUnknown's three, reached through either interface: the INamer ones by adjustor thunks)
    NOINLINE HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **out) override {
        if (IsEqualGUID(iid, IID_IUnknown) || IsEqualGUID(iid, IID_ICounter)) *out = static_cast<ICounter *>(this);
        else if (IsEqualGUID(iid, IID_INamer)) *out = static_cast<INamer *>(this);
        else { *out = NULL; return E_NOINTERFACE; }
        AddRef();
        return S_OK;
    }
    NOINLINE ULONG STDMETHODCALLTYPE AddRef() override { return (ULONG)InterlockedIncrement(&refs); }
    NOINLINE ULONG STDMETHODCALLTYPE Release() override {
        LONG r = InterlockedDecrement(&refs);
        if (r == 0) delete this;
        return (ULONG)r;
    }
    NOINLINE HRESULT STDMETHODCALLTYPE Add(LONG by, LONG *now) override {
        count += by;
        if (now) *now = count;
        return by < 0 ? S_FALSE : S_OK;
    }
    NOINLINE LONG STDMETHODCALLTYPE Get() override { return count; }
    NOINLINE HRESULT STDMETHODCALLTYPE Name(wchar_t *out, UINT size) override {
        static const wchar_t n[] = L"thing";
        if (size < 6) return E_INVALIDARG;
        memcpy(out, n, sizeof n);
        return S_OK;
    }
    NOINLINE double STDMETHODCALLTYPE Ratio(double a, float b) override { return a / b + count; }
    virtual ~Thing() {}
    LONG refs;
    LONG count;
};

// (a caller that knows only IUnknown*)
NOINLINE static u32 use_object(IUnknown *u)
{
    ICounter *c = NULL;
    INamer *n = NULL;
    u32 acc = 0;
    LONG now = 0;
    wchar_t name[16];
    void *none = (void *)1;
    if (SUCCEEDED(u->QueryInterface(IID_ICounter, (void **)&c))) {
        acc += c->Add(5 + (LONG)seed(), &now) == S_OK;
        acc = acc * 10 + (c->Add(-2, &now) == S_FALSE);
        acc = acc * 100 + (u32)now;
    }
    if (SUCCEEDED(c->QueryInterface(IID_INamer, (void **)&n))) {
        n->Name(name, 16);
        acc = acc * 31 + fnvw(name);
        acc = acc * 31 + (u32)(int)(n->Ratio(9.0, 2.0f) * 4);
        // (and back to the first interface from the second, through its thunk)
        ICounter *c2 = NULL;
        n->QueryInterface(IID_ICounter, (void **)&c2);
        acc = acc * 31 + (c2 == c) + (u32)c2->Get();
        c2->Release();
        n->Release();
    }
    acc = acc * 31 + (u->QueryInterface(IID_IStream, &none) == E_NOINTERFACE) + (none == NULL);
    c->Release();
    return acc;
}

NOINLINE static void own_objects()
{
    Thing *t = new Thing();
    IUnknown *u = static_cast<ICounter *>(t);
    check("own object", use_object(u));
    check("own refs", (u32)t->AddRef() - 1);
    t->Release();
    check("own released", u->Release());
}

// ---- the system's objects ----------------------------------------------------------------------

NOINLINE static void hglobal_stream()
{
    IStream *s = NULL;
    u8 data[48], back[48];
    ULONG n = 0;
    LARGE_INTEGER zero;
    ULARGE_INTEGER pos;
    STATSTG st;
    HGLOBAL g = NULL;
    u32 i;
    for (i = 0; i < sizeof data; i++) data[i] = (u8)(i * 5 + seed());
    check("stream made", CreateStreamOnHGlobal(NULL, TRUE, &s) == S_OK);
    check("stream write", s->Write(data, sizeof data, &n) == S_OK && n == sizeof data);
    zero.QuadPart = 8;
    check("stream seek", s->Seek(zero, STREAM_SEEK_SET, &pos) == S_OK);
    check("stream pos", (u32)pos.QuadPart);
    memset(back, 0, sizeof back);
    check("stream read", s->Read(back, 32, &n) == S_OK);
    check("stream read n", n);
    check("stream read ok", memcmp(back, data + 8, 32) == 0);
    memset(&st, 0, sizeof st);
    check("stream stat", s->Stat(&st, STATFLAG_NONAME) == S_OK);
    check("stream stat size", (u32)st.cbSize.QuadPart);
    check("stream stat type", st.type);
    check("stream hglobal", GetHGlobalFromStream(s, &g) == S_OK && g != NULL);
    check("stream global size", (u32)GlobalSize(g) >= sizeof data);
    check("stream released", s->Release());
}

NOINLINE static void task_allocator()
{
    IMalloc *m = NULL;
    void *p;
    check("malloc got", CoGetMalloc(1, &m) == S_OK);
    p = m->Alloc(100 + seed());
    check("malloc size", (u32)m->GetSize(p));
    p = m->Realloc(p, 300);
    check("malloc did", m->DidAlloc(p));
    check("malloc resize", (u32)m->GetSize(p));
    m->Free(p);
    p = CoTaskMemAlloc(64);
    check("task mem", p != NULL);
    CoTaskMemFree(p);
    m->Release();
}

NOINLINE static void automation()
{
    BSTR b = SysAllocString(L"automation");
    VARIANT v, w;
    SAFEARRAY *sa;
    SAFEARRAYBOUND bound;
    LONG i, lo = 0, hi = 0, sum = 0;
    check("bstr len", SysStringLen(b));
    check("bstr bytes", SysStringByteLen(b));
    check("bstr hash", fnvw(b));
    VariantInit(&v);
    VariantInit(&w);
    v.vt = VT_I4;
    v.lVal = 1234 + (LONG)seed();
    check("variant to string", VariantChangeType(&w, &v, 0, VT_BSTR) == S_OK);
    check("variant string", fnvw(w.bstrVal));
    check("variant back", VariantChangeType(&v, &w, 0, VT_R8) == S_OK);
    check("variant double", (u32)(int)(v.dblVal * 2));
    VariantClear(&w);
    VariantClear(&v);
    bound.lLbound = 3;
    bound.cElements = 5;
    sa = SafeArrayCreate(VT_I4, 1, &bound);
    check("safearray made", sa != NULL);
    for (i = 3; i < 8; i++) {
        LONG val = i * i + (LONG)seed();
        SafeArrayPutElement(sa, &i, &val);
    }
    SafeArrayGetLBound(sa, 1, &lo);
    SafeArrayGetUBound(sa, 1, &hi);
    check("safearray bounds", (u32)(lo * 100 + hi));
    check("safearray dims", SafeArrayGetDim(sa));
    check("safearray elem size", SafeArrayGetElemsize(sa));
    {
        LONG *data = NULL;
        SafeArrayAccessData(sa, (void **)&data);
        for (i = 0; i < 5; i++) sum += data[i];
        SafeArrayUnaccessData(sa);
    }
    check("safearray sum", (u32)sum);
    SafeArrayDestroy(sa);
    SysFreeString(b);
}

NOINLINE static void guids()
{
    wchar_t text[40];
    GUID back;
    check("guid text", (u32)StringFromGUID2(IID_INamer, text, 40));
    check("guid hash", fnvw(text));
    check("guid parse", CLSIDFromString(text, &back) == S_OK);
    check("guid same", IsEqualGUID(back, IID_INamer));
}

NOINLINE static void shell_link()
{
    IShellLinkA *link = NULL;
    IPersist *persist = NULL;
    CLSID clsid;
    char desc[64];
    HRESULT hr = CoCreateInstance(CLSID_ShellLink, NULL, CLSCTX_INPROC_SERVER, IID_IShellLinkA, (void **)&link);
    check("link made", hr == S_OK);
    if (FAILED(hr)) return;
    check("link describe", link->SetDescription("rhtest link") == S_OK);
    memset(desc, 0, sizeof desc);
    check("link description", link->GetDescription(desc, sizeof desc) == S_OK);
    check("link description len", (u32)strlen(desc));
    check("link persist", link->QueryInterface(IID_IPersist, (void **)&persist) == S_OK);
    check("link class", persist->GetClassID(&clsid) == S_OK && IsEqualGUID(clsid, CLSID_ShellLink));
    persist->Release();
    check("link released", link->Release());
}

extern "C" void rhtest_run(void)
{
    check("co init", SUCCEEDED(CoInitializeEx(NULL, COINIT_APARTMENTTHREADED)));
    own_objects();
    hglobal_stream();
    task_allocator();
    automation();
    guids();
    shell_link();
    CoUninitialize();
}
