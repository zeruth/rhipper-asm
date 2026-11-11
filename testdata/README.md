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
| `src/exports.c`, `src/exports.def` | `exports64.dll`, `exports32.dll` | Named exports, an export by ordinal only, a forwarder |
| `rhtest/src/core.c` | `rhtest/core-O2.exe`, `rhtest/core-Od.exe` | The old pipeline's test program: integers, branches, loops, recursion, switches, arrays, structs by value and through a hidden pointer, globals, strings, a linked list, packed and punned layouts |

Beside each binary is the linker map (`.map`): the analysis tests compare the functions they find
with it. `expected/<name>.txt` is what the program prints when it runs natively; the execution
oracle compares the lifted program with it.

`rhtest/` holds the old pipeline's test programs: `rhtest/src` is their source (each program
defines `rhtest_run()` over `rhtest.h`; `rhtest_main.c`, linked into each, prints every check and
a checksum through `WriteFile`), built with MSVC 14.51 for x86 at `/O2` and `/Od` against the C
runtime DLLs, as `<name>-O2.exe` and `<name>-Od.exe` with a link map each. `<name>.txt` is the
native output, with the exit code (the checksum's low seven bits) as its last line; the emit
oracle ports each variant as C and as Rust and compares both the output and the exit code. The
machine and promoted levels do not run them: a 32-bit program that starts through the C runtime
needs the standalone port.
