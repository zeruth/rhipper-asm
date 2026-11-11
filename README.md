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
| `ra-asm-x86` | The x86 and x86-64 decoder adapter: bytes to `Instr` records with operands, flow and accesses |
| `ra-asm-analysis` | Functions, basic blocks, the control-flow graph, jump tables, no-return propagation |
| `ra-asm-ir` | The LLVM-IR-shaped intermediate representation and its `.ll` printer |
| `ra-asm-lift` | Instructions to machine IR, and the raising passes |
| `ra-asm-emit` | The C, C++ and Rust backends (planned) |
| `ras` (app) | The command line: `info`, `sections`, `imports`, `exports`, `functions`, `disasm`, `cfg`, `lift` |

## Build and test

```powershell
cargo build
cargo test
cargo run -p ras -- info testdata/bin/hello64.exe
```

The fixtures under `testdata/` are small C programs built by clang and lld for x86-64 and x86, with
the binaries committed, so the tests need no compiler. The loader and decoder tests also compare
with `llvm-readobj` and `llvm-objdump` when those are on the PATH, and skip that comparison when
they are not. `testdata/build.ps1` rebuilds the fixtures.

## Roadmap

[`rhipper.md`](rhipper.md) is the roadmap, in the format Rhipper Agent reads: milestones A to H
from the image to other architectures, each task with its objective and acceptance. The notes of
the project node record the decisions.

## License

AGPL-3.0-or-later
