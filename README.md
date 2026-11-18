# Rhipper Asm

A static recompiler in pure Rust: a compiled program in, source that builds and runs as a program
of today's machine out. It loads an executable of a PC or a console, decodes its code, lifts each
function to an LLVM-IR-shaped intermediate representation, raises that IR until it reads as
source, and writes it as C, C++ or Rust.

It is the engine of the Rhipper Workbench (`github.com/zeruth/Rhipper-IDE`), where it replaces
Ghidra and the Python Rhipper pipeline (`github.com/zeruth/Rhipper`). Rhipper Agent orchestrates
the work on it.

## What it reads

One `Image` stands behind every container, and one IR behind every architecture, so the analysis,
the raise and the backends are the same for all of them. How far each one is today:

| Program | Container | Processor | Today |
|---|---|---|---|
| Windows, 64-bit | PE | x86-64 | Ports build and run; the fixtures match the native programs' output |
| Windows, 32-bit | PE | x86 with the x87 | Ports build and run as 64-bit executables that keep 32-bit pointers; real programs of a few thousand functions run, one of nineteen thousand builds and starts |
| Windows on ARM | PE | AArch64 | Lifts, raises and emits; fixtures verified |
| Linux, Android | ELF | x86-64, AArch64, ARM32 and Thumb-2, MIPS32 in both byte orders | Lifts and emits; a Linux port runs on Linux against the host's C library |
| macOS | Mach-O, thin or fat | x86-64, AArch64 | Lifts and emits; fixtures verified |
| Xbox | XBE | x86 | The kernel's thunk table by name; fixture verified |
| PlayStation | PS-X EXE | R3000 with the GTE | Lifts with no trap, the BIOS calls by name; a whole program does not run interpreted yet |
| PlayStation 2 | ELF, IRX | R5900, the vector units, the IOP | Lifts with no trap in a program's own code and runs interpreted from `main`; the C library's start does not run yet |
| PSP | PRX | Allegrex with the VFPU | Lifts, the imports named from their NIDs; runs interpreted through its two system calls |
| Nintendo 64 | ROM, in its three byte orders | VR4300 | Lifts; the RSP's microcode is in progress |
| Dreamcast | `1ST_READ.BIN`, plain or scrambled | SH-4 | Loads and decodes; the lift is in progress |
| GameCube, Wii, Xbox 360 | DOL, REL, XEX | PowerPC | Planned: the decoder is the next card |

A console program is proven on small homebrew fixtures built with the open SDKs, not on a game.
Its devices are calls on a platform interface (a register read or written, a kernel function), with
a stub runtime that logs them; the runtimes that render, play sound and read a controller are
planned, not written.

## How it works

```text
executable ──▶ Image ──▶ instructions ──▶ functions, blocks, CFG, classes, signatures
                                                     │
                                  lift: machine IR over an explicit %State
                                  (every register and flag a field of it)
                                                     │
                       raise: promote to SSA, infer signatures, apply the ABI,
                       frames, conditions, types            ──▶  .ll  (clang checks it)
                                                     │
                       emit: structure (if, loops, switch), then C │ C++ │ Rust
                                                     │
                         build with the runtime ──▶ a program that runs
```

Four decisions shape the design:

- **A decoder is a dependency or a table.** x86 wraps `iced-x86` and ARM wraps `yaxpeax-arm`
  behind the crate's own `Instr` record and an `Arch` trait; MIPS is a table of rows, one per
  instruction, from which the decoder, the lift, the assembler and the test cases all come. The
  work of this project is the lift, the raise and the backends, not opcode tables.
- **The IR is a subset of LLVM IR.** `ra-asm-ir` has its own types, values, blocks and printer.
  Every `.ll` the core writes must pass `clang -c -x ir`: that is the syntax and type gate, and
  clang is also how the tests run lifted code against the reference.
- **Fix a defect where it arises.** In the loader, the lift, a pass or a backend, never in one
  function's output. A translation must build in one round.
- **A port is evidence, not a claim.** An interpreter runs the IR at every level of the raise, a
  recorder keeps what each function was given and left behind, and a replayer runs each function
  again after every step. The scoreboard (`docs/scoreboard.md`) is that evidence for the
  fixtures, and a test holds every change to it.

## Using it

```powershell
cargo build
cargo run -p ras -- info testdata/bin/hello64.exe
cargo run -p ras -- emit testdata/bin/control64.exe --out out/control
clang -O1 out/control/program.c runtime/driver.c -I out/control -I runtime -o out/control/ported.exe
out/control/ported.exe
```

`ras` is the command line. What it does, by what one asks of it:

| To | Commands |
|---|---|
| Read an image | `info`, `sections`, `imports`, `exports`, `relocs`, `debug`, `symbols` |
| Read its code | `functions`, `cfg`, `disasm`, `callers`, `xrefs`, `signatures`, `diff` |
| Translate it | `lift` (the IR, at `--level machine`, `promoted` or `abi`), `emit` (`--lang c`, `cpp` or `rust`) |
| Run it without building | `run`, the program through the IR interpreter |
| Port it end to end | `pipeline`, the steps `unpack`, `analyze`, `lift`, `emit`, `build` and `run` of a project's `rhipper.json` |
| Prove it | `verify`, `bisect` (the first raise step and function that differ), `case`, `score`, `smoke` |
| Serve the workbench | `serve` (JSON-RPC over stdio, the image and its analysis kept in memory), `plugins` |
| Check the machine | `doctor`, the tools a port needs, or with `--host` what answers a Windows program's imports off Windows |

`ras lift` raises in three levels. `machine` keeps the lifted IR over `%State`; `promoted` turns
the registers and flags into SSA values; `abi`, the default, applies the inferred signatures, so
each function takes its parameters, keeps its locals in allocas and returns its result, with no
`%State` left.

`ras emit` structures the raised IR and writes source: typed locals, the inferred parameters and
results, loops and conditions, the data sections as structures of named fields, one file per
module of the program when its symbols say which object each function came from. The C output
builds with clang against `runtime/driver.c` and needs no image at run time; `--lang cpp` writes
the same source as `.cpp` files with C linkage; `--lang rust` writes a cargo package of unsafe
Rust over `runtime/rhipper.rs`, which builds with no C toolchain at all. Beside the source it
writes `functions.json`, the table the workbench reads: every function's address, name, size,
file and line, how far the lift got, its calls and its imports.

`ras pipeline` runs a port project from its `rhipper.json`, which names the reference binary and,
with defaults for all of them, the directory of the translation, its language, the build and how
to run the port. Each step logs to `build/pipeline/<step>.log`, the build writes a
`CMakeLists.txt` for a project that has none, and a regeneration says which functions changed.

## What the analysis and the raise know

- **Names.** A PDB or a link map beside the image names every function and says which library
  each came from; an ELF's symbol table does the same. Library signatures made from `.lib`, `.a`
  and `.obj` files name the functions of a statically linked library and keep their bodies out of
  the port.
- **C++.** The RTTI of the Microsoft ABI gives each class with virtual functions, its bases and
  its vtables; a call through a vtable slot is a call to the functions that fill it. Exceptions
  port with their frames, the unwinding and the catch blocks: on x64 with a fixture that proves
  it, on x86 newer and less proven.
- **Calling conventions.** On x86 the raise tells `cdecl`, `stdcall`, `fastcall` and `thiscall`
  apart by the registers a function reads first and the bytes it pops on return, reads a 64-bit
  result from `edx:eax`, and gives every function a pointer reaches a universal entry point.
- **A 32-bit program on a 64-bit host.** The port is linked without large-address awareness, so
  every address of its process fits the program's 32-bit pointers. The imports that hand
  pointers or structures across that boundary go through shims of the runtime, which keep the
  program's layout; the x87 unit is eight doubles in a global, or exact 80-bit arithmetic.
- **What only a run shows.** A traced run resolves the calls through computed targets, and the
  port reads as direct calls after it. Code no reading finds (unpacked, or reached through an
  address computed at run time) is lifted and run by the residual engine when the port meets it.
  A packed program is unpacked by running its stub in the interpreter; the packed fixtures
  still time out on the scoreboard.

## Crates

| Crate | Responsibility |
|---|---|
| `ra-asm-format` | Readers of the containers: ELF, Mach-O, PS-X EXE, the Nintendo 64 ROM, the Dreamcast's boot program, each refusal naming the field and the offset |
| `ra-asm-pe` | The `Image` every container maps to, and the PE itself: headers, sections, imports, exports, relocations, the exception table, TLS, debug, resources, the PDB and the link map, the toolset's fingerprint; the XBE, the PRX's NIDs and the IOP's names |
| `ra-asm-arch` | The architecture-neutral `Instr` record (operands, flow, accesses) and the `Arch` trait every decoder implements |
| `ra-asm-x86`, `ra-asm-arm64` | The x86 and ARM decoder adapters, over iced-x86 and yaxpeax-arm |
| `ra-asm-machine` | Machines described by rows: a decoder, a lift, an access summary, an assembler and the cases of each instruction from one table |
| `ra-asm-mips`, `ra-asm-sh`, `ra-asm-vu` | MIPS from the R3000 to the R5900, the VR4300 and the Allegrex; the Dreamcast's SH-4; the PlayStation 2's vector units |
| `ra-asm-analysis` | Functions, basic blocks, the control-flow graph, jump tables, no-return propagation, classes, library recognition |
| `ra-asm-ir` | The LLVM-IR-shaped intermediate representation, its `.ll` printer and its interpreter |
| `ra-asm-protos` | The import prototype tables generated from the SDK headers, apart from the lift so its edits do not recompile them |
| `ra-asm-lift` | Instructions to machine IR over an explicit `%State`, and the raising passes: signature inference, state promotion, the ABI rewrite, conditions, frames, simplification |
| `ra-asm-emit` | The structuring pass, the C backend with its C++ dialect, the Rust backend, the data layouts and the function table |
| `ra-asm-host` | The interpreter's host: the Win32 boundary a lifted program runs against, the record of a run and its replay, the unpacker |
| `ra-asm-residual` | The residual engine: code the static analysis never found, lifted and run from the program's memory |
| `ra-asm-verify` | Function vectors: the recorder, the replayer at each level, the generator for functions no run entered |
| `ra-asm-wasm` | A sandboxed WebAssembly interpreter for the engine's plugins |
| `ras` (app) | The command line and the service |

`runtime/` is what a port links: `driver.c` and `rhipper.h` for C and C++, `rhipper.rs` for Rust,
`verify.c` and `replay.c` for a port that checks itself against recorded vectors.

## Tests

`cargo test` is the fast tier and the gate of every change: no tool but cargo, since the fixtures
under `testdata/` are committed as binaries beside their sources. They are small programs built
by clang for Windows, Linux and macOS, and by the open console SDKs in WSL (PSn00bSDK, ps2dev,
pspdev, libdragon, KallistiOS); `testdata/README.md` says what each one holds and how it was
built. The loader and decoder tests also compare with `llvm-readobj` and `llvm-objdump` when
those are on the PATH.

The slow tier is behind the `oracle` feature of `ra-asm-lift`, `ra-asm-emit` and `ras`: it builds
every fixture's port with clang at every level and compares its output with the native program's.
A nightly workflow runs it, and another fuzzes the loaders and the decoders.

`docs/scoreboard.md` is the state of the fixtures in one table: 59 programs today, with the
functions lifted at each level, the traps, the functions whose vectors replay, and the gotos and
casts per thousand lines of the C. `cargo run -p ras -- score --write testdata/bin` rewrites it.

## Release

A tag `v<version>` that matches the workspace version in `Cargo.toml` runs
`.github/workflows/release.yml`: the tests on Windows and Linux, a release build of `ras` for each
host, and a GitHub release with `version.txt` and `ras-<version>-<host>.zip` (the binary with the
runtime files) with its `.sha256` for `windows-x64`, `linux-x64`, `macos-x64` and `macos-arm64`.
The Rhipper Workbench installs `ras` from those files.

On Linux and macOS `ras` lifts and emits as on Windows. The interpreter needs memory below 4 GB,
which Linux gives and macOS keeps for itself, and answers only the imports it runs itself, with no
Windows DLLs to call. The port of a Windows program links the Windows system libraries and builds
and runs on a Windows host, so the tests that build and run one skip elsewhere and say so.

## Roadmap

[`rhipper.md`](rhipper.md) is the roadmap, in the format Rhipper Agent reads: milestones from the
image to the platform runtimes, each card with its objective, its acceptance and the files it
names. What is ahead, in the order it stands there: PowerPC and its consoles, the SH-4 lift, the
platform runtimes that make a console program a PC program (graphics, audio, input, threads),
and after the homebrew, the games.

## License

AGPL-3.0-or-later
