# Fixtures

Small C programs, built by clang and lld for x86-64 and x86 without a C runtime, and the binaries
they produce. The tests of every crate read them, so the binaries are committed and the tests need
no compiler. `build.ps1` rebuilds them; `/Brepro` keeps a rebuild byte-identical for the same LLVM.

| Source | Binaries | What it holds |
|---|---|---|
| `src/hello.c` | `hello64.exe`, `hello32.exe` | Three imports, two leaf functions, one caller |
| `src/control.c` | `control64.exe`, `control32.exe` | Loops, if-else, a dense switch, recursion, a table of function pointers, globals, a no-return call mid-function; a CodeView record |
| `src/float.c` | `float64.exe`, `float32.exe` | Floating-point arithmetic, conversions and compares |
| `src/classes.cpp` | `classes64.exe`, `classes32.exe` | C++ with RTTI: a base, a derived class, a second base and a class with two vtables, called through base pointers; no C++ runtime |
| `src/throws.cpp` | `throws64.exe` | C++ exceptions: a throw caught by value and one by reference to a base through a function without a handler, a rethrow into a catch-all, and a local destroyed on the way out; vcruntime's throw and frame handler, nothing else of a runtime |
| `src/exports.c`, `src/exports.def` | `exports64.dll`, `exports32.dll` | Named exports, an export by ordinal only, a forwarder |
| `src/tls.c` | `tls64.exe`, `tls32.exe` | A `__declspec(thread)` counter stepped from two threads, with the TLS directory (index, template bounds, a callback) the program defines itself for want of a C runtime |
| `src/structs.c` | `structs64.exe`, `structs32.exe` | A record of mixed fields filled and read through a pointer, for the type recovery |
| `src/window.c` | `window64.exe`, `window32.exe` | A window: a class whose procedure user32 calls back, a popup of a known client size, one paint through `BeginPaint`, and a message loop ended by `PostQuitMessage`; the log names the messages whose order is the window's own and the quit code |
| `rhtest/src/core.c` | `rhtest/core-O2.exe`, `rhtest/core-Od.exe` | The old pipeline's test program: integers, branches, loops, recursion, switches, arrays, structs by value and through a hidden pointer, globals, strings, a linked list, packed and punned layouts |
| `rhtest/src/<name>.c`, `.cpp` | `rhtest/<name>-clang.exe` | The other fifteen rhtest programs, built here by `rhtest/build.ps1` with clang-cl over the MSVC C runtime (`/O2 /MD`): `calls`, `int64`, `float`, `fpenv`, `crt` port and run; `misc`, `sys`, `win`, `seh`, `eh`, `cpp`, `com`, `stl`, `stlio`, `engine` wait for what the last paragraph below names |
| `rhtest/src/core.c`, `int64.c` | `rhtest/core-static.exe`, `rhtest/int64-static.exe` | The same by clang-cl at `/O2 /MT`: the C runtime linked into the image, no PDB, only the map, as a VS2010 `/MT` program ships. The runtime's functions are recognized by the map's libraries and left to the host, the startup is the driver's, and the x86 helpers (`_alldiv`, the stack probe, the cookie check) stay the program's |

Beside each binary is the linker map (`.map`): the analysis tests compare the functions they find
with it. `expected/<name>.txt` is what the program prints when it runs natively; the execution
oracle compares the lifted program with it.

`rhtest/` holds the old pipeline's test programs: `rhtest/src` is their source (each program
defines `rhtest_run()` over `rhtest.h`; `rhtest_main.c`, linked into each, prints every check and
a checksum through `WriteFile`), built with MSVC 14.51 for x86 at `/O2` and `/Od` against the C
runtime DLLs, as `<name>-O2.exe` and `<name>-Od.exe` with a link map each, and by clang-cl
(`rhtest/build.ps1`, `/O2 /MD` over the same runtime: the program's code is clang's, the startup
and the helpers are MSVC's) as `<name>-clang.exe`. `<name>.txt` is the native output, with the
exit code (the checksum's low seven bits) as its last line (`cargo test -p ra-asm-emit --test
oracle rhtest_write_references -- --ignored` writes a missing one); the emit oracle ports each
variant as C and as Rust and compares both the output and the exit code. The machine and promoted
levels do not run them: a 32-bit program that starts through the C runtime needs the standalone
port. The Visual Studio 2010 builds (`cl` 16.00 at `/MT`, the toolset of a VS2010 game client) go
beside the others as `<name>-vs2010.exe` with their maps, built elsewhere like the `-O2` and `-Od`
ones; the emit oracle's `rhtest_<name>_vs2010_*` tests run `core` and `int64` when the binary is
there and skip with a message when it is not.

What the programs still waiting for need, each an `ignored` test naming it: `seh` and `eh` the
exception handling of milestone J (task 49); `misc`, `sys` and `win` the Win32 structures laid out
apart on x86 and x64 (`WNDCLASS`, `MSG`, `OVERLAPPED`, `SLIST`, `WSADATA`, `CRITICAL_SECTION`, a
COM vtable) adapted by the runtime; `stl` and `stlio` the C++ runtime (`msvcp140`) under x86
mangled names the x64 DLL does not export; `cpp`, `com` and `engine` the same C++ runtime and
exceptions, plus instructions the lift does not cover yet (`com`).
