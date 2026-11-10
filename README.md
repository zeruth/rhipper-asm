# Rhipper Asm

The disassembly core of Rhipper, in pure Rust: a replacement for Ghidra's front end. It reads a
PE file, lifts machine code to an intermediate representation, and hands the IR to translators
that emit compilable C — later also LLVM IR and other targets.

It replaces the Ghidra headless step of the Rhipper pipeline (`github.com/zeruth/Rhipper`), not
the pipeline itself. Rhipper Agent orchestrates the work around it.

## Crates

| Crate | Responsibility |
|---|---|
| `ra-asm-pe` | The PE image: headers, sections, imports, exports, relocations |
| `ra-asm-x86` | The x86-64 decoder: bytes to instruction records |
| `ra-asm-ir` | The intermediate representation: instructions, registers, control flow |
| `ra-asm-format` | Shared binary-format helpers (later COFF, ELF) |

## First milestone

x86-64 PE loader plus disassembly to IR. LLVM emission follows once the IR holds real programs.

## License

AGPL-3.0-or-later
