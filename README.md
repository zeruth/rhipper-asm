# Rhipper Asm

The disassembly and lifting core of Rhipper, in pure Rust. It reads a Windows PE image, decodes
its code, lifts each function to an LLVM-IR-shaped intermediate representation, and hands that IR
to backends that write C, C++ or Rust source that builds and runs.

It replaces Ghidra and the Python Rhipper pipeline (`github.com/zeruth/Rhipper`) in the Rhipper
Workbench (`github.com/zeruth/Rhipper-IDE`). Rhipper Agent orchestrates the work around it.

## How it works

```text
PE file ──ra-asm-pe──▶ Image ──ra-asm-x86──▶ Instr ──ra-asm-analysis──▶ Functions, CFG
                                                                             │
                                                   ra-asm-lift ◀─────────────┘
                                                        │
                                   Machine IR (every register and flag explicit)
                                                        │
                                   raising passes: locals, SSA, signatures, types
                                                        │
                                              Source-shaped IR  ──▶  .ll  (clang checks it)
                                                        │
                                       ra-asm-emit ──▶ C  │  C++  │  Rust
```

Three decisions shape the design:

- **The decoder is a dependency.** `ra-asm-x86` wraps the `iced-x86` crate behind the crate's own
  `Instr` record and an `Arch` trait. The work of this project is the lift and the backends, not
  opcode tables. A second architecture (ARM64 first) implements the same trait.
- **The IR is a subset of LLVM IR.** `ra-asm-ir` has its own types, values, blocks and printer. Every
  `.ll` the core writes must pass `clang -c -x ir`. That is the syntax and type gate, and clang is
  also how the tests run lifted code against the reference.
- **Fix a defect where it arises.** As in the old pipeline: in the loader, the lift, a pass or a
  backend. Never in one function's output. A translation must build in one round.

## Crates

| Crate | Responsibility |
|---|---|
| `ra-asm-format` | Shared binary-format readers: a little-endian cursor whose errors name the offset |
| `ra-asm-pe` | The PE image: headers, sections, the flat image by RVA, imports, exports, relocations, the exception table, TLS, debug |
| `ra-asm-arch` | The architecture-neutral `Instr` record (operands, flow, accesses) and the `Arch` trait every decoder implements |
| `ra-asm-x86` | The x86 and x86-64 decoder adapter over iced-x86; keeps iced's own instruction beside the record for the lift |
| `ra-asm-analysis` | Functions, basic blocks, the control-flow graph, jump tables, no-return propagation |
| `ra-asm-ir` | The LLVM-IR-shaped intermediate representation and its `.ll` printer |
| `ra-asm-lift` | Instructions to machine IR over an explicit `%State`, and the raising passes: signature inference, state promotion, the ABI rewrite, conditions, frames, simplification |
| `ra-asm-emit` | The structuring pass (if, else, loops, switch, short-circuit conditions), the C backend with its C++ dialect, the Rust backend, and the function table |
| `ras` (app) | The command line: `info`, `sections`, `imports`, `exports`, `relocs`, `debug`, `functions`, `cfg`, `disasm`, `callers`, `xrefs`, `lift`, `signatures`, `emit`, `pipeline` |

`ras lift` raises in three levels: `--level machine` keeps the lifted IR over `%State`;
`--level promoted` turns the registers and flags into SSA values; `--level abi`, the default,
applies the inferred signatures, so each function takes its parameters, keeps its locals in allocas
and returns its result, with no `%State` left. `--standalone`, the default at the `abi` level, writes
the data sections as globals and resolves the relocated references to them, so the IR is the one
the oracle and the emit build from. `ras signatures` prints what the inference found.

`ras emit --lang c` structures the raised IR and writes it as C: typed locals, the inferred
parameters and results, loops and conditions, and the data sections as globals, with the helpers
the output needs in `runtime/rhipper.h`. The output is standalone: it builds with clang against
`runtime/driver.c` and needs no image at run time. `--lang cpp` writes the same source as `.cpp`
files with C linkage, for a port that goes on in C++. `--lang rust` writes a cargo package of
unsafe Rust with its own runtime module, `runtime/rhipper.rs`, which cargo builds and runs with no
C toolchain at all. A program of more than 500 functions splits into a header, a data file and a
file per 500 functions, or into one Rust module per 500 functions. Beside the source, `--out` writes
`functions.json`, the table the workbench reads: every function's address, name, size, file and
line, how far the lift got (`lifted`, `partial` with a trap count, `thunk`, `not lifted`), its
calls and its imports.

`ras pipeline` runs a port project end to end from its `rhipper.json`, which names the reference
binary and, with defaults for all of them, the `gen` directory of the translation, its `lang`
(`c`, `cpp` or `rust`), the CMake build (cargo for Rust) and how to run the port. The steps are `analyze`, `lift`, `emit`, `build` and `run`; `--from` and
`--only` pick them. Each step logs to `build/pipeline/<step>.log` and prints a `[  12s] step`
progress line, the build writes a `CMakeLists.txt` for a project that has none, and every run of
the port appends to `build/pipeline/runs.jsonl`. This is what the Rhipper Workbench drives.

The analysis also reads the RTTI a Microsoft C++ ABI program carries: each class with virtual
functions, its bases and its vtables. A virtual function nothing else names takes its slot's name
(`Square_vf2`), and every function lists the string literals it addresses. `ras functions --json`
prints the classes beside the functions.

`ras serve` is the engine as a service for the workbench: JSON-RPC 2.0 over stdio, one request and
one response per line, with the image and its analysis held in memory, so a question about a large
program costs no new analysis. The methods are `project`, `info`, `functions`, `function` (the
analysis, the table entry, the source, the IR and the instructions of one function), `disasm`,
`cfg`, `callers`, `xrefs`, `strings` and `shutdown`. It serves the project of `--project`, or a
bare image with `--exe`, and reloads the image when its file changes.

A 32-bit x86 program lifts through the same model with 32-bit addresses. Its imports take their
arguments from the stack and a `stdcall` import pops them, by the prototype table; the x87 unit is
eight doubles in a global with a top, control word and status word; an `fs:` access lands in a
thread environment block the runtime keeps. The raise tells `cdecl`, `stdcall`, `fastcall` and
`thiscall` apart by the registers a function reads first and the bytes it pops on return, reads a
64-bit result from `edx:eax`, and gives every function a pointer reaches a universal entry point
that takes the arguments in one order and reports what the function pops. The port of a 32-bit
program is a 64-bit executable that keeps the program's 32-bit pointers: it is linked without
large-address awareness, so every address of its process fits in them, and its data words that
hold addresses are written at startup.

`runtime/driver.c` hosts a lifted program at any level: it maps the image at its own base, binds
the import slots, and either gives the machine state a stack or installs a trampoline at each
function's image address and calls the entry as a plain function. The execution oracle in the lift
crate's tests builds each fixture this way with clang at every level and compares its output with
the native program's.

## Build and test

```powershell
cargo build
cargo test
cargo run -p ras -- info testdata/bin/hello64.exe
cargo run -p ras -- cfg testdata/bin/control64.exe 0x140001370
cargo run -p ras -- lift testdata/bin/control64.exe --out out/control
clang -O1 out/control/program.ll runtime/driver.c -I out/control -o out/control/lifted.exe
out/control/lifted.exe
cargo run -p ras -- emit testdata/bin/control64.exe --out out/control
clang -O1 out/control/program.c runtime/driver.c -I out/control -I runtime -o out/control/ported.exe
out/control/ported.exe
```

The fixtures under `testdata/` are small C programs built by clang and lld for x86-64 and x86, with
the binaries committed, so the tests need no compiler. The loader and decoder tests also compare
with `llvm-readobj` and `llvm-objdump` when those are on the PATH, and skip that comparison when
they are not. `testdata/build.ps1` rebuilds the fixtures.

## Release

A tag `v<version>` that matches the workspace version in `Cargo.toml` runs `.github/workflows/release.yml`:
the tests on Windows and Linux, a release build of `ras` for each host, and a GitHub release with
`version.txt` and `ras-<version>-<host>.zip` (the binary with the runtime files) with its `.sha256`
for `windows-x64`, `linux-x64`, `macos-x64` and `macos-arm64`. The archives of Linux and macOS keep
the executable bit of `ras` for an unzip that reads it. The Rhipper Workbench installs `ras` from
those files.

On Linux and macOS `ras` lifts and emits as on Windows. The interpreter needs memory below 4 GB,
which Linux gives and macOS keeps for itself, and answers only the imports it runs itself, with no
Windows DLLs to call. The port of a Windows program links the Windows system libraries and builds
and runs on a Windows host, so the tests that build and run one skip elsewhere and say so. The port
of a Linux program builds with the host's clang beside `runtime/driver.c` and runs on Linux, its
imports bound to the host's C library by name.

## Roadmap

[`rhipper.md`](rhipper.md) is the roadmap, in the format Rhipper Agent reads: milestones A to H
from the image to other architectures, each task with its objective and acceptance. The notes of
the project node record the decisions.

## License

AGPL-3.0-or-later
