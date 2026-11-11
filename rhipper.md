# Rhipper Asm

## Gambits

- g1 ‹gambit› Exact or it stops: the lift traps rather than guesses, and every oracle stays green through every change {w5} [active]
    focus: lift raise oracle fixture
- g2 ‹gambit› Local work first: everything runs and is proven on this machine with clang, no service and no network {w4} [active]
    focus: oracle pipeline runtime
- g3 ‹gambit› The old pipeline's test programs port before any new architecture or readability work {w4} [active]
    focus: rhtest x86 32-bit CRT
- g4 ‹gambit› No compatibility with Ghidra or the Python pipeline; the engine writes its own shapes and the workbench follows {w3} [active]
    focus: workbench serve functions.json rhipper-ide
- g5 ‹gambit› Readability only on top of a correct port: types, names and idioms after the oracle, never instead of it {w3} [active]
    focus: idioms types names structuring

- 1 ‹project› Rhipper Asm {c5} [in_progress]
    objective: A pure-Rust disassembly and lifting core that replaces Ghidra and the Python Rhipper in the Rhipper Workbench: a PE image in, LLVM IR out, and C, C++ or Rust source out of that IR.
    acceptance: The workbench runs a project from the reference binary to a built and running port with no Ghidra, no JDK and no Python installed.
    notes: Decisions, 2026-11-10. (1) The decoder is the iced-x86 crate behind the crate's own `Instr` record, so the work goes into the lift and the backends, not into opcode tables; a second architecture plugs in behind the same trait. (2) The IR is a subset of LLVM IR with its own printer: every `.ll` the core writes must pass `clang -c -x ir`, which is the syntax and type gate, and clang is also the way to run lifted code in tests. (3) The first target is x86-64 PE; the loader and the decoder handle PE32 and 32-bit code from the start because the old pipeline's test programs are 32-bit. (4) Milestones run in order A to H below; a later milestone may start when its inputs exist. (5) No compatibility with the old pipeline's files or conventions (Ghidra's corpus and names, the fidelity scores, the gen-index comments): the engine writes its own shapes and the workbench's readers of the old ones are deleted as the new ones land, decided 2026-11-10; Ghidra-based projects stop opening in the workbench until `ras` lifts 32-bit programs. (6) The lift is exact or it stops: an instruction the lift does not cover becomes a trap that names its address, never a guess, so the grade of a function is its trap count and the oracle is the proof; readability work (widths, types, idioms) must keep the oracle green. (7) 32-bit x86 is its own milestone (I), ahead of ARM64 and ELF (H), because it is what lets the old test programs and projects port through `ras`. (8) 2026-11-11: the port of a 32-bit program is a 64-bit executable linked without large-address awareness that keeps the program's 32-bit pointers; see milestone I. Vision, 2026-11-10: milestones A to D are done and E to G are in progress; the engine lifts, raises and writes x64 programs to C that builds and runs, the workbench runs it end to end, and nothing of Ghidra or Python remains. What follows, in order: the readability that is still mechanical (widths, then the C++ and Rust printers over the same structured IR), the engine as a service and as a release, 32-bit x86, and then the depth that turns a correct port into a readable one: types from access patterns and debug info, names from RTTI and the CRT, idioms, a hand-port takeover per function, exceptions, resources, and a side-by-side trace of the reference and the port (milestone J). Each of those is a nice-to-have until the one before it is solid, and the oracle and the fixtures grow with each. Converted to the Rhipper Agent card spec 2026-11-11: the gambits above order the walk, every card names its files, the reviewed cards are accepted, and the engine's workers run from `rhipper.context.md`.
  - 2 ‹milestone› A: the image {c3} [accepted]
      objective: `ra-asm-pe` maps a PE file to an image by RVA and reads every structure the later steps need: headers, sections, imports, exports, relocations, the exception table, TLS, and the debug directory.
      acceptance: `ras info`, `ras sections`, `ras imports` and `ras exports` print the fixtures correctly, and the loader tests agree with `llvm-readobj` on every fixture.
    - 3 ‹section› Readers {c1} [accepted]
        objective: `ra-asm-format` holds a little-endian cursor whose every refusal names the offset and the field.
        acceptance: A truncated header fails with a message that names the offset; the cursor tests pass.
        files: crates/ra-asm-format/src/reader.rs
    - 4 ‹section› Headers, sections and the flat image {c2} [accepted]
        objective: The DOS, COFF and optional headers (PE32 and PE32+), the data directories and the section table; an image laid out by RVA with RVA, VA and file-offset conversions and bounded reads.
        acceptance: `ras info` prints machine, subsystem, image base, entry point and the section table of each fixture; a read past a section fails with the address in the message.
        files: crates/ra-asm-pe/src/headers.rs crates/ra-asm-pe/src/image.rs crates/ra-asm-pe/src/section.rs
    - 5 ‹section› Directories {c3} [accepted]
        objective: The import table (names, ordinals, IAT slots), the export table (names, ordinals, forwarders), base relocations, the x64 exception table with its unwind info and chained entries, the TLS directory with its callbacks, and the CodeView record of the debug directory.
        acceptance: `ras imports` and `ras exports` agree with `llvm-readobj --coff-imports --coff-exports`; `ras functions --pdata` lists every RUNTIME_FUNCTION of the x64 fixtures.
        files: crates/ra-asm-pe/src/imports.rs crates/ra-asm-pe/src/exports.rs crates/ra-asm-pe/src/relocs.rs crates/ra-asm-pe/src/exception.rs crates/ra-asm-pe/src/tls.rs crates/ra-asm-pe/src/debug.rs
    - 6 ‹section› Fixtures and the oracle {c2} [accepted]
        objective: `testdata/` holds small C programs, built by clang for x86-64 and x86 with lld, and the binaries they produce; a test compares the loader with `llvm-readobj` when it is on the PATH and skips when it is not.
        acceptance: `cargo test -p ra-asm-pe` passes with and without LLVM on the PATH; `testdata/build.ps1` rebuilds every fixture.
        notes: Done 2026-11-10. Six fixtures (hello, control, exports; 64 and 32), 2 to 6 KB each, no C runtime; -ffreestanding drops the x64 unwind tables, so the build uses -fno-builtin -funwind-tables. The native output of each program is kept under testdata/expected for the execution oracle.
        files: testdata/build.ps1 testdata/README.md crates/ra-asm-pe/tests/fixtures.rs
  - 7 ‹milestone› B: the instructions {c3} [accepted]
      objective: `ra-asm-x86` turns the bytes of a function into `Instr` records with operands, length, flow kind, and the registers and memory each one reads and writes, in 64-bit and 32-bit mode.
      acceptance: `ras disasm <exe> <address> [length]` prints a function; a test decodes every fixture's `.text` and agrees with `llvm-objdump` on each instruction's length and mnemonic.
    - 8 ‹task› The decoder adapter {c2} [accepted]
        objective: An `Arch` trait with an x86 implementation over iced-x86: decode one instruction, give its text, its flow (fall through, jump, conditional jump, call, return, indirect, trap), its branch target, and its register and memory accesses.
        acceptance: A unit test decodes a known byte sequence and checks the operands, the flow and the target.
        notes: Done 2026-11-10. The neutral record and the Arch trait live in a small ra-asm-arch crate, so a second architecture and the analysis share them without depending on x86. X86::decode_full keeps iced's Instruction beside the record for the lift.
        files: crates/ra-asm-arch/src/lib.rs crates/ra-asm-x86/src/decode.rs crates/ra-asm-x86/src/registers.rs
    - 9 ‹task› `ras disasm` {c1} [accepted]
        objective: The command prints `address  mnemonic operands`, with a note for a RIP-relative or absolute target inside the image, and the C string there when there is one, as the old `disasm.py` did.
        acceptance: The command on a fixture's entry point prints the whole function by its `.pdata` extent.
        files: apps/ras/src/disasm.rs
        waits-on: 8
    - 10 ‹task› The objdump oracle {c2} [accepted]
        objective: A test disassembles the `.text` of every fixture linearly and compares lengths and mnemonics with `llvm-objdump -d`; a difference prints both sides.
        acceptance: The test passes on every fixture, and skips with a message when `llvm-objdump` is not on the PATH.
        files: crates/ra-asm-x86/tests/objdump.rs
        waits-on: 8
  - 11 ‹milestone› C: functions and control flow {c4} [accepted]
      objective: `ra-asm-analysis` finds the functions of an image and builds the control-flow graph of each one.
      acceptance: `ras functions` lists every function of the fixtures with its size and block count, and finds the same functions as the link map of each fixture.
    - 12 ‹task› Function discovery {c3} [accepted]
        objective: Entry point, exports, TLS callbacks, every `.pdata` entry (x64), then recursive descent through direct calls; import thunks (`jmp [iat]`) become named thunks, not functions; chained unwind entries join their parent.
        acceptance: On the x64 fixtures the function list matches the lld map file; on the x86 fixtures every function the map names is found.
        notes: Done 2026-11-10. A fixpoint: seeds, descent, a call target inside a built function splits it, a function proven no-return makes its callers rebuild. The analysis finds exactly the map's functions on all four executables. Pointer seeds run last so that x86 jump-table entries are owned by their function first.
        files: crates/ra-asm-analysis/src/lib.rs crates/ra-asm-analysis/src/function.rs crates/ra-asm-analysis/tests/fixtures.rs
    - 13 ‹task› Basic blocks and the graph {c3} [accepted]
        objective: Each function splits into blocks at jump targets and after control transfers; each block names its successors; a `.pdata` extent bounds a function and a jump out of it is a tail call.
        acceptance: `ras cfg <address>` prints the blocks and edges of a function; a test checks a function with a loop and an if-else.
        files: crates/ra-asm-analysis/src/builder.rs crates/ra-asm-analysis/src/function.rs
        waits-on: 12
    - 14 ‹task› Jump tables {c3} [accepted]
        objective: The MSVC and clang x64 switch forms (`lea base; mov/movsxd index,[base+idx*4]; add; jmp`) and the x86 form (`jmp [table+idx*4]`) resolve to their targets, bounded by the compare before them and by the function extent.
        acceptance: A fixture with a dense switch has every case block reachable in `ras cfg`.
        notes: Done 2026-11-10 for the clang x64 form (table-relative entries), the MSVC x64 form (RVA entries) and the x86 absolute form. The MSVC two-level form (a byte or word index table before the address table, `movzx idx2, byte ptr [idx + cases]; jmp [idx2*4 + table]`) landed 2026-11-11 with the first MSVC fixture, composed by the original index so the switch's cases are the program's.
        files: crates/ra-asm-analysis/src/jumptable.rs
        waits-on: 13
    - 15 ‹task› Functions that never return {c2} [accepted]
        objective: Known imports (`ExitProcess`, `exit`, `abort`, `_exit`, `__fastfail`, and the CRT's invalid-parameter handlers) and any function whose every path ends in one are marked no-return, so the bytes after a call to them start a new block or function.
        acceptance: A fixture calling `ExitProcess` in the middle of `.text` decodes the code after the call as its own function.
        notes: `_cexit` and `_c_exit` return (they run the exit tables and come back); they left the list 2026-11-11 when the CRT startup of an MSVC program lost the code after them.
        files: crates/ra-asm-analysis/src/noreturn.rs
        waits-on: 12
  - 16 ‹milestone› D: the IR and the lift {c5} [accepted]
      objective: `ra-asm-ir` is an LLVM-IR-shaped SSA IR with a `.ll` printer, and `ra-asm-lift` turns each function's instructions into it with the machine state explicit.
      acceptance: `ras lift <exe> --ll out.ll` writes a module that `clang -c -x ir` accepts for every fixture, and the lifted fixtures print what the references print when run through the test harness.
    - 17 ‹task› The IR {c3} [accepted]
        objective: Types (`i1` to `i128`, `half` to `double`, vectors, `ptr`), values, the instruction subset (alloca, load, store, the integer and float binops, icmp, fcmp, select, br, switch, call, ret, unreachable, the casts, phi, a few intrinsics), blocks, functions, globals and declarations, with an address attached to each instruction for the backends and the IDE.
        acceptance: The printer round-trips a hand-built module through `clang -c -x ir` without error; `cargo test -p ra-asm-ir` passes.
        files: crates/ra-asm-ir/src/types.rs crates/ra-asm-ir/src/inst.rs crates/ra-asm-ir/src/module.rs crates/ra-asm-ir/src/builder.rs crates/ra-asm-ir/src/printer.rs crates/ra-asm-ir/tests/clang.rs
    - 18 ‹task› The machine-state lift {c4} [accepted]
        objective: Each native function becomes an IR function over a `%State` pointer: registers are fields, flags are fields computed by each instruction that defines them, memory goes through `inttoptr` into the flat image, a direct call calls the lifted callee, an import calls a declared external through an adapter, and an indirect call goes through a resolver.
        acceptance: `ras lift` on the x64 fixtures passes clang; the integer subset (moves, arithmetic, logic, shifts, compares, setcc, cmovcc, jumps, calls, push and pop, lea, the widening moves, multiply and divide, the string moves) is covered.
        files: crates/ra-asm-lift/src/x86.rs crates/ra-asm-lift/src/state.rs crates/ra-asm-lift/src/lib.rs
        waits-on: 17 13
    - 19 ‹task› The execution oracle {c3} [accepted]
        objective: A test lifts a fixture, compiles the `.ll` with clang together with a C driver that maps the image and calls the lifted entry, runs it, and compares the output with the reference's.
        acceptance: The x64 fixtures print the same lines lifted as native.
        notes: Done 2026-11-10. hello64 and control64 run lifted and print their native output. The driver (runtime/driver.c) maps the image at its base, binds the IAT through GetProcAddress, gives rsp a 1 MiB stack and calls F_<entry>. Imports go through an eight-integer adapter; an indirect call goes through ra_call, which finds the lifted function or calls native code. Limits recorded as tasks: 32-bit import calls need an arity table; a 64-bit division whose dividend does not fit 64 bits traps; fs and gs reads are flat.
        files: crates/ra-asm-lift/tests/oracle.rs runtime/driver.c runtime/rhipper.h
        waits-on: 18
    - 20 ‹task› SSE and the rest of the subset {c3} [accepted]
        objective: Scalar SSE arithmetic and conversions, packed moves and zeroing idioms, bit tests, rotates, adc and sbb, bswap, xchg and cmpxchg, the no-ops and traps.
        acceptance: A float fixture passes the execution oracle.
        notes: Done 2026-11-10. testdata/src/float.c (scalar arithmetic in both widths, conversions both ways, NaN and infinity compares, a struct of floats) runs lifted and matches. The packed float operations, the compares with a predicate, movmskps/pd, the packed conversions and the shuffles are lifted too; x87 is not, and comes with a 32-bit fixture that uses it.
        files: crates/ra-asm-lift/src/x86.rs testdata/src/float.c
        waits-on: 19
  - 21 ‹milestone› E: raising {c5} [accepted]
      objective: Passes turn the machine-state IR into source-shaped IR: stack slots become locals, state fields become SSA values, flags become comparisons, calling conventions become signatures, and data references become typed globals.
      acceptance: The raised IR of the fixtures still passes the execution oracle and is a fraction of the machine IR's size.
    - 22 ‹task› State promotion and dead flags {c3} [accepted]
        objective: Loads and stores of state fields within a function become SSA values with phis; flag definitions that no use reaches disappear.
        acceptance: No `%State` field load or store remains in a leaf function with no calls; the oracle still passes.
        notes: Done 2026-11-10. ra-asm-lift/src/raise: promote (liveness-guided SSA construction over the register and flag slots; a call flushes the dirty registers and forgets them; the flags are dead at calls and returns), simplify (identities and constant folding), dce (unused pure values, trivial phis). The oracle runs both the machine and the raised IR of every fixture. Lines of IR: hello64 634 to 206, control64 4975 to 2072, float64 2722 to 1825. The vector registers stay in memory until the type-aware promotion. A leaf function still loads its live-in registers and stores its results: that is the signature pass's job (task 24).
        files: crates/ra-asm-lift/src/raise/promote.rs crates/ra-asm-lift/src/raise/simplify.rs crates/ra-asm-lift/src/raise/dce.rs crates/ra-asm-lift/src/raise/liveness.rs
    - 23 ‹task› Stack frames and locals {c4} [accepted]
        objective: `rsp`- and `rbp`-relative slots become allocas sized by their accesses; the frame setup and teardown disappear.
        acceptance: A fixture function with locals shows them as allocas with no stack pointer arithmetic.
        notes: Done 2026-11-10. raise/frame.rs follows the stack pointer arithmetic from the frame top: a pointer at a constant offset becomes a gep from the frame, and a slot that is loaded and stored at one type, whose address neither escapes to a call nor flows into dynamic pointer arithmetic, becomes its own alloca; a slot at or above an escaped address stays in the frame, since the callee may reach it through the pointer. The ABI rewrite drops the pushed return address, and a store of undef (a saved register) goes. A leaf function with no locals has no frame at all: op_div is a null check and a division, fib a loop with two phis and a call. Remaining for later: promoting the private allocas to SSA (LLVM's mem2reg does it today), and narrowing the i64 register values to the width their uses have.
        files: crates/ra-asm-lift/src/raise/frame.rs crates/ra-asm-lift/src/raise/stack.rs
        waits-on: 22
    - 24 ‹task› Calling conventions and signatures {c4} [accepted]
        objective: Win64 (`rcx rdx r8 r9`, `xmm0`-`xmm3`, the stack, `rax` and `xmm0` out) and the x86 conventions (`cdecl`, `stdcall`, `fastcall`, `thiscall`) by the registers and slots a function reads before writing and writes before returning; imports take their prototypes from a table.
        acceptance: Each fixture function gets the parameter count of its source; the oracle calls raised functions by their signatures.
        notes: Inference done 2026-11-10 (raise/abi.rs, liveness.rs, stack.rs, prototypes.rs; `ras signatures`). The parameters are the argument registers read before written under a call model where a call reads its callee's parameters (iterated to a fixpoint) and writes the caller-saved registers; imports read what a prototype table says, unknown ones all four; the stack arguments come from a symbolic stack-pointer analysis that also sizes the frame. A root returns a value when it writes rax; the rest when a caller reads rax after the call. The lift now zeroes a register on `xor r, r` and `pxor x, x` without reading it, and calls imports through per-import wrappers, so neither shows as a read. The signatures of control64 and float64 match the binaries (clang folded constant arguments into collatz, mean and mix). The rewrite landed the same day: raise/promote.rs in ABI mode gives each function its parameters, a frame alloca whose top is the entry stack pointer (8 past a 16-byte boundary, as after a native call), calls with arguments and results, float arguments and results through a local vector array, imports through their adapters, indirect calls through the pointer itself, and a returned value; no %State remains. runtime/driver.c installs a 12-byte trampoline at each lifted function's image address, so a pointer into the image reaches the host function. The oracle runs every fixture at the machine, promoted and ABI levels. Lines of IR at the ABI level: hello64 173, control64 1159, float64 1568. The stack frame is still addressed through inttoptr of the frame top: task 23 turns the slots into allocas.
        files: crates/ra-asm-lift/src/raise/abi.rs crates/ra-asm-lift/src/raise/promote.rs crates/ra-asm-lift/src/raise/prototypes.rs crates/ra-asm-lift/tests/signatures.rs apps/ras/src/signatures.rs
        waits-on: 23
    - 25 ‹task› Conditions, globals and types {c4} [accepted]
        objective: `cmp` and `test` followed by a conditional become one comparison; references into data sections become globals of the width their uses say; integer widths, pointers and floats come from the uses.
        acceptance: A raised fixture reads as C-shaped IR with named globals and `icmp`-driven branches.
        notes: The conditions landed 2026-11-10 (raise/conditions.rs): the sf, of, zf and cf formulas of a subtraction fold back into one icmp of the operands, with the inverse and the le/be combinations; simplify also folds trunc of zext and per-block common subexpressions. Lines of IR at the ABI level: hello64 132, control64 665 (from 4975 machine), float64 945. The globals landed next (ra-asm-lift/src/data.rs): in standalone mode each data section is one global, a packed structure of byte runs and address words (a relocated word is the address of the function or the data byte it names; an import slot is the import's own address, bound at link time); a RIP-relative or absolute reference becomes the address of a byte of a global; a switch jumps by its index register, read before the table entry, so the table is never read. The ABI level is standalone: the driver maps nothing and binds nothing, and all three fixtures run that way. The widths landed 2026-11-11 (raise/widths.rs): a demand analysis finds the low bits each i64 value's uses read and a known-zero analysis how far up a value is zero; an extension, an addition and its kin, a phi, a select or a switch at a demand below 64 is defined again at that width, an unsigned compare of two values zero above the same width compares there, and the parameters and results of a function that no outside caller or pointer reaches narrow to what the body demands and the callers accept, as an optimistic fixpoint across the functions (a dispatcher that returns a callee's result as it is otherwise keeps both at 64). `fib` reads `uint32_t F(uint32_t)` with `uint32_t` phis, `op_div` divides `uint32_t`s. The C printer drops the cast around a 32-bit or 64-bit operation and prints a signed compare's constant as a number. Left for the idiom work of milestone J: the lift's own division-by-zero guard repeats the program's check (a known-conditions pass), and `cdq; idiv` reads as a shift-or instead of a sign extension.
        files: crates/ra-asm-lift/src/raise/conditions.rs crates/ra-asm-lift/src/raise/widths.rs crates/ra-asm-lift/src/raise/known.rs crates/ra-asm-lift/src/data.rs
        waits-on: 24
  - 26 ‹milestone› F: the backends {c5} [in_progress]
      objective: `ra-asm-emit` writes C, C++ or Rust source from the raised IR, with a runtime header per language, and the function table the workbench reads.
      acceptance: A fixture's C, C++ and Rust outputs each build in one round and print what the reference prints.
      notes: Started 2026-11-10 with the C backend; the three x64 fixtures build from their C with clang and print what the reference prints. By 2026-11-11 the C, the C++ dialect and the Rust output all pass the oracle; the C++ output of its own shape (task 29) waits for the names and vtables of task 46.
    - 27 ‹task› Structuring {c4} [accepted]
        objective: Reducible graphs become if, else, while and switch; a C or C++ output keeps gotos for the rest; the Rust output uses labeled loops and breaks, since Rust has no goto.
        acceptance: Every fixture function structures without a goto in C; the Rust output compiles.
        notes: Landed 2026-11-10 (ra-asm-emit/src/structure.rs), language-neutral: dominators and post-dominators give the join of each branch, natural loops give `for (;;)` with break and continue, a switch terminator gives a switch, and a block that only computes a condition folds into the branch above it as `&&` or `||`, so a short-circuit reads as one. Anything else falls back to a goto with a label, and the fixtures have none. The Rust half (labeled loops) waits for task 30.
        files: crates/ra-asm-emit/src/structure.rs crates/ra-asm-ir/src/dom.rs
    - 28 ‹task› C {c4} [accepted]
        objective: C that builds in one round with clang and clang-cl: typed locals, structured control flow, the data as globals, a `rhipper.h` runtime for the helpers, and a split into a header, a data file and one `.c` per 500 functions when a program is large.
        acceptance: The x64 fixtures compile with clang with no warning and pass their expected output, as one file and split.
        notes: `ras emit --lang c` writes `program.c` and `runtime/rhipper.h` holds the helpers (the 128-bit and vector types, the intrinsics, the trap and the call resolver). Landed 2026-11-10: typed locals from the IR, phis through `_next` temporaries, single-use values inlined into their expression, globals as packed structures, the frame and the xmm array as aligned byte buffers, an empty then-branch inverted and a negated integer compare printed as its inverse. The oracle (ra-asm-emit/tests/oracle.rs) builds hello64, control64 and float64 with `clang -O1 -Wall -Werror` and runs them. control64 is 495 lines of C. The split landed 2026-11-11: `Unit::files` writes `program.c` alone up to 500 functions, else `program.h` (the externs, the prototypes, the structure types of the data and their `extern` declarations), `data.c` (the data definitions) and `program_NNN.c`; the function table names each function's file; `write_files` removes the parts of an earlier, larger output; the split oracle builds control64 as five files. Decided 2026-11-11 to need a clang front end (clang or clang-cl, which the pipeline picks) rather than port the output to cl: the runtime leans on 128-bit integers and vector types, cl has neither, and clang-cl builds against the same MSVC headers and libraries. The widths of task 25 and the known values that followed made the C read as source: control64 is 461 lines.
        files: crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/src/common.rs runtime/rhipper.h crates/ra-asm-emit/tests/oracle.rs
        waits-on: 27
    - 29 ‹task› C++ {c3} [in_progress]
        objective: The C output with references, classes where RTTI and vtables name them, and overloads where the mangled export names give them.
        acceptance: `cargo test -p ra-asm-emit -q` passes with the classes fixture emitted as a `class` per RTTI class whose virtual functions are its methods, and its C++ port prints what the reference prints.
        notes: The dialect landed 2026-11-11: `ras emit --lang cpp`, or `"lang": "cpp"` in `rhipper.json`, writes the same source as `.cpp` files with the imports, the prototypes and the data under `extern "C"` (the driver is C, the imports are the DLLs' names, and a C++ `const` global would otherwise have internal linkage); clang++ builds control64 split and float64 whole beside the C driver, and the oracle checks both. The shape of its own (classes, references, overloads) waits for the names and vtables of task 46.
        files: crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/src/lib.rs crates/ra-asm-analysis/src/rtti.rs crates/ra-asm-emit/tests/oracle.rs
        waits-on: 28 46
        priority: 2
    - 30 ‹task› Rust {c4} [accepted]
        objective: Unsafe Rust: one module per source range, `extern "system"` declarations for the imports, the loops of the structuring pass, and a `rhipper` runtime module.
        acceptance: The x64 fixtures build with cargo and pass their expected output.
        notes: Landed 2026-11-11 (ra-asm-emit/src/rust.rs, runtime/rhipper.rs): `ras emit --lang rust`, or `"lang": "rust"`, writes a cargo package under `gen` (`Cargo.toml`, `src/main.rs` with `main`, `src/rhipper.rs`, `src/data.rs` with the imports by DLL and the data, `src/program.rs` or `src/program_NNN.rs`). Every lifted function is an `unsafe extern "C" fn`, so a pointer to it in the data calls it; memory goes through `read_unaligned` and `write_unaligned`; arithmetic wraps; an `i1` is a `bool`; a vector is an array and its operations go lane by lane through the runtime's `vbin`, `vcmp`, `vcast`, `shuffle` and `insert`; a packed data section is a `#[repr(C, packed)]` struct inside an aligned wrapper, and `ra_init` writes its address words at startup, since the compiler cannot fold a function address into a static. The pipeline builds a Rust project with cargo under `build/cargo` and runs the binary from there. The oracle builds hello64, control64 (split) and float64 with cargo and runs them; the pipeline test ports control64 as Rust. Rust has no `goto`: a jump the structuring cannot express stops the port, as a trap does, and the fixtures have none.
        files: crates/ra-asm-emit/src/rust.rs runtime/rhipper.rs crates/ra-asm-emit/tests/oracle.rs
        waits-on: 27
    - 31 ‹task› The function table {c2} [accepted]
        objective: `ras emit --out` writes `functions.json`, one entry per function the analysis found: `addr` (`0x` and at least eight hex digits), `name`, `size`, `file` and `line` of its definition in the written source, `status` (`lifted`, `partial` with a `traps` count, `thunk`, `not lifted`), `calls` and `imports`. The workbench lists, opens and grades functions from this one file; nothing is written in the shapes of the old pipeline.
        acceptance: The table names every function of a fixture, each `line` is that function's definition line, and the workbench's function views read it.
        notes: Decided 2026-11-10 to drop the corpus and fidelity shims: the corpus held Ghidra's decompilation beside the translation, and with one engine the two are the same C; the fidelity score graded a lossy translation by its call lists, and the lift is exact by construction with the oracle as the proof, so the honest grade is the trap count, the build and the run. Landed the same day (ra-asm-emit/src/functions.rs): the C emitter returns the unit in pieces with each function's definition line and trap count; the table joins that with the analysis. The analysis names an unnamed function `F_<address>`, the same as the lift and the C, and the `FUN_`/`GEN_`/`/* ref */` conventions are gone from the engine. The workbench side is phase 1 of `docs/rhipper/rhipper-asm.md` in rhipper-ide: a `RasFunctions` reader replaces the corpus, fidelity and gen-index readers.
        files: crates/ra-asm-emit/src/functions.rs apps/ras/src/emit.rs
        waits-on: 28
    - 54 ‹task› Return blocks duplicated in the structuring {c3} [defined]
        objective: A small block that several branches reach and that the structuring cannot place under one of them (a return block two exits of a loop both reach, as the `lock cmpxchg` loop of `__scrt_acquire_startup_lock` in every MSVC program) is duplicated into each predecessor instead of becoming a goto, up to a few instructions; a larger one stays a goto in C and still stops the Rust port, which names the function and the block.
        acceptance: `cargo test -p ra-asm-emit -q` passes with a structuring test of a loop with two exits to one return block that produces no goto, and the Rust port of `testdata/rhtest/core-O2.exe` builds once task 43 lands.
        files: crates/ra-asm-emit/src/structure.rs crates/ra-asm-emit/src/rust.rs crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/tests/oracle.rs
        waits-on: 27
        priority: 4
  - 32 ‹milestone› G: the pipeline and the workbench {c4} [in_progress]
      objective: `ras` is the engine of the Rhipper Workbench: a pipeline with steps, a JSON interface the IDE calls, and a managed install from releases; Ghidra, the JDK and Python leave the tool list.
      acceptance: The workbench runs the pipeline on a project with only `ras`, clang and CMake installed.
      notes: Started 2026-11-10 with the pipeline, ahead of the backends' C++ and Rust tasks, because the workbench sweep (no Ghidra, no Python) needs an engine to call first.
    - 33 ‹task› The pipeline steps {c3} [accepted]
        objective: `ras pipeline --from <step>` runs analyze, lift, emit, build and run, writes `build/pipeline/<step>.log` and the `[  12s] step` progress lines the IDE's status reader parses, and reads its settings from `rhipper.json`.
        acceptance: The IDE's step buttons drive `ras` on a project whose `rhipper.json` names the reference binary.
        notes: Landed 2026-11-10 (apps/ras/src/pipeline.rs, project.rs). `rhipper.json` keeps `name`, `exe`, `gen`, `sources`, `build` (dir, config, target) and `run` (exe, cwd, args, with, timeout); the Ghidra section, the Rhipper home, the architecture, the image base and the translator arguments are gone, since the engine reads the image. `emit` writes `program.c`, `image.h` and the runtime (`rhipper.h`, `driver.c`, embedded in `ras`) under `gen`, and `functions.json` under `build/`; `lift` writes `build/program.ll`. `build` writes a `CMakeLists.txt` for a project with none (gen/*.c plus the driver, clang flags, lld on Windows) and configures a new build directory with Ninja and clang when both are on the PATH, else Visual Studio with clang-cl; the translation needs a clang front end until the MSVC work of task 28. `run` starts the `with` side processes, runs the port with the `timeout`, and appends `runs.jsonl` (when, commit, dirty_gen, secs, alive, exit, fault, with the Windows status and the runtime's own exit codes named). The integration test (apps/ras/tests/pipeline.rs) runs the whole pipeline on control64 and checks the port's output. A canonical Windows path (`\\?\`) broke cmd.exe as a working directory: the project root is absolute, not canonical.
        files: apps/ras/src/pipeline.rs apps/ras/src/project.rs apps/ras/templates/CMakeLists.txt apps/ras/tests/pipeline.rs
    - 34 ‹task› `ras serve` {c3} [accepted]
        objective: JSON-RPC over stdio: project, functions query, function detail (bytes, instructions, IR, source), callers, xrefs, strings; the IDE and its MCP tools call it instead of Python scripts.
        acceptance: The MCP tools of the workbench answer from `ras serve` on a project with no Python installed.
        notes: Landed 2026-11-11 (apps/ras/src/serve.rs). JSON-RPC 2.0, one request and one response per line, the image and the analysis in memory for the life of the process; the methods are `project`, `info`, `functions`, `function` (the analysis, the `functions.json` entry, the source of the function cut from the translation, its IR cut from `program.ll`, and its instructions), `disasm`, `cfg`, `callers`, `xrefs`, `strings` and `shutdown`. `--project` serves a project, `--exe` a bare image. When the image changes on disk the server answers `-32000`, reloads, and the client asks again. The subcommands and the server share one set of value-returning functions (`info_json`, `functions_json`, `cfg_json`, `decode`/`disasm_json`/`disasm_text`, `callers_json`, `xrefs_json`), so `--json` and the server cannot drift. The workbench side landed the same day in rhipper-ide: `RasServer` (a project service that starts the server on the first question and restarts it when it ended or the `ras` path changed), `RasJsonRpc` (the codec, with tests), the disassembly view and the MCP tools `disassemble`, `cfg`, `callers`, `xrefs`, the new `strings`, and `function` with `disassembly`/`ir` on the server; only `rhipper_ras` and the pipeline steps still run `ras` as a process. Before this, the workbench called the subcommands with `--json`; the sweep of 2026-11-10 that removed Ghidra, Python and the JDK from rhipper-ide is described in phase 2 of `docs/rhipper/rhipper-asm.md` there.
        files: apps/ras/src/serve.rs apps/ras/src/report.rs apps/ras/src/functions.rs apps/ras/src/xrefs.rs apps/ras/tests/serve.rs
        waits-on: 33
    - 35 ‹task› The managed install {c2} [in_progress]
        objective: The workbench downloads `ras` from this repository's releases into its tools directory, as it did Rhipper, and a CI workflow here builds and publishes the release.
        acceptance: A fresh workbench installs `ras` on first start and shows its version under Tools.
        notes: The workbench side landed 2026-11-10 (`RhipperTools.RAS` in rhipper-ide): it reads `<releases>/latest/download/version.txt`, downloads `ras-<version>-<os>-<arch>.zip` (`windows`/`macos`/`linux`, `x64`/`arm64`) with its `.sha256`, unpacks it to `<tools>/ras/<version>/ras.exe`, and reads `ras --version` for the version it shows. The workflow landed 2026-11-11 (.github/workflows/release.yml): a tag `v<version>` that matches the workspace version runs the tests with LLVM installed, builds `ras` in release, packs `ras.exe` with the runtime files, and publishes `version.txt`, `ras-<version>-windows-x64.zip` and its `.sha256`. Left, and the person's to do: the repository has no remote yet; the workbench's `RAS_RELEASES` names `github.com/zeruth/rhipper-asm`, so the remote must be that or the constant must change, and the first tag proves the workflow.
        files: .github/workflows/release.yml Cargo.toml
        waits-on: 33
        priority: 2
    - 36 ‹task› The facts of the old pipeline as analyses {c3} [accepted]
        objective: What the Python extractors produced as facts and `ras` does not have yet (vtables and the class names RTTI gives them, function pointers in data, pointer globals, float constants, string literals) becomes an analysis of the core, in the function table and over `ras serve`, with the results cached under `build/`.
        acceptance: `ras functions --json` names the classes and the virtual functions of a C++ fixture, and the string literals it references.
        notes: Rewritten 2026-11-10: the 32-bit part moved to milestone I. Prototypes, no-return and switches already are analyses of the core. Landed 2026-11-11 (ra-asm-analysis/src/rtti.rs): the complete object locators of the Microsoft C++ ABI are found by their shape (x64: the self RVA; x86: a type descriptor whose name starts with `.?A`), each names its class and its bases through the type descriptors and the class hierarchy, and the vtable follows the slot that points at the locator. `Analysis.classes` lists them; every virtual function is seeded from its vtable (`Origin::Vtable`, a root of the lift like a pointer, so its signature keeps its result), carries its slots in `Function.virtuals`, and takes its slot's name when nothing else names it: `Square_vf2`, or `Rect_Named_vf0` for a vtable that implements a base's interface. `Function.strings` lists the string literals each function addresses. `ras functions --json` adds `strings`, `virtuals` and a top-level `classes`; `functions.json` adds `strings`, `class` and `slot`; `ras serve` answers `classes` and puts the facts in `function`. The fixture is `classes.cpp` (a base, a derived class, a second base, a class with two vtables, no C++ runtime), built for x64 and x86, and it passes the oracle at every level and as C and Rust. Function pointers in data were already seeds (`Origin::Pointer`); pointer globals and float constants are not facts the engine needs, since the lift reads the data itself: typed globals are task 45 and typed data task 50. Nothing is cached under `build/`: the analysis of a fixture takes milliseconds and `ras serve` keeps it in memory; a cache is for a program where that stops being true.
        files: crates/ra-asm-analysis/src/rtti.rs crates/ra-asm-analysis/src/lib.rs testdata/src/classes.cpp
        waits-on: 34
    - 59 ‹task› `ras doctor` {c1} [defined]
        objective: One command checks the tools a project needs on this machine (`clang`, `clang++`, `lld`, `cmake`, `ninja` or Visual Studio, `cargo` for a Rust port, `llvm-symbolizer` for a crash), prints each with its version or `missing` and what it is needed for, as text and as `--json`, and exits non-zero when a required one is absent; the pipeline's `build` step runs the same check first and fails with that message instead of CMake's.
        acceptance: `cargo test -p ras -q` passes with a test that parses the JSON of the command on this machine and finds clang in it.
        files: apps/ras/src/main.rs apps/ras/src/pipeline.rs apps/ras/tests/pipeline.rs
        waits-on: 33
        priority: 2
    - 60 ‹task› The coverage report {c2} [defined]
        objective: `ras lift --report` (and the same under `functions --json` as `coverage`) counts, over a whole program, the instructions the lift does not cover by mnemonic with one example address each, the functions by trap count, the imports without a prototype, and the indirect jumps that did not resolve, so the next lift work is chosen by what a real program needs rather than by a guess.
        acceptance: `cargo test -p ras -q` passes with a test that runs the report on a fixture and finds its tables, and the report on `testdata/rhtest/core-O2.exe` lists no uncovered instruction.
        files: apps/ras/src/lift.rs crates/ra-asm-lift/src/lib.rs apps/ras/src/functions.rs apps/ras/src/report.rs
        waits-on: 34
        priority: 3
    - 62 ‹task› The reference run beside the port {c2} [defined]
        objective: The pipeline's `run` step also runs the reference binary (`exe` of `rhipper.json`) with the same arguments, working directory and timeout when `run.compare` is on, and records both outputs' first difference and the exit codes in `runs.jsonl` and the log as a verdict (`same`, `differs at line N`, `port crashed`); the workbench shows the verdict on the run step.
        acceptance: `cargo test -p ras -q` passes with the pipeline test checking a `same` verdict on control64.
        files: apps/ras/src/pipeline.rs apps/ras/src/project.rs apps/ras/tests/pipeline.rs
        waits-on: 33
  - 40 ‹milestone› I: 32-bit x86 {c4} [in_progress]
      objective: The lift, the raising passes and the C backend handle 32-bit x86 PE programs, so the old pipeline's test programs and projects port through `ras`.
      acceptance: The x86 fixtures pass the execution oracle at every level, and a test program of the old pipeline builds and prints what the reference prints.
      notes: Placed before milestone H on 2026-11-10 (project decision 7). The loader, the decoder and the analysis handle PE32 already; the lift and the raise do not. Decision 8, 2026-11-11: the port of a 32-bit program is a 64-bit executable that keeps the program's 32-bit pointers. It is linked without large-address awareness, so the loader keeps every address of the process (heap, stack, environment block, kernel32) below 2 GB and a pointer truncated to 32 bits is still the pointer; its data words that hold addresses are written at startup, since no compiler folds a truncated address into a static initializer. The levels that map the image at its base (machine, promoted) run as plain processes instead, with the driver's stack placed low: the loader of a non-large-address-aware process puts its own heap and environment block at the image's base.
    - 41 ‹task› The 32-bit lift {c4} [accepted]
        objective: The machine model in 32 bits: 32-bit pointers and `esp`, pushes as arguments, the x87 stack for floats, `fs` as the TEB, and the import adapters and trampolines of the runtime in 32-bit form (a port of a 32-bit program is still an x64 executable, so the runtime translates the pointer width at the import boundary).
        acceptance: hello32, control32 and a float fixture with x87 run lifted and print their native output.
        notes: Landed 2026-11-11 (x86.rs, x87.rs, state.rs, driver.c). A 32-bit register write zero-extends in both modes, since nothing in 32-bit code reads the upper half. An import call reads its arguments from the stack through a wrapper of eight dwords, and a `stdcall` import pops them: the lift moves `esp` by the import's argument bytes from the prototype table after the call (a C runtime DLL is `cdecl`; an unknown import is taken as `cdecl` with a warning). The x87 unit is eight doubles in a global with a global top, control word and status word, which cross calls as `st(0)` does and survive the raising passes as plain memory: loads, stores, integer conversions by the control word's rounding mode, arithmetic, compares with their condition codes, and the control and status word moves; the 80-bit precision of the real unit is not kept. An `fs:` or `gs:` access lands in a zero thread environment block the runtime keeps. In standalone mode a relocated immediate (`push offset string`) or a relocated displacement (`[table + ecx*4]`) is the address of the data global, which the base relocations mark exactly. Machine memory accesses carry alignment 1, so a `movups` never becomes a `movaps` in the compiled port (this had bitten 64-bit code too in principle). The driver gives a 32-bit program a stack below 4 GB, binds 32-bit import slots, calls native code with dword arguments, and names the place of a crash as a module offset for llvm-symbolizer. Found along the way and fixed for both widths: `movsd` with an xmm operand was lifted as the string move when its destination was memory; `adc` and `sbb` named two values `cf.1`; the stack analysis moved the stack pointer after an import wrapper call, which pushed nothing; `roundeven` is `rint` throughout, since the C runtime lacks the former.
        files: crates/ra-asm-lift/src/x86.rs crates/ra-asm-lift/src/x87.rs crates/ra-asm-lift/src/state.rs runtime/driver.c crates/ra-asm-lift/tests/oracle.rs
        waits-on: 20
    - 42 ‹task› The x86 calling conventions {c3} [accepted]
        objective: `cdecl`, `stdcall`, `fastcall` and `thiscall` by the stack slots and registers a function reads before writing and by who pops the arguments; an import arity table for the Win32 API so an import call takes the right number of stack arguments.
        acceptance: `ras signatures` on the x86 fixtures matches their sources, and the oracle passes at the ABI level.
        notes: Landed 2026-11-11 (raise/mod.rs `Target`, abi.rs, stack.rs, promote.rs, stubs.rs, frame.rs). A `Target` carries the pointer width through the raise: the first stack argument (4 past the return address, against 0x28 on Win64), the argument registers (`ecx`, `edx`; the floats of a local function in `xmm0` to `xmm2`, which clang passes beside them, each class counted on its own), the callee-saved set, and the entry misalignment. The stack analysis follows the pointer through the truncation and widening of the 32-bit model, moves it by the return address and what a lifted callee pops after a call, and reads what the function itself pops from its offset at `ret`, so `Signature.pops` tells `stdcall` and `thiscall` from `cdecl`. A stack slot read as 64 bits is one wide parameter over two dwords; a 64-bit result comes back in `edx:eax` when the callee writes `edx` and a caller reads it; a float result in `xmm0` when a caller reads it; a double on the x87 stack travels in the globals, outside the signature. Every address-taken function gets a universal entry point `U_<address>` with the universal signature, six stack dwords then `ecx` and `edx`, that calls the function with the arguments its signature places, writes what it pops to `ra_pops`, and returns the result; the data and the function table point at the stub, a call through a pointer passes the universal arguments and moves the stack pointer by the return address plus `ra_pops`, and the stack of a function with such a call is dynamic from there, with 4 KB of slack below its frame. The prologue's stack-parameter stores are slots of the frame pass, which they were not (a latent bug for Win64 functions with stack parameters). An unknown x86 callee reads only `ecx`, the `this` of a `thiscall` method. The C backend prints every machine access through an unaligned twin of its type, casts an address to a narrow integer through `uintptr_t`, writes the narrow address words of the data in a constructor and drops `const` from a global it writes, and widens a literal before a 64-bit shift; the Rust backend writes dword address words at their width and gives a 32-bit port a `build.rs` that links it without large-address awareness; the CMake template does the same by the pointer width. `ras signatures` prints `i32` and `pops N` for x86. Left: a call through a pointer to native code (an import held in a variable) reports no pops (task 63); an `edx:eax` result is missed when the callee writes `edx` on no path the analysis sees; the frames of functions with indirect calls could shrink when the possible callees are known (task 64); the universal calls and the stack-pointer arithmetic around them read poorly in C and are a target of task 47.
        files: crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-lift/src/raise/abi.rs crates/ra-asm-lift/src/raise/stack.rs crates/ra-asm-lift/src/raise/promote.rs crates/ra-asm-lift/src/raise/stubs.rs crates/ra-asm-lift/src/raise/frame.rs
        waits-on: 41 24
    - 43 ‹task› The first rhtest program: core {c4} [in_progress]
        objective: The old pipeline's test programs (`testdata/rhtest/src`, sixteen programs over `rhtest.h` and `rhtest_main.c`, built with MSVC 14.51 for x86 as `<name>-O2` and `<name>-Od`, with a link map each) become fixtures here, starting with `core`: `testdata/rhtest/core-O2.exe` and `core-Od.exe` port through `ras emit` as C and as Rust, build, and print `testdata/rhtest/core.txt`; the emit oracle runs them like the clang fixtures (`run_oracle_32` with the rhtest expected file), the testdata README gets a row for `rhtest/`, and the card's report records what the engine needed and the limitations (the machine and promoted levels cannot run a 32-bit CRT program; an integer constant in the image's address range of a stripped image may be taken for an address).
        acceptance: `cargo test -p ra-asm-emit -q` passes with core-O2 and core-Od in the emit oracle as C and as Rust, each printing the 54 lines of `core.txt`.
        notes: In progress 2026-11-11. Landed for it so far, all green in the suite: pointer seeds and code addresses in immediates for an image without relocations (MSVC /O2 strips `.reloc`), the two-level MSVC jump table, `_cexit`/`_c_exit` return, `cpuid`/`xgetbv`/`rdtsc`/`stmxcsr`/`ldmxcsr` lifted with runtime helpers (`ra_cpuid`, `ra_xgetbv`, `ra_rdtsc`, `ra_mxcsr`), the x86 CRT imports the host lacks (`_except_handler4_common` and the EH helpers, `prototypes::host_lacks`) as IR stubs that trap, the CRT functions that walk 32-bit tables (`_initterm`, `_initterm_e`, the onexit table, `__p___argv`, `_get_initial_narrow_environment`) routed to `ra_x86_*` shims of the runtime (`prototypes::x86_shim`, driver.c, rhipper.rs), a `sec_headers` global for the CRT's reads of the PE header, calls through a register that holds an import slot (`mov ebx,[__imp_GetStdHandle]; call ebx`) resolved by a dataflow over the blocks (ra-asm-analysis/src/imports.rs, `Function.register_import_calls`, used by the lift's indirect-call arm), a `noreturn` function's `ret` lifted as unreachable, `RA_ENTER` tracing under `-DRA_TRACE`, and a vectored exception handler in the driver so the crash report survives a program's own unhandled-exception filter. Where core-O2 stands: the standalone C port at -O0 runs the CRT startup and reaches `main → rhtest_run → level1 → seed → add3 → check → put`; in `put` (0x402b00, the byte-copy loop into the line buffer) the port stores through a pointer the raise made 0 (`rax_phi_1_next = 0u` on one path into the loop; see `ras disasm testdata/rhtest/core-O2.exe 0x402b00` against the C of F_00402b00 in `out/core-o2/program.c`), an access violation writing 0x0; suspect the known-values or simplify pass over the loop's phi, or the `xor eax,eax` of the register-import call sequence. The Rust port of core-O2 builds and stops with `a goto to L_b_4033a4` in `__scrt_acquire_startup_lock`: task 54. Build a port by hand as `rhipper.context.md` describes; the reference binaries of the other programs are under `C:\Users\tyler\Documents\code\Rhipper\tests\rhtest\bins\msvc-14.51-x86\<name>-<variant>\` until task 55 copies them in.
        files: crates/ra-asm-lift/src/raise/known.rs crates/ra-asm-lift/src/raise/simplify.rs crates/ra-asm-lift/src/raise/promote.rs crates/ra-asm-lift/src/x86.rs crates/ra-asm-lift/src/raise/prototypes.rs crates/ra-asm-emit/tests/oracle.rs runtime/driver.c testdata/rhtest/src/rhtest.h testdata/README.md
        waits-on: 42 28
        effort: high
        priority: 5
    - 55 ‹task› The other rhtest programs {c4} [defined]
        objective: The remaining MSVC 14.51 builds (`calls`, `int64`, `float`, `fpenv`, `misc`, `crt`, `sys`, `win`, `seh`, `eh`, `cpp`, `com`, `stl`, `stlio`, `engine`, each `-O2` and `-Od`, plus the `-GL` and `-IA32` variants) join `testdata/rhtest` with their maps and expected outputs, in the order of what each needs from the engine (the integer and x87 programs first, the C runtime and Win32 ones next, SEH, C++ exceptions and the STL last), and each is added to the emit oracle as C and as Rust as it ports; a program that needs a milestone J task (exceptions, task 49) is added to the oracle as `ignored` with the task named.
        acceptance: `cargo test -p ra-asm-emit -q` passes with every rhtest program that does not depend on exception handling in the emit oracle, each printing its expected file.
        files: testdata/rhtest/src/rhtest.h crates/ra-asm-emit/tests/oracle.rs crates/ra-asm-lift/src/x86.rs crates/ra-asm-lift/src/x87.rs crates/ra-asm-lift/src/raise/prototypes.rs runtime/driver.c runtime/rhipper.rs testdata/README.md
        waits-on: 43
        effort: high
        priority: 4
    - 56 ‹task› The Visual Studio 2010 builds: a statically linked CRT {c4} [defined]
        objective: The `msvc-16.00-x86` builds of the rhtest programs (`/MT`, the toolset of a VS2010 game client) port with the C runtime recognized and called rather than lifted: the CRT's functions found by their code (task 46) take their prototypes from the table, their bodies are not emitted, and the startup (`__tmainCRTStartup`) is replaced by the driver's own so `main` runs with the host's runtime.
        acceptance: `cargo test -p ra-asm-emit -q` passes with the VS2010 `core` and `int64` builds in the emit oracle.
        files: crates/ra-asm-analysis/src/lib.rs crates/ra-asm-lift/src/raise/prototypes.rs crates/ra-asm-lift/src/lib.rs runtime/driver.c testdata/README.md
        waits-on: 55 46
    - 57 ‹task› The MSVC code shapes in the decoder and function oracles {c2} [defined]
        objective: The objdump oracle and the map-file function test run over `testdata/rhtest` too: the MSVC shapes (two-level jump tables, `int 3` padding between functions, `__security_check_cookie`, hot and cold halves of one function, the `/GL` builds' calling conventions) are proven in the decoder and the analysis, not only in the port; a map's function the analysis misses or splits fails the test with its name.
        acceptance: `cargo test -p ra-asm-x86 -p ra-asm-analysis -q` passes over every binary under `testdata/rhtest`.
        files: crates/ra-asm-x86/tests/objdump.rs crates/ra-asm-analysis/tests/fixtures.rs crates/ra-asm-analysis/src/lib.rs
        waits-on: 43
        priority: 3
    - 58 ‹task› The x87 unit at 80 bits {c3} [defined]
        objective: The eight x87 registers as 80-bit extended values through a software `f80` of the runtime (load and store of `tbyte`, add, subtract, multiply, divide, square root, compares, conversions to and from `double`, `float` and the integers, the rounding modes of the control word), so a program that depends on the extended precision (a long summation, `fld tbyte ptr`, a VS2010 `printf` of a double) prints what the reference prints; the double-precision unit of today stays the default and `rhipper.json` selects the exact one.
        acceptance: `cargo test -p ra-asm-lift -q` passes with a fixture that sums a series in x87 and prints 17 digits, matching native under the exact unit.
        files: crates/ra-asm-lift/src/x87.rs runtime/rhipper.h runtime/driver.c runtime/rhipper.rs testdata/src/float.c
        waits-on: 41
    - 67 ‹task› Thread-local storage {c3} [defined]
        objective: A program with `__declspec(thread)` variables ports: the TLS directory's template and callbacks (task 5 reads them) become the runtime's per-thread block, `fs:[0x2c]` and `gs:[0x58]` (the thread-local array) read the port's own block, `__tls_index` resolves, and the callbacks run at startup as the loader runs them.
        acceptance: `cargo test -p ra-asm-lift -q` passes with a fixture that reads and writes a thread-local counter from two threads and prints both values.
        files: crates/ra-asm-lift/src/x86.rs crates/ra-asm-lift/src/state.rs runtime/driver.c runtime/rhipper.rs crates/ra-asm-pe/src/tls.rs
        waits-on: 41
  - 37 ‹milestone› H: other architectures and formats {c4} [defined]
      objective: The `Arch` trait and the loader take a second architecture and a second container without changes to the IR, the raising passes or the backends.
      acceptance: An ARM64 PE lifts to IR that passes clang, and an ELF x86-64 file loads and disassembles.
    - 38 ‹task› ARM64 PE {c4} [defined]
        objective: An `Arch` implementation for AArch64 over a maintained decoder crate, the ARM64 exception table, and the lift of the integer subset.
        acceptance: `cargo test -q` passes with a clang-built ARM64 fixture that lifts and passes the IR gate.
        files: crates/ra-asm-arch/src/lib.rs crates/ra-asm-pe/src/exception.rs crates/ra-asm-lift/src/lib.rs testdata/build.ps1
        waits-on: 55
    - 39 ‹task› ELF and Mach-O {c3} [defined]
        objective: `ra-asm-format` grows the ELF and Mach-O containers behind the same `Image` trait.
        acceptance: `cargo test -q` passes with a Linux x86-64 binary whose sections and symbols `ras info` prints.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-pe/src/image.rs apps/ras/src/report.rs
        waits-on: 55
  - 44 ‹milestone› J: from a correct port to a readable one {c5} [defined]
      objective: The depth that makes the C read as source: types, names, idioms, a hand-port takeover per function, exceptions, resources, and a trace of the reference beside the port. Each task is a nice-to-have until the one before it is solid; the oracle and the fixtures grow with each.
      acceptance: A real program of a few thousand functions ports with its structures, its names and its strings, and a person can take over one function at a time without touching the rest.
      notes: Defined 2026-11-10 as the vision after milestones E to I; the order below is the suggested priority, and a task may move up when a program needs it.
    - 45 ‹task› Width and type recovery {c4} [defined]
        objective: Beyond the width narrowing of task 25: pointers typed by what they point to, structure fields from the offsets a base pointer is accessed at, arrays from indexed accesses, `bool` for values only compared and branched on, string literals as `const char *`, and the types of a PDB when the binary names one.
        acceptance: `cargo test -q` passes with a fixture of a struct of mixed fields that reads as a `struct` with named fields in the C.
        files: crates/ra-asm-lift/src/raise/widths.rs crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-emit/src/c.rs crates/ra-asm-ir/src/types.rs
        waits-on: 25 55
    - 46 ‹task› Names from the binary {c3} [defined]
        objective: Names from RTTI (classes, vtables), from a PDB when present, from the exports and from the CRT (a statically linked C runtime recognized by its code, so `printf` reads as `printf` and its body is not ported), and the Windows API prototype table generated from the SDK headers instead of written by hand.
        acceptance: `cargo test -q` passes with the classes fixture's names carried into the C, and a fixture with a static CRT ports with the CRT called, not lifted.
        files: crates/ra-asm-analysis/src/lib.rs crates/ra-asm-analysis/src/rtti.rs crates/ra-asm-lift/src/raise/prototypes.rs crates/ra-asm-pe/src/debug.rs crates/ra-asm-emit/src/functions.rs
        waits-on: 36
    - 61 ‹task› Names from a link map beside the image {c2} [defined]
        objective: When `<exe>.map` (the MSVC or the lld form, which the fixture tests already parse) lies beside the image, or `rhipper.json` names one, the analysis names its functions from it, and the names reach the IR, the C comments, `functions.json` and `ras serve`, so a fixture's port and a `-DRA_TRACE` trace read by name without a hand lookup; a map that disagrees with the image (a different size or entry point) is refused with its path.
        acceptance: `cargo test -p ra-asm-analysis -p ras -q` passes with a test that names `testdata/rhtest/core-O2.exe`'s functions from its map and finds `_rhtest_run` in `ras functions --json`.
        files: crates/ra-asm-analysis/src/lib.rs crates/ra-asm-analysis/tests/fixtures.rs apps/ras/src/functions.rs apps/ras/src/project.rs
        waits-on: 36
        priority: 3
    - 47 ‹task› Idioms and loop forms {c3} [defined]
        objective: The compiler's output read back as its source: `memcpy`, `memset` and `strlen` loops, division and modulo by a constant through multiplication, rotates and byte swaps, min and max through cmov, `for` loops with an induction variable, and a `switch` over a string table.
        acceptance: `cargo test -q` passes with the control fixture's loops read as `for` loops and its idioms as calls.
        files: crates/ra-asm-lift/src/raise/simplify.rs crates/ra-asm-lift/src/raise/known.rs crates/ra-asm-emit/src/structure.rs crates/ra-asm-emit/src/c.rs
        waits-on: 45
    - 63 ‹task› Imports called through a pointer kept in data {c2} [defined]
        objective: A call through a pointer loaded from a data word that holds an import's address, or from a `GetProcAddress` result stored in a global, takes the import's convention: the data analysis marks the words that hold import slots' values, the lift calls the import with its prototype and pops what it pops, and the universal call path is left for the pointers the engine cannot resolve; the pops of a native target reached through the universal path come from the driver's binding table instead of `ra_pops`.
        acceptance: `cargo test -p ra-asm-lift -q` passes with a fixture that calls `WriteFile` through a function pointer in a global, at every level.
        files: crates/ra-asm-analysis/src/imports.rs crates/ra-asm-lift/src/data.rs crates/ra-asm-lift/src/raise/promote.rs runtime/driver.c testdata/src/control.c
        waits-on: 42
    - 64 ‹task› Devirtualization by vtable slot {c3} [defined]
        objective: A call through a vtable slot whose receiver's class is known (the vtable store the constructor made, or the only vtables that fill the slot) resolves to the slot's functions: the signature pass takes their parameters instead of the unknown callee's, the frames of the callers shrink to what the callees pop, and the C reads as a direct call where one class fills the slot and as a switch over the classes otherwise.
        acceptance: `cargo test -q` passes with the classes fixture's virtual calls read as direct calls in the C.
        files: crates/ra-asm-analysis/src/rtti.rs crates/ra-asm-lift/src/raise/abi.rs crates/ra-asm-lift/src/raise/promote.rs crates/ra-asm-emit/src/c.rs
        waits-on: 36 42
    - 48 ‹task› The hand-port takeover {c4} [defined]
        objective: A port is something a person changes: `rhipper.json` (or a marker in the source) names the functions the hand-written sources own, `ras emit` writes them as declarations instead of definitions, the function table marks them, and the workbench shows and toggles the ownership per function.
        acceptance: `cargo test -p ras -q` passes with a fixture with one function taken over by hand that builds, runs, and prints what the reference prints, and a table that says which function is hand-written.
        files: apps/ras/src/project.rs apps/ras/src/emit.rs crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/src/functions.rs
        waits-on: 31
    - 49 ‹task› Exceptions and unwinding {c3} [defined]
        objective: SEH and C++ exception handling from the x64 unwind data and handler tables, and from the x86 `_except_handler4` scope tables and the `FuncInfo` of the C++ frame handler: `__try` and `__except` regions, C++ landing pads and the cleanup of locals, so a program that throws ports instead of trapping.
        acceptance: `cargo test -q` passes with a fixture that throws and catches a C++ exception and prints what the reference prints.
        files: crates/ra-asm-pe/src/exception.rs crates/ra-asm-lift/src/x86.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-emit/src/c.rs runtime/driver.c
        waits-on: 24 55
    - 50 ‹task› Resources and typed data {c2} [defined]
        objective: PE resources (strings, dialogs, icons, manifests) extracted to files the port loads, and data sections written as typed arrays and strings where their uses say so, instead of byte blobs.
        acceptance: `cargo test -q` passes with a fixture with a string table resource that ports with the strings in a file, and whose data reads as arrays in the C.
        files: crates/ra-asm-pe/src/lib.rs crates/ra-asm-lift/src/data.rs crates/ra-asm-emit/src/c.rs apps/ras/src/emit.rs
        waits-on: 45
    - 69 ‹task› Windows GUI programs {c3} [defined]
        objective: A program with a window ports: the window procedure the OS calls back reaches the lifted function (the x64 trampolines and the x86 universal stubs take a `stdcall` callback of four arguments, with the pops the OS expects), the message loop and `PostQuitMessage` run, and the pipeline's run step closes the window after the timeout rather than hanging; a fixture registers a class, creates a window, paints once and quits.
        acceptance: `cargo test -p ra-asm-emit -q` passes with the window fixture, 64 and 32, printing its message log.
        files: runtime/driver.c crates/ra-asm-lift/src/raise/stubs.rs apps/ras/src/pipeline.rs testdata/build.ps1 testdata/README.md
        waits-on: 42
    - 70 ‹task› DLL images {c3} [defined]
        objective: A PE DLL ports as a DLL: its exports (by name and ordinal, with forwarders) are the port's exports, `DllMain` runs from the port's, the relocations are honoured by the host's loader, and the pipeline builds it as a shared library the reference program (or a test host) loads; a game's DLLs then port beside its executable.
        acceptance: `cargo test -p ras -q` passes with the exports fixture ported as a DLL and a host that loads it and calls an export.
        files: apps/ras/src/emit.rs apps/ras/src/pipeline.rs apps/ras/templates/CMakeLists.txt crates/ra-asm-emit/src/c.rs runtime/driver.c
        waits-on: 31
    - 51 ‹task› The trace beside the port {c4} [defined]
        objective: A recorded trace of the reference (calls, returns, and the values at each) and the same trace of the port, compared to the first point where they part; the basis of the workbench's side-by-side stepping.
        acceptance: `cargo test -p ras -q` passes with a port with one wrong function found by the trace comparison in one run, naming the function.
        files: runtime/driver.c runtime/rhipper.h apps/ras/src/pipeline.rs crates/ra-asm-emit/src/c.rs
        waits-on: 34 62
    - 52 ‹task› Incremental and parallel {c3} [defined]
        objective: The analysis cached under `build/`, the lift and the emit of only the functions a change touched, and the lift of independent functions in parallel, so a program of tens of thousands of functions turns around in seconds.
        acceptance: `cargo test -p ras -q` passes, a second `ras pipeline` run on an unchanged program finishes in under a second per step, and a large program lifts on every core.
        files: apps/ras/src/pipeline.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-analysis/src/lib.rs
        waits-on: 34
    - 53 ‹task› Packed and protected binaries {c2} [defined]
        objective: Recognize the common packers by their entry section and import table, and dump the unpacked image through an emulated run to the original entry point, so the pipeline starts from the real program.
        acceptance: `cargo test -q` passes with a UPX-packed fixture that ports as its unpacked program.
        files: crates/ra-asm-pe/src/image.rs crates/ra-asm-analysis/src/lib.rs apps/ras/src/pipeline.rs
        waits-on: 41
