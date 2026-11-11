# Fixtures

Small C programs, built by clang and lld for x86-64 and x86 without a C runtime, and the binaries
they produce. The tests of every crate read them, so the binaries are committed and the tests need
no compiler. `build.ps1` rebuilds them; `/Brepro` keeps a rebuild byte-identical for the same LLVM.

| Source | Binaries | What it holds |
|---|---|---|
| `src/hello.c` | `hello64.exe`, `hello32.exe` | Three imports, two leaf functions, one caller |
| `src/control.c` | `control64.exe`, `control32.exe` | Loops, if-else, a dense switch, recursion, a table of function pointers, globals, a no-return call mid-function; a CodeView record |
| `src/exports.c`, `src/exports.def` | `exports64.dll`, `exports32.dll` | Named exports, an export by ordinal only, a forwarder |

Beside each binary is the linker map (`.map`): the analysis tests compare the functions they find
with it. `expected/<name>.txt` is what the program prints when it runs natively; the execution
oracle compares the lifted program with it.
