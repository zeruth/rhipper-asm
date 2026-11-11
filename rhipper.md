# Rhipper Asm

- 1 ‹project› Rhipper Asm {c5} [in_progress]
    objective: A pure-Rust disassembly and lifting core that replaces Ghidra and the Python Rhipper in the Rhipper Workbench: a PE image in, LLVM IR out, and C, C++ or Rust source out of that IR.
    acceptance: The workbench runs a project from the reference binary to a built and running port with no Ghidra, no JDK and no Python installed.
    notes: Decisions, 2026-11-10. (1) The decoder is the iced-x86 crate behind the crate's own `Instr` record, so the work goes into the lift and the backends, not into opcode tables; a second architecture plugs in behind the same trait. (2) The IR is a subset of LLVM IR with its own printer: every `.ll` the core writes must pass `clang -c -x ir`, which is the syntax and type gate, and clang is also the way to run lifted code in tests. (3) The first target is x86-64 PE; the loader and the decoder handle PE32 and 32-bit code from the start because the old pipeline's test programs are 32-bit. (4) Milestones run in order A to H below; a later milestone may start when its inputs exist. (5) No compatibility with the old pipeline's files or conventions (Ghidra's corpus and names, the fidelity scores, the gen-index comments): the engine writes its own shapes and the workbench's readers of the old ones are deleted as the new ones land, decided 2026-11-10; Ghidra-based projects stop opening in the workbench until `ras` lifts 32-bit programs. (6) The lift is exact or it stops: an instruction the lift does not cover becomes a trap that names its address, never a guess, so the grade of a function is its trap count and the oracle is the proof; readability work (widths, types, idioms) must keep the oracle green. (7) 32-bit x86 is its own milestone (I), ahead of ARM64 and ELF (H), because it is what lets the old test programs and projects port through `ras`. Vision, 2026-11-10: milestones A to D are done and E to G are in progress; the engine lifts, raises and writes x64 programs to C that builds and runs, the workbench runs it end to end, and nothing of Ghidra or Python remains. What follows, in order: the readability that is still mechanical (widths, then the C++ and Rust printers over the same structured IR), the engine as a service and as a release, 32-bit x86, and then the depth that turns a correct port into a readable one: types from access patterns and debug info, names from RTTI and the CRT, idioms, a hand-port takeover per function, exceptions, resources, and a side-by-side trace of the reference and the port (milestone J). Each of those is a nice-to-have until the one before it is solid, and the oracle and the fixtures grow with each.
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
  - 7 ‹milestone› B: the instructions {c3} [implemented]
      objective: `ra-asm-x86` turns the bytes of a function into `Instr` records with operands, length, flow kind, and the registers and memory each one reads and writes, in 64-bit and 32-bit mode.
      acceptance: `ras disasm <exe> <address> [length]` prints a function; a test decodes every fixture's `.text` and agrees with `llvm-objdump` on each instruction's length and mnemonic.
    - 8 ‹task› The decoder adapter {c2} [implemented]
        objective: An `Arch` trait with an x86 implementation over iced-x86: decode one instruction, give its text, its flow (fall through, jump, conditional jump, call, return, indirect, trap), its branch target, and its register and memory accesses.
        acceptance: A unit test decodes a known byte sequence and checks the operands, the flow and the target.
        notes: Done 2026-11-10. The neutral record and the Arch trait live in a small ra-asm-arch crate, so a second architecture and the analysis share them without depending on x86. X86::decode_full keeps iced's Instruction beside the record for the lift.
    - 9 ‹task› `ras disasm` {c1} [implemented]
        objective: The command prints `address  mnemonic operands`, with a note for a RIP-relative or absolute target inside the image, and the C string there when there is one, as the old `disasm.py` did.
        acceptance: The command on a fixture's entry point prints the whole function by its `.pdata` extent.
        waits-on: 8
    - 10 ‹task› The objdump oracle {c2} [implemented]
        objective: A test disassembles the `.text` of every fixture linearly and compares lengths and mnemonics with `llvm-objdump -d`; a difference prints both sides.
        acceptance: The test passes on every fixture, and skips with a message when `llvm-objdump` is not on the PATH.
        waits-on: 8
  - 11 ‹milestone› C: functions and control flow {c4} [implemented]
      objective: `ra-asm-analysis` finds the functions of an image and builds the control-flow graph of each one.
      acceptance: `ras functions` lists every function of the fixtures with its size and block count, and finds the same functions as the link map of each fixture.
    - 12 ‹task› Function discovery {c3} [implemented]
        objective: Entry point, exports, TLS callbacks, every `.pdata` entry (x64), then recursive descent through direct calls; import thunks (`jmp [iat]`) become named thunks, not functions; chained unwind entries join their parent.
        acceptance: On the x64 fixtures the function list matches the lld map file; on the x86 fixtures every function the map names is found.
        notes: Done 2026-11-10. A fixpoint: seeds, descent, a call target inside a built function splits it, a function proven no-return makes its callers rebuild. The analysis finds exactly the map's functions on all four executables. Pointer seeds run last so that x86 jump-table entries are owned by their function first.
    - 13 ‹task› Basic blocks and the graph {c3} [implemented]
        objective: Each function splits into blocks at jump targets and after control transfers; each block names its successors; a `.pdata` extent bounds a function and a jump out of it is a tail call.
        acceptance: `ras cfg <address>` prints the blocks and edges of a function; a test checks a function with a loop and an if-else.
        waits-on: 12
    - 14 ‹task› Jump tables {c3} [implemented]
        objective: The MSVC and clang x64 switch forms (`lea base; mov/movsxd index,[base+idx*4]; add; jmp`) and the x86 form (`jmp [table+idx*4]`) resolve to their targets, bounded by the compare before them and by the function extent.
        acceptance: A fixture with a dense switch has every case block reachable in `ras cfg`.
        notes: Done 2026-11-10 for the clang x64 form (table-relative entries), the MSVC x64 form (RVA entries) and the x86 absolute form. The MSVC two-level form with a byte index table is not handled yet: no MSVC-built fixture exists; it comes with the first MSVC fixture.
        waits-on: 13
    - 15 ‹task› Functions that never return {c2} [implemented]
        objective: Known imports (`ExitProcess`, `exit`, `abort`, `_exit`, `__fastfail`, and the CRT's invalid-parameter handlers) and any function whose every path ends in one are marked no-return, so the bytes after a call to them start a new block or function.
        acceptance: A fixture calling `ExitProcess` in the middle of `.text` decodes the code after the call as its own function.
        waits-on: 12
  - 16 ‹milestone› D: the IR and the lift {c5} [implemented]
      objective: `ra-asm-ir` is an LLVM-IR-shaped SSA IR with a `.ll` printer, and `ra-asm-lift` turns each function's instructions into it with the machine state explicit.
      acceptance: `ras lift <exe> --ll out.ll` writes a module that `clang -c -x ir` accepts for every fixture, and the lifted fixtures print what the references print when run through the test harness.
    - 17 ‹task› The IR {c3} [implemented]
        objective: Types (`i1` to `i128`, `half` to `double`, vectors, `ptr`), values, the instruction subset (alloca, load, store, the integer and float binops, icmp, fcmp, select, br, switch, call, ret, unreachable, the casts, phi, a few intrinsics), blocks, functions, globals and declarations, with an address attached to each instruction for the backends and the IDE.
        acceptance: The printer round-trips a hand-built module through `clang -c -x ir` without error; `cargo test -p ra-asm-ir` passes.
    - 18 ‹task› The machine-state lift {c4} [implemented]
        objective: Each native function becomes an IR function over a `%State` pointer: registers are fields, flags are fields computed by each instruction that defines them, memory goes through `inttoptr` into the flat image, a direct call calls the lifted callee, an import calls a declared external through an adapter, and an indirect call goes through a resolver.
        acceptance: `ras lift` on the x64 fixtures passes clang; the integer subset (moves, arithmetic, logic, shifts, compares, setcc, cmovcc, jumps, calls, push and pop, lea, the widening moves, multiply and divide, the string moves) is covered.
        waits-on: 17 13
    - 19 ‹task› The execution oracle {c3} [implemented]
        objective: A test lifts a fixture, compiles the `.ll` with clang together with a C driver that maps the image and calls the lifted entry, runs it, and compares the output with the reference's.
        acceptance: The x64 fixtures print the same lines lifted as native.
        notes: Done 2026-11-10. hello64 and control64 run lifted and print their native output. The driver (runtime/driver.c) maps the image at its base, binds the IAT through GetProcAddress, gives rsp a 1 MiB stack and calls F_<entry>. Imports go through an eight-integer adapter; an indirect call goes through ra_call, which finds the lifted function or calls native code. Limits recorded as tasks: 32-bit import calls need an arity table; a 64-bit division whose dividend does not fit 64 bits traps; fs and gs reads are flat.
        waits-on: 18
    - 20 ‹task› SSE and the rest of the subset {c3} [implemented]
        objective: Scalar SSE arithmetic and conversions, packed moves and zeroing idioms, bit tests, rotates, adc and sbb, bswap, xchg and cmpxchg, the no-ops and traps.
        acceptance: A float fixture passes the execution oracle.
        notes: Done 2026-11-10. testdata/src/float.c (scalar arithmetic in both widths, conversions both ways, NaN and infinity compares, a struct of floats) runs lifted and matches. The packed float operations, the compares with a predicate, movmskps/pd, the packed conversions and the shuffles are lifted too; x87 is not, and comes with a 32-bit fixture that uses it.
        waits-on: 19
  - 21 ‹milestone› E: raising {c5} [implemented]
      objective: Passes turn the machine-state IR into source-shaped IR: stack slots become locals, state fields become SSA values, flags become comparisons, calling conventions become signatures, and data references become typed globals.
      acceptance: The raised IR of the fixtures still passes the execution oracle and is a fraction of the machine IR's size.
    - 22 ‹task› State promotion and dead flags {c3} [implemented]
        objective: Loads and stores of state fields within a function become SSA values with phis; flag definitions that no use reaches disappear.
        acceptance: No `%State` field load or store remains in a leaf function with no calls; the oracle still passes.
        notes: Done 2026-11-10. ra-asm-lift/src/raise: promote (liveness-guided SSA construction over the register and flag slots; a call flushes the dirty registers and forgets them; the flags are dead at calls and returns), simplify (identities and constant folding), dce (unused pure values, trivial phis). The oracle runs both the machine and the raised IR of every fixture. Lines of IR: hello64 634 to 206, control64 4975 to 2072, float64 2722 to 1825. The vector registers stay in memory until the type-aware promotion. A leaf function still loads its live-in registers and stores its results: that is the signature pass's job (task 24).
    - 23 ‹task› Stack frames and locals {c4} [implemented]
        objective: `rsp`- and `rbp`-relative slots become allocas sized by their accesses; the frame setup and teardown disappear.
        acceptance: A fixture function with locals shows them as allocas with no stack pointer arithmetic.
        notes: Done 2026-11-10. raise/frame.rs follows the stack pointer arithmetic from the frame top: a pointer at a constant offset becomes a gep from the frame, and a slot that is loaded and stored at one type, whose address neither escapes to a call nor flows into dynamic pointer arithmetic, becomes its own alloca; a slot at or above an escaped address stays in the frame, since the callee may reach it through the pointer. The ABI rewrite drops the pushed return address, and a store of undef (a saved register) goes. A leaf function with no locals has no frame at all: op_div is a null check and a division, fib a loop with two phis and a call. Remaining for later: promoting the private allocas to SSA (LLVM's mem2reg does it today), and narrowing the i64 register values to the width their uses have.
        waits-on: 22
    - 24 ‹task› Calling conventions and signatures {c4} [implemented]
        objective: Win64 (`rcx rdx r8 r9`, `xmm0`-`xmm3`, the stack, `rax` and `xmm0` out) and the x86 conventions (`cdecl`, `stdcall`, `fastcall`, `thiscall`) by the registers and slots a function reads before writing and writes before returning; imports take their prototypes from a table.
        acceptance: Each fixture function gets the parameter count of its source; the oracle calls raised functions by their signatures.
        notes: Inference done 2026-11-10 (raise/abi.rs, liveness.rs, stack.rs, prototypes.rs; `ras signatures`). The parameters are the argument registers read before written under a call model where a call reads its callee's parameters (iterated to a fixpoint) and writes the caller-saved registers; imports read what a prototype table says, unknown ones all four; the stack arguments come from a symbolic stack-pointer analysis that also sizes the frame. A root returns a value when it writes rax; the rest when a caller reads rax after the call. The lift now zeroes a register on `xor r, r` and `pxor x, x` without reading it, and calls imports through per-import wrappers, so neither shows as a read. The signatures of control64 and float64 match the binaries (clang folded constant arguments into collatz, mean and mix). The rewrite landed the same day: raise/promote.rs in ABI mode gives each function its parameters, a frame alloca whose top is the entry stack pointer (8 past a 16-byte boundary, as after a native call), calls with arguments and results, float arguments and results through a local vector array, imports through their adapters, indirect calls through the pointer itself, and a returned value; no %State remains. runtime/driver.c installs a 12-byte trampoline at each lifted function's image address, so a pointer into the image reaches the host function. The oracle runs every fixture at the machine, promoted and ABI levels. Lines of IR at the ABI level: hello64 173, control64 1159, float64 1568. The stack frame is still addressed through inttoptr of the frame top: task 23 turns the slots into allocas.
        waits-on: 23
    - 25 ‹task› Conditions, globals and types {c4} [implemented]
        objective: `cmp` and `test` followed by a conditional become one comparison; references into data sections become globals of the width their uses say; integer widths, pointers and floats come from the uses.
        acceptance: A raised fixture reads as C-shaped IR with named globals and `icmp`-driven branches.
        notes: The conditions landed 2026-11-10 (raise/conditions.rs): the sf, of, zf and cf formulas of a subtraction fold back into one icmp of the operands, with the inverse and the le/be combinations; simplify also folds trunc of zext and per-block common subexpressions. Lines of IR at the ABI level: hello64 132, control64 665 (from 4975 machine), float64 945. The globals landed next (ra-asm-lift/src/data.rs): in standalone mode each data section is one global, a packed structure of byte runs and address words (a relocated word is the address of the function or the data byte it names; an import slot is the import's own address, bound at link time); a RIP-relative or absolute reference becomes the address of a byte of a global; a switch jumps by its index register, read before the table entry, so the table is never read. The ABI level is standalone: the driver maps nothing and binds nothing, and all three fixtures run that way. The widths landed 2026-11-11 (raise/widths.rs): a demand analysis finds the low bits each i64 value's uses read and a known-zero analysis how far up a value is zero; an extension, an addition and its kin, a phi, a select or a switch at a demand below 64 is defined again at that width, an unsigned compare of two values zero above the same width compares there, and the parameters and results of a function that no outside caller or pointer reaches narrow to what the body demands and the callers accept, as an optimistic fixpoint across the functions (a dispatcher that returns a callee's result as it is otherwise keeps both at 64). `fib` reads `uint32_t F(uint32_t)` with `uint32_t` phis, `op_div` divides `uint32_t`s. The C printer drops the cast around a 32-bit or 64-bit operation and prints a signed compare's constant as a number. Left for the idiom work of milestone J: the lift's own division-by-zero guard repeats the program's check (a known-conditions pass), and `cdq; idiv` reads as a shift-or instead of a sign extension.
        waits-on: 24
  - 26 ‹milestone› F: the backends {c5} [in_progress]
      objective: `ra-asm-emit` writes C, C++ or Rust source from the raised IR, with a runtime header per language, and the function table the workbench reads.
      acceptance: A fixture's C, C++ and Rust outputs each build in one round and print what the reference prints.
      notes: Started 2026-11-10 with the C backend; the three x64 fixtures build from their C with clang and print what the reference prints.
    - 27 ‹task› Structuring {c4} [implemented]
        objective: Reducible graphs become if, else, while and switch; a C or C++ output keeps gotos for the rest; the Rust output uses labeled loops and breaks, since Rust has no goto.
        acceptance: Every fixture function structures without a goto in C; the Rust output compiles.
        notes: Landed 2026-11-10 (ra-asm-emit/src/structure.rs), language-neutral: dominators and post-dominators give the join of each branch, natural loops give `for (;;)` with break and continue, a switch terminator gives a switch, and a block that only computes a condition folds into the branch above it as `&&` or `||`, so a short-circuit reads as one. Anything else falls back to a goto with a label, and the fixtures have none. The Rust half (labeled loops) waits for task 30.
    - 28 ‹task› C {c4} [implemented]
        objective: C that builds in one round with clang and clang-cl: typed locals, structured control flow, the data as globals, a `rhipper.h` runtime for the helpers, and a split into a header, a data file and one `.c` per 500 functions when a program is large.
        acceptance: The x64 fixtures compile with clang with no warning and pass their expected output, as one file and split.
        notes: `ras emit --lang c` writes `program.c` and `runtime/rhipper.h` holds the helpers (the 128-bit and vector types, the intrinsics, the trap and the call resolver). Landed 2026-11-10: typed locals from the IR, phis through `_next` temporaries, single-use values inlined into their expression, globals as packed structures, the frame and the xmm array as aligned byte buffers, an empty then-branch inverted and a negated integer compare printed as its inverse. The oracle (ra-asm-emit/tests/oracle.rs) builds hello64, control64 and float64 with `clang -O1 -Wall -Werror` and runs them. control64 is 495 lines of C. The split landed 2026-11-11: `Unit::files` writes `program.c` alone up to 500 functions, else `program.h` (the externs, the prototypes, the structure types of the data and their `extern` declarations), `data.c` (the data definitions) and `program_NNN.c`; the function table names each function's file; `write_files` removes the parts of an earlier, larger output; the split oracle builds control64 as five files. Decided 2026-11-11 to need a clang front end (clang or clang-cl, which the pipeline picks) rather than port the output to cl: the runtime leans on 128-bit integers and vector types, cl has neither, and clang-cl builds against the same MSVC headers and libraries. The widths of task 25 and the known values that followed made the C read as source: control64 is 461 lines.
        waits-on: 27
    - 29 ‹task› C++ {c3} [defined]
        objective: The C output with references, classes where RTTI and vtables name them, and overloads where the mangled export names give them.
        acceptance: The C++ fixture builds and runs.
        waits-on: 28
    - 30 ‹task› Rust {c4} [defined]
        objective: Unsafe Rust: one module per source range, `extern "system"` declarations for the imports, labeled loops from the structuring pass, and a `rhipper` runtime crate.
        acceptance: The x64 fixtures build with cargo and pass their expected output.
        waits-on: 27
    - 31 ‹task› The function table {c2} [implemented]
        objective: `ras emit --out` writes `functions.json`, one entry per function the analysis found: `addr` (`0x` and at least eight hex digits), `name`, `size`, `file` and `line` of its definition in the written source, `status` (`lifted`, `partial` with a `traps` count, `thunk`, `not lifted`), `calls` and `imports`. The workbench lists, opens and grades functions from this one file; nothing is written in the shapes of the old pipeline.
        acceptance: The table names every function of a fixture, each `line` is that function's definition line, and the workbench's function views read it.
        notes: Decided 2026-11-10 to drop the corpus and fidelity shims: the corpus held Ghidra's decompilation beside the translation, and with one engine the two are the same C; the fidelity score graded a lossy translation by its call lists, and the lift is exact by construction with the oracle as the proof, so the honest grade is the trap count, the build and the run. Landed the same day (ra-asm-emit/src/functions.rs): the C emitter returns the unit in pieces with each function's definition line and trap count; the table joins that with the analysis. The analysis names an unnamed function `F_<address>`, the same as the lift and the C, and the `FUN_`/`GEN_`/`/* ref */` conventions are gone from the engine. The workbench side is phase 1 of `docs/rhipper/rhipper-asm.md` in rhipper-ide: a `RasFunctions` reader replaces the corpus, fidelity and gen-index readers.
        waits-on: 28
  - 32 ‹milestone› G: the pipeline and the workbench {c4} [in_progress]
      objective: `ras` is the engine of the Rhipper Workbench: a pipeline with steps, a JSON interface the IDE calls, and a managed install from releases; Ghidra, the JDK and Python leave the tool list.
      acceptance: The workbench runs the pipeline on a project with only `ras`, clang and CMake installed.
      notes: Started 2026-11-10 with the pipeline, ahead of the backends' C++ and Rust tasks, because the workbench sweep (no Ghidra, no Python) needs an engine to call first.
    - 33 ‹task› The pipeline steps {c3} [implemented]
        objective: `ras pipeline --from <step>` runs analyze, lift, emit, build and run, writes `build/pipeline/<step>.log` and the `[  12s] step` progress lines the IDE's status reader parses, and reads its settings from `rhipper.json`.
        acceptance: The IDE's step buttons drive `ras` on a project whose `rhipper.json` names the reference binary.
        notes: Landed 2026-11-10 (apps/ras/src/pipeline.rs, project.rs). `rhipper.json` keeps `name`, `exe`, `gen`, `sources`, `build` (dir, config, target) and `run` (exe, cwd, args, with, timeout); the Ghidra section, the Rhipper home, the architecture, the image base and the translator arguments are gone, since the engine reads the image. `emit` writes `program.c`, `image.h` and the runtime (`rhipper.h`, `driver.c`, embedded in `ras`) under `gen`, and `functions.json` under `build/`; `lift` writes `build/program.ll`. `build` writes a `CMakeLists.txt` for a project with none (gen/*.c plus the driver, clang flags, lld on Windows) and configures a new build directory with Ninja and clang when both are on the PATH, else Visual Studio with clang-cl; the translation needs a clang front end until the MSVC work of task 28. `run` starts the `with` side processes, runs the port with the `timeout`, and appends `runs.jsonl` (when, commit, dirty_gen, secs, alive, exit, fault, with the Windows status and the runtime's own exit codes named). The integration test (apps/ras/tests/pipeline.rs) runs the whole pipeline on control64 and checks the port's output. A canonical Windows path (`\\?\`) broke cmd.exe as a working directory: the project root is absolute, not canonical.
    - 34 ‹task› `ras serve` {c3} [defined]
        objective: JSON-RPC over stdio: project, functions query, function detail (bytes, instructions, IR, source), callers, xrefs, strings; the IDE and its MCP tools call it instead of Python scripts.
        acceptance: The MCP tools of the workbench answer from `ras serve` on a project with no Python installed.
        notes: Until it exists the workbench calls the subcommands with `--json`: `functions`, `cfg`, `disasm`, `callers` and `xrefs` (the last two added 2026-11-10: every call, jump, address, memory or immediate reference and every relocated data pointer into a range). The workbench sweep of 2026-11-10 (rhipper-ide) removed Ghidra, Python, the JDK and the Python Rhipper from the tools, the steps, the readers, the views and the MCP tools: `RasFunctions` reads `functions.json`, `RhipperCommandLines` runs `ras pipeline`, the Build tool window parses the clang diagnostics of the build step, and the MCP tools are `rhipper_project`, `run_step`, `stop_step`, `function`, `query_functions`, `lift_summary`, `clangd_diagnostics`, `port_runs`, `step_log`, `disassemble`, `cfg`, `callers`, `xrefs`, `ras` and `open`.
        waits-on: 33
    - 35 ‹task› The managed install {c2} [in_progress]
        objective: The workbench downloads `ras` from this repository's releases into its tools directory, as it did Rhipper, and a CI workflow here builds and publishes the release.
        acceptance: A fresh workbench installs `ras` on first start and shows its version under Tools.
        notes: The workbench side landed 2026-11-10 (`RhipperTools.RAS` in rhipper-ide): it reads `<releases>/latest/download/version.txt`, downloads `ras-<version>-<os>-<arch>.zip` (`windows`/`macos`/`linux`, `x64`/`arm64`) with its `.sha256`, unpacks it to `<tools>/ras/<version>/ras.exe`, and reads `ras --version` for the version it shows. The release workflow here must publish those three files under the tag `v<version>`; it does not exist yet.
        waits-on: 33
    - 36 ‹task› The facts of the old pipeline as analyses {c3} [defined]
        objective: What the Python extractors produced as facts and `ras` does not have yet (vtables and the class names RTTI gives them, function pointers in data, pointer globals, float constants, string literals) becomes an analysis of the core, in the function table and over `ras serve`, with the results cached under `build/`.
        acceptance: `ras functions --json` names the classes and the virtual functions of a C++ fixture, and the string literals it references.
        notes: Rewritten 2026-11-10: the 32-bit part moved to milestone I. Prototypes, no-return and switches already are analyses of the core.
        waits-on: 34
  - 40 ‹milestone› I: 32-bit x86 {c4} [defined]
      objective: The lift, the raising passes and the C backend handle 32-bit x86 PE programs, so the old pipeline's test programs and projects port through `ras`.
      acceptance: The x86 fixtures pass the execution oracle at every level, and a test program of the old pipeline builds and prints what the reference prints.
      notes: Placed before milestone H on 2026-11-10 (project decision 7). The loader, the decoder and the analysis handle PE32 already; the lift and the raise do not.
    - 41 ‹task› The 32-bit lift {c4} [defined]
        objective: The machine model in 32 bits: 32-bit pointers and `esp`, pushes as arguments, the x87 stack for floats, `fs` as the TEB, and the import adapters and trampolines of the runtime in 32-bit form (a port of a 32-bit program is still an x64 executable, so the runtime translates the pointer width at the import boundary).
        acceptance: hello32, control32 and a float fixture with x87 run lifted and print their native output.
        waits-on: 20
    - 42 ‹task› The x86 calling conventions {c3} [defined]
        objective: `cdecl`, `stdcall`, `fastcall` and `thiscall` by the stack slots and registers a function reads before writing and by who pops the arguments; an import arity table for the Win32 API so an import call takes the right number of stack arguments.
        acceptance: `ras signatures` on the x86 fixtures matches their sources, and the oracle passes at the ABI level.
        waits-on: 41 24
    - 43 ‹task› The old test programs {c3} [defined]
        objective: The test programs of the old pipeline (its rhtest sources, built with its MSVC toolsets) become fixtures here, with their expected outputs, and port through `ras pipeline`.
        acceptance: Each program builds from its C and prints what the reference prints; the workbench opens its project again.
        waits-on: 42 28
  - 37 ‹milestone› H: other architectures and formats {c4} [defined]
      objective: The `Arch` trait and the loader take a second architecture and a second container without changes to the IR, the raising passes or the backends.
      acceptance: An ARM64 PE lifts to IR that passes clang, and an ELF x86-64 file loads and disassembles.
    - 38 ‹task› ARM64 PE {c4} [defined]
        objective: An `Arch` implementation for AArch64 over a maintained decoder crate, the ARM64 exception table, and the lift of the integer subset.
        acceptance: A clang-built ARM64 fixture lifts and passes the IR gate.
    - 39 ‹task› ELF and Mach-O {c3} [defined]
        objective: `ra-asm-format` grows the ELF and Mach-O containers behind the same `Image` trait.
        acceptance: `ras info` prints a Linux x86-64 binary's sections and symbols.
  - 44 ‹milestone› J: from a correct port to a readable one {c5} [defined]
      objective: The depth that makes the C read as source: types, names, idioms, a hand-port takeover per function, exceptions, resources, and a trace of the reference beside the port. Each task is a nice-to-have until the one before it is solid; the oracle and the fixtures grow with each.
      acceptance: A real program of a few thousand functions ports with its structures, its names and its strings, and a person can take over one function at a time without touching the rest.
      notes: Defined 2026-11-10 as the vision after milestones E to I; the order below is the suggested priority, and a task may move up when a program needs it.
    - 45 ‹task› Width and type recovery {c4} [defined]
        objective: Beyond the width narrowing of task 25: pointers typed by what they point to, structure fields from the offsets a base pointer is accessed at, arrays from indexed accesses, `bool` for values only compared and branched on, string literals as `const char *`, and the types of a PDB when the binary names one.
        acceptance: A fixture with a struct of mixed fields reads as a `struct` with named fields in the C, and the oracle stays green.
        waits-on: 25
    - 46 ‹task› Names from the binary {c3} [defined]
        objective: Names from RTTI (classes, vtables), from a PDB when present, from the exports and from the CRT (a statically linked C runtime recognized by its code, so `printf` reads as `printf` and its body is not ported), and the Windows API prototype table generated from the SDK headers instead of written by hand.
        acceptance: A C++ fixture's classes and virtual functions carry their names into the C; a program with a static CRT ports with the CRT called, not lifted.
        waits-on: 36
    - 47 ‹task› Idioms and loop forms {c3} [defined]
        objective: The compiler's output read back as its source: `memcpy`, `memset` and `strlen` loops, division and modulo by a constant through multiplication, rotates and byte swaps, min and max through cmov, `for` loops with an induction variable, and a `switch` over a string table.
        acceptance: The control fixture's loops read as `for` loops and its idioms as calls, with no change to the oracle.
        waits-on: 45
    - 48 ‹task› The hand-port takeover {c4} [defined]
        objective: A port is something a person changes: `rhipper.json` (or a marker in the source) names the functions the hand-written sources own, `ras emit` writes them as declarations instead of definitions, the function table marks them, and the workbench shows and toggles the ownership per function.
        acceptance: A fixture with one function taken over by hand builds, runs, and prints what the reference prints; the table says which function is hand-written.
        waits-on: 31
    - 49 ‹task› Exceptions and unwinding {c3} [defined]
        objective: SEH and C++ exception handling from the x64 unwind data and handler tables: `__try` and `__except` regions, C++ landing pads and the cleanup of locals, so a program that throws ports instead of trapping.
        acceptance: A fixture that throws and catches a C++ exception runs ported and prints what the reference prints.
        waits-on: 24
    - 50 ‹task› Resources and typed data {c2} [defined]
        objective: PE resources (strings, dialogs, icons, manifests) extracted to files the port loads, and data sections written as typed arrays and strings where their uses say so, instead of byte blobs.
        acceptance: A fixture with a string table resource ports with the strings in a file, and its data reads as arrays in the C.
        waits-on: 45
    - 51 ‹task› The trace beside the port {c4} [defined]
        objective: A recorded trace of the reference (calls, returns, and the values at each) and the same trace of the port, compared to the first point where they part; the basis of the workbench's side-by-side stepping.
        acceptance: A port with one wrong function is found by the trace comparison in one run, and the workbench opens that function.
        waits-on: 34
    - 52 ‹task› Incremental and parallel {c3} [defined]
        objective: The analysis cached under `build/`, the lift and the emit of only the functions a change touched, and the lift of independent functions in parallel, so a program of tens of thousands of functions turns around in seconds.
        acceptance: A second `ras pipeline` run on an unchanged program finishes in under a second per step, and a large program lifts on every core.
        waits-on: 34
    - 53 ‹task› Packed and protected binaries {c2} [defined]
        objective: Recognize the common packers by their entry section and import table, and dump the unpacked image through an emulated run to the original entry point, so the pipeline starts from the real program.
        acceptance: A UPX-packed fixture ports as its unpacked program.
        waits-on: 41
