# Rhipper Asm

- 1 ‹project› Rhipper Asm {c5} [in_progress]
    objective: A pure-Rust disassembly and lifting core that replaces Ghidra and the Python Rhipper in the Rhipper Workbench: a PE image in, LLVM IR out, and C, C++ or Rust source out of that IR.
    acceptance: The workbench runs a project from the reference binary to a built and running port with no Ghidra, no JDK and no Python installed.
    notes: Decisions, 2026-11-10. (1) The decoder is the iced-x86 crate behind the crate's own `Instr` record, so the work goes into the lift and the backends, not into opcode tables; a second architecture plugs in behind the same trait. (2) The IR is a subset of LLVM IR with its own printer: every `.ll` the core writes must pass `clang -c -x ir`, which is the syntax and type gate, and clang is also the way to run lifted code in tests. (3) The first target is x86-64 PE; the loader and the decoder handle PE32 and 32-bit code from the start because the old pipeline's test programs are 32-bit. (4) Milestones run in order A to H below; a later milestone may start when its inputs exist.
  - 2 ‹milestone› A: the image {c3} [implemented]
      objective: `ra-asm-pe` maps a PE file to an image by RVA and reads every structure the later steps need: headers, sections, imports, exports, relocations, the exception table, TLS, and the debug directory.
      acceptance: `ras info`, `ras sections`, `ras imports` and `ras exports` print the fixtures correctly, and the loader tests agree with `llvm-readobj` on every fixture.
    - 3 ‹section› Readers {c1} [implemented]
        objective: `ra-asm-format` holds a little-endian cursor whose every refusal names the offset and the field.
        acceptance: A truncated header fails with a message that names the offset; the cursor tests pass.
    - 4 ‹section› Headers, sections and the flat image {c2} [implemented]
        objective: The DOS, COFF and optional headers (PE32 and PE32+), the data directories and the section table; an image laid out by RVA with RVA, VA and file-offset conversions and bounded reads.
        acceptance: `ras info` prints machine, subsystem, image base, entry point and the section table of each fixture; a read past a section fails with the address in the message.
    - 5 ‹section› Directories {c3} [implemented]
        objective: The import table (names, ordinals, IAT slots), the export table (names, ordinals, forwarders), base relocations, the x64 exception table with its unwind info and chained entries, the TLS directory with its callbacks, and the CodeView record of the debug directory.
        acceptance: `ras imports` and `ras exports` agree with `llvm-readobj --coff-imports --coff-exports`; `ras functions --pdata` lists every RUNTIME_FUNCTION of the x64 fixtures.
    - 6 ‹section› Fixtures and the oracle {c2} [implemented]
        objective: `testdata/` holds small C programs, built by clang for x86-64 and x86 with lld, and the binaries they produce; a test compares the loader with `llvm-readobj` when it is on the PATH and skips when it is not.
        acceptance: `cargo test -p ra-asm-pe` passes with and without LLVM on the PATH; `testdata/build.ps1` rebuilds every fixture.
        notes: Done 2026-11-10. Six fixtures (hello, control, exports; 64 and 32), 2 to 6 KB each, no C runtime; -ffreestanding drops the x64 unwind tables, so the build uses -fno-builtin -funwind-tables. The native output of each program is kept under testdata/expected for the execution oracle.
  - 7 ‹milestone› B: the instructions {c3} [in_progress]
      objective: `ra-asm-x86` turns the bytes of a function into `Instr` records with operands, length, flow kind, and the registers and memory each one reads and writes, in 64-bit and 32-bit mode.
      acceptance: `ras disasm <exe> <address> [length]` prints a function; a test decodes every fixture's `.text` and agrees with `llvm-objdump` on each instruction's length and mnemonic.
    - 8 ‹task› The decoder adapter {c2} [defined]
        objective: An `Arch` trait with an x86 implementation over iced-x86: decode one instruction, give its text, its flow (fall through, jump, conditional jump, call, return, indirect, trap), its branch target, and its register and memory accesses.
        acceptance: A unit test decodes a known byte sequence and checks the operands, the flow and the target.
    - 9 ‹task› `ras disasm` {c1} [defined]
        objective: The command prints `address  mnemonic operands`, with a note for a RIP-relative or absolute target inside the image, and the C string there when there is one, as the old `disasm.py` did.
        acceptance: The command on a fixture's entry point prints the whole function by its `.pdata` extent.
        waits-on: 8
    - 10 ‹task› The objdump oracle {c2} [defined]
        objective: A test disassembles the `.text` of every fixture linearly and compares lengths and mnemonics with `llvm-objdump -d`; a difference prints both sides.
        acceptance: The test passes on every fixture, and skips with a message when `llvm-objdump` is not on the PATH.
        waits-on: 8
  - 11 ‹milestone› C: functions and control flow {c4} [defined]
      objective: `ra-asm-analysis` finds the functions of an image and builds the control-flow graph of each one.
      acceptance: `ras functions` lists every function of the fixtures with its size and block count, and finds the same functions as the link map of each fixture.
    - 12 ‹task› Function discovery {c3} [defined]
        objective: Entry point, exports, TLS callbacks, every `.pdata` entry (x64), then recursive descent through direct calls; import thunks (`jmp [iat]`) become named thunks, not functions; chained unwind entries join their parent.
        acceptance: On the x64 fixtures the function list matches the lld map file; on the x86 fixtures every function the map names is found.
    - 13 ‹task› Basic blocks and the graph {c3} [defined]
        objective: Each function splits into blocks at jump targets and after control transfers; each block names its successors; a `.pdata` extent bounds a function and a jump out of it is a tail call.
        acceptance: `ras cfg <address>` prints the blocks and edges of a function; a test checks a function with a loop and an if-else.
        waits-on: 12
    - 14 ‹task› Jump tables {c3} [defined]
        objective: The MSVC and clang x64 switch forms (`lea base; mov/movsxd index,[base+idx*4]; add; jmp`) and the x86 form (`jmp [table+idx*4]`) resolve to their targets, bounded by the compare before them and by the function extent.
        acceptance: A fixture with a dense switch has every case block reachable in `ras cfg`.
        waits-on: 13
    - 15 ‹task› Functions that never return {c2} [defined]
        objective: Known imports (`ExitProcess`, `exit`, `abort`, `_exit`, `__fastfail`, and the CRT's invalid-parameter handlers) and any function whose every path ends in one are marked no-return, so the bytes after a call to them start a new block or function.
        acceptance: A fixture calling `ExitProcess` in the middle of `.text` decodes the code after the call as its own function.
        waits-on: 12
  - 16 ‹milestone› D: the IR and the lift {c5} [defined]
      objective: `ra-asm-ir` is an LLVM-IR-shaped SSA IR with a `.ll` printer, and `ra-asm-lift` turns each function's instructions into it with the machine state explicit.
      acceptance: `ras lift <exe> --ll out.ll` writes a module that `clang -c -x ir` accepts for every fixture, and the lifted fixtures print what the references print when run through the test harness.
    - 17 ‹task› The IR {c3} [defined]
        objective: Types (`i1` to `i128`, `half` to `double`, vectors, `ptr`), values, the instruction subset (alloca, load, store, the integer and float binops, icmp, fcmp, select, br, switch, call, ret, unreachable, the casts, phi, a few intrinsics), blocks, functions, globals and declarations, with an address attached to each instruction for the backends and the IDE.
        acceptance: The printer round-trips a hand-built module through `clang -c -x ir` without error; `cargo test -p ra-asm-ir` passes.
    - 18 ‹task› The machine-state lift {c4} [defined]
        objective: Each native function becomes an IR function over a `%State` pointer: registers are fields, flags are fields computed by each instruction that defines them, memory goes through `inttoptr` into the flat image, a direct call calls the lifted callee, an import calls a declared external through an adapter, and an indirect call goes through a resolver.
        acceptance: `ras lift` on the x64 fixtures passes clang; the integer subset (moves, arithmetic, logic, shifts, compares, setcc, cmovcc, jumps, calls, push and pop, lea, the widening moves, multiply and divide, the string moves) is covered.
        waits-on: 17 13
    - 19 ‹task› The execution oracle {c3} [defined]
        objective: A test lifts a fixture, compiles the `.ll` with clang together with a C driver that maps the image and calls the lifted entry, runs it, and compares the output with the reference's.
        acceptance: The x64 fixtures print the same lines lifted as native.
        waits-on: 18
    - 20 ‹task› SSE and the rest of the subset {c3} [defined]
        objective: Scalar SSE arithmetic and conversions, packed moves and zeroing idioms, bit tests, rotates, adc and sbb, bswap, xchg and cmpxchg, the no-ops and traps.
        acceptance: A float fixture passes the execution oracle.
        waits-on: 19
  - 21 ‹milestone› E: raising {c5} [defined]
      objective: Passes turn the machine-state IR into source-shaped IR: stack slots become locals, state fields become SSA values, flags become comparisons, calling conventions become signatures, and data references become typed globals.
      acceptance: The raised IR of the fixtures still passes the execution oracle and is a fraction of the machine IR's size.
    - 22 ‹task› State promotion and dead flags {c3} [defined]
        objective: Loads and stores of state fields within a function become SSA values with phis; flag definitions that no use reaches disappear.
        acceptance: No `%State` field load or store remains in a leaf function with no calls; the oracle still passes.
    - 23 ‹task› Stack frames and locals {c4} [defined]
        objective: `rsp`- and `rbp`-relative slots become allocas sized by their accesses; the frame setup and teardown disappear.
        acceptance: A fixture function with locals shows them as allocas with no stack pointer arithmetic.
        waits-on: 22
    - 24 ‹task› Calling conventions and signatures {c4} [defined]
        objective: Win64 (`rcx rdx r8 r9`, `xmm0`-`xmm3`, the stack, `rax` and `xmm0` out) and the x86 conventions (`cdecl`, `stdcall`, `fastcall`, `thiscall`) by the registers and slots a function reads before writing and writes before returning; imports take their prototypes from a table.
        acceptance: Each fixture function gets the parameter count of its source; the oracle calls raised functions by their signatures.
        waits-on: 23
    - 25 ‹task› Conditions, globals and types {c4} [defined]
        objective: `cmp` and `test` followed by a conditional become one comparison; references into data sections become globals of the width their uses say; integer widths, pointers and floats come from the uses.
        acceptance: A raised fixture reads as C-shaped IR with named globals and `icmp`-driven branches.
        waits-on: 24
  - 26 ‹milestone› F: the backends {c5} [defined]
      objective: `ra-asm-emit` writes C, C++ or Rust source from the raised IR, with a runtime header per language, and writes the corpus and fidelity files the workbench reads today.
      acceptance: A fixture's C, C++ and Rust outputs each build in one round and print what the reference prints.
    - 27 ‹task› Structuring {c4} [defined]
        objective: Reducible graphs become if, else, while and switch; a C or C++ output keeps gotos for the rest; the Rust output uses labeled loops and breaks, since Rust has no goto.
        acceptance: Every fixture function structures without a goto in C; the Rust output compiles.
    - 28 ‹task› C {c4} [defined]
        objective: One `.c` per source file or per address range, a `/* ref: name */` comment above each function so the workbench's gen index keeps working, and a `rhipper.h` runtime for the helpers.
        acceptance: The x64 fixtures compile with clang and MSVC with no warning and pass their expected output.
        waits-on: 27
    - 29 ‹task› C++ {c3} [defined]
        objective: The C output with references, classes where RTTI and vtables name them, and overloads where the mangled export names give them.
        acceptance: The C++ fixture builds and runs.
        waits-on: 28
    - 30 ‹task› Rust {c4} [defined]
        objective: Unsafe Rust: one module per source range, `extern "system"` declarations for the imports, labeled loops from the structuring pass, and a `rhipper` runtime crate.
        acceptance: The x64 fixtures build with cargo and pass their expected output.
        waits-on: 27
    - 31 ‹task› The corpus and fidelity files {c2} [defined]
        objective: `build/corpus.jsonl` (addr, name, size, c or error) and `build/fidelity.json` in the shapes the workbench's `RhipperCorpus` and `RhipperFidelity` read, so the tool windows work before the IDE moves to the new API.
        acceptance: The workbench opens a project produced by `ras` and lists its functions.
        waits-on: 28
  - 32 ‹milestone› G: the pipeline and the workbench {c4} [defined]
      objective: `ras` is the engine of the Rhipper Workbench: a pipeline with steps, a JSON interface the IDE calls, and a managed install from releases; Ghidra, the JDK and Python leave the tool list.
      acceptance: The workbench runs the pipeline on a project with only `ras`, clang and CMake installed.
    - 33 ‹task› The pipeline steps {c3} [defined]
        objective: `ras pipeline --from <step>` runs analyze, lift, emit, build and run, writes `build/pipeline/<step>.log` and the `[  12s] step` progress lines the IDE's status reader parses, and reads its settings from `rhipper.json`.
        acceptance: The IDE's step buttons drive `ras` on a project with `"engine": "ras"` in its `rhipper.json`.
    - 34 ‹task› `ras serve` {c3} [defined]
        objective: JSON-RPC over stdio: project, functions query, function detail (bytes, instructions, IR, source), callers, xrefs, strings; the IDE and its MCP tools call it instead of Python scripts.
        acceptance: The MCP tools of the workbench answer from `ras serve` on a project with no Python installed.
        waits-on: 33
    - 35 ‹task› The managed install {c2} [defined]
        objective: The workbench downloads `ras` from this repository's releases into its tools directory, as it does Rhipper today, and a CI workflow here builds and publishes the release.
        acceptance: A fresh workbench installs `ras` on first start and shows its version under Tools.
        waits-on: 33
    - 36 ‹task› The facts of the old pipeline {c4} [defined]
        objective: What the Python extractors produced as facts (prototypes, no-return, vtables, switches, pieces, function pointers, pointer globals, float data) is an analysis of the core, run on demand and cached in `build/`.
        acceptance: A 32-bit test program of the old pipeline ports through `ras` with fidelity equal to the old pipeline's.
        waits-on: 34
  - 37 ‹milestone› H: other architectures and formats {c4} [defined]
      objective: The `Arch` trait and the loader take a second architecture and a second container without changes to the IR, the raising passes or the backends.
      acceptance: An ARM64 PE lifts to IR that passes clang, and an ELF x86-64 file loads and disassembles.
    - 38 ‹task› ARM64 PE {c4} [defined]
        objective: An `Arch` implementation for AArch64 over a maintained decoder crate, the ARM64 exception table, and the lift of the integer subset.
        acceptance: A clang-built ARM64 fixture lifts and passes the IR gate.
    - 39 ‹task› ELF and Mach-O {c3} [defined]
        objective: `ra-asm-format` grows the ELF and Mach-O containers behind the same `Image` trait.
        acceptance: `ras info` prints a Linux x86-64 binary's sections and symbols.
