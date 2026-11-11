# Rhipper Asm

## Gambits

- g1 ‹gambit› Exact or it stops: the lift traps rather than guesses, and every oracle stays green through every change {w5} [active]
    focus: lift raise oracle fixture interpreter
- g2 ‹gambit› Local work first: everything runs and is proven on this machine with clang, no service and no network {w4} [active]
    focus: oracle pipeline runtime
- g3 ‹gambit› The old pipeline's test programs port before any new architecture or readability work {w4} [active]
    focus: rhtest x86 32-bit CRT
- g4 ‹gambit› No compatibility with Ghidra or the Python pipeline; the engine writes its own shapes and the workbench follows {w3} [active]
    focus: workbench serve functions.json rhipper-ide
- g5 ‹gambit› Readability only on top of a correct port: types, names and idioms after the oracle, never instead of it {w3} [active]
    focus: idioms types names structuring
- g6 ‹gambit› One IR, one raise, one set of backends: a new architecture or container is a decoder and a loader, never a fork of the middle {w4} [active]
    focus: arch decoder loader container MIPS ARM PowerPC

- 1 ‹project› Rhipper Asm {c5} [in_progress]
    objective: A pure-Rust disassembly and lifting core that replaces Ghidra and the Python Rhipper in the Rhipper Workbench: a binary in (a Windows PE first, then the executables of Linux, macOS and the consoles), LLVM IR out, and C, C++ or Rust source out of that IR that builds and runs as a program of today's machine.
    acceptance: The workbench runs a project from the reference binary to a built and running port with no Ghidra, no JDK and no Python installed; a PlayStation 2 game ports the same way, as a PC program over the platform runtime.
    notes: Decisions, 2026-11-10. (1) The decoder is the iced-x86 crate behind the crate's own `Instr` record, so the work goes into the lift and the backends, not into opcode tables; a second architecture plugs in behind the same trait. (2) The IR is a subset of LLVM IR with its own printer: every `.ll` the core writes must pass `clang -c -x ir`, which is the syntax and type gate, and clang is also the way to run lifted code in tests. (3) The first target is x86-64 PE; the loader and the decoder handle PE32 and 32-bit code from the start because the old pipeline's test programs are 32-bit. (4) Milestones run in order A to H below; a later milestone may start when its inputs exist. (5) No compatibility with the old pipeline's files or conventions (Ghidra's corpus and names, the fidelity scores, the gen-index comments): the engine writes its own shapes and the workbench's readers of the old ones are deleted as the new ones land, decided 2026-11-10; Ghidra-based projects stop opening in the workbench until `ras` lifts 32-bit programs. (6) The lift is exact or it stops: an instruction the lift does not cover becomes a trap that names its address, never a guess, so the grade of a function is its trap count and the oracle is the proof; readability work (widths, types, idioms) must keep the oracle green. (7) 32-bit x86 is its own milestone (I), ahead of ARM64 and ELF (H), because it is what lets the old test programs and projects port through `ras`. (8) 2026-11-11: the port of a 32-bit program is a 64-bit executable linked without large-address awareness that keeps the program's 32-bit pointers; see milestone I. (9) 2026-11-11: the accepted cards are pruned from this file into `rhipper.done.md`, same format and ids, so the roadmap shows the work ahead and the record stays; a `waits-on` to a pruned card is dropped, since an accepted card is met. Vision, 2026-11-11: milestones A to E are done; F, G and I are in progress; the engine lifts, raises and writes x64 and x86 programs to C, C++ and Rust that build and run, the workbench runs it end to end, and nothing of Ghidra or Python remains. The far shape, in the order the milestones below suggest: the rhtest programs and the readability depth (J), the second and third architectures through the one IR (K ARM, L MIPS and the PlayStation family, M PowerPC and the rest), the platform runtimes that make a console program a PC program (N), signatures and libraries (O), the IR interpreter and dynamic analysis (P), decompilation depth (Q), the port as a codebase (R), the workbench end game (S), scale and release (T). Each is a nice-to-have until the one before it is solid, and the oracle and the fixtures grow with each.
  - 26 ‹milestone› F: the backends {c5} [in_progress]
      objective: `ra-asm-emit` writes C, C++ or Rust source from the raised IR, with a runtime header per language, and the function table the workbench reads.
      acceptance: A fixture's C, C++ and Rust outputs each build in one round and print what the reference prints.
      notes: Started 2026-11-10 with the C backend. By 2026-11-11 the C, the C++ dialect and the Rust output all pass the oracle (cards 27, 28, 30, 31 in rhipper.done.md); the C++ output of its own shape (task 29) waits for the names and vtables of task 46.
    - 29 ‹task› C++ {c3} [in_progress]
        objective: The C output with references, classes where RTTI and vtables name them, and overloads where the mangled export names give them.
        acceptance: `cargo test -p ra-asm-emit -q` passes with the classes fixture emitted as a `class` per RTTI class whose virtual functions are its methods, and its C++ port prints what the reference prints.
        notes: The dialect landed 2026-11-11: `ras emit --lang cpp`, or `"lang": "cpp"` in `rhipper.json`, writes the same source as `.cpp` files with the imports, the prototypes and the data under `extern "C"` (the driver is C, the imports are the DLLs' names, and a C++ `const` global would otherwise have internal linkage); clang++ builds control64 split and float64 whole beside the C driver, and the oracle checks both. The shape of its own (classes, references, overloads) waits for the names and vtables of task 46.
        files: crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/src/lib.rs crates/ra-asm-analysis/src/rtti.rs crates/ra-asm-emit/tests/oracle.rs
        waits-on: 46
        priority: 2
    - 54 ‹task› Return blocks duplicated in the structuring {c2} [defined]
        objective: Card 43 landed the mechanism: a short placed block whose exit the context can name is written again (`Stmt::Copy`, chains up to three, a conditional latch as an `if`), and the Rust ports of the MSVC CRT programs have no goto. Left: a structuring unit test of a loop with two exits to one return block, the limit above which a block is not copied but stays a goto in C and stops the Rust port naming the function and the block, and a line in the function table's readability data counting the copies.
        acceptance: `cargo test -p ra-asm-emit -q` passes with a structuring test of a loop with two exits to one return block that produces no goto and a test of a block over the limit that keeps its goto in C.
        files: crates/ra-asm-emit/src/structure.rs crates/ra-asm-emit/src/rust.rs crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/tests/oracle.rs
        priority: 4
  - 32 ‹milestone› G: the pipeline and the workbench {c4} [in_progress]
      objective: `ras` is the engine of the Rhipper Workbench: a pipeline with steps, a JSON interface the IDE calls, and a managed install from releases; Ghidra, the JDK and Python leave the tool list.
      acceptance: The workbench runs the pipeline on a project with only `ras`, clang and CMake installed.
      notes: Started 2026-11-10 with the pipeline, ahead of the backends' C++ and Rust tasks, because the workbench sweep (no Ghidra, no Python) needs an engine to call first. The pipeline (33), `ras serve` (34) and the facts of the old pipeline (36) are accepted and pruned to rhipper.done.md.
    - 35 ‹task› The managed install {c2} [in_progress]
        objective: The workbench downloads `ras` from this repository's releases into its tools directory, as it did Rhipper, and a CI workflow here builds and publishes the release.
        acceptance: A fresh workbench installs `ras` on first start and shows its version under Tools.
        notes: The workbench side landed 2026-11-10 (`RhipperTools.RAS` in rhipper-ide): it reads `<releases>/latest/download/version.txt`, downloads `ras-<version>-<os>-<arch>.zip` (`windows`/`macos`/`linux`, `x64`/`arm64`) with its `.sha256`, unpacks it to `<tools>/ras/<version>/ras.exe`, and reads `ras --version` for the version it shows. The workflow landed 2026-11-11 (.github/workflows/release.yml): a tag `v<version>` that matches the workspace version runs the tests with LLVM installed, builds `ras` in release, packs `ras.exe` with the runtime files, and publishes `version.txt`, `ras-<version>-windows-x64.zip` and its `.sha256`. Left, and the person's to do: the repository has no remote yet; the workbench's `RAS_RELEASES` names `github.com/zeruth/rhipper-asm`, so the remote must be that or the constant must change, and the first tag proves the workflow.
        files: .github/workflows/release.yml Cargo.toml
        priority: 2
    - 59 ‹task› `ras doctor` {c1} [defined]
        objective: One command checks the tools a project needs on this machine (`clang`, `clang++`, `lld`, `cmake`, `ninja` or Visual Studio, `cargo` for a Rust port, `llvm-symbolizer` for a crash), prints each with its version or `missing` and what it is needed for, as text and as `--json`, and exits non-zero when a required one is absent; the pipeline's `build` step runs the same check first and fails with that message instead of CMake's.
        acceptance: `cargo test -p ras -q` passes with a test that parses the JSON of the command on this machine and finds clang in it.
        files: apps/ras/src/main.rs apps/ras/src/pipeline.rs apps/ras/tests/pipeline.rs
        priority: 2
    - 60 ‹task› The coverage report {c2} [defined]
        objective: `ras lift --report` (and the same under `functions --json` as `coverage`) counts, over a whole program, the instructions the lift does not cover by mnemonic with one example address each, the functions by trap count, the imports without a prototype, and the indirect jumps that did not resolve, so the next lift work is chosen by what a real program needs rather than by a guess.
        acceptance: `cargo test -p ras -q` passes with a test that runs the report on a fixture and finds its tables, and the report on `testdata/rhtest/core-O2.exe` lists no uncovered instruction.
        files: apps/ras/src/lift.rs crates/ra-asm-lift/src/lib.rs apps/ras/src/functions.rs apps/ras/src/report.rs
        priority: 3
    - 62 ‹task› The reference run beside the port {c2} [defined]
        objective: The pipeline's `run` step also runs the reference binary (`exe` of `rhipper.json`) with the same arguments, working directory and timeout when `run.compare` is on, and records both outputs' first difference and the exit codes in `runs.jsonl` and the log as a verdict (`same`, `differs at line N`, `port crashed`); the workbench shows the verdict on the run step.
        acceptance: `cargo test -p ras -q` passes with the pipeline test checking a `same` verdict on control64.
        files: apps/ras/src/pipeline.rs apps/ras/src/project.rs apps/ras/tests/pipeline.rs
    - 135 ‹task› ras lift reproduces the oracle: --standalone and --level {c1} [defined]
        objective: The CLI lifts what the oracle lifts: `ras lift --level machine|promoted|abi` picks the raise level and `--standalone` (the default at the ABI level, as the oracle and the emit have it) folds the data sections into globals and resolves the relocated references, so the `.ll` a person reads is the one the port was built from; the worker of card 43 lost ten turns comparing a non-standalone lift with the oracle's IR.
        acceptance: `cargo test -p ras -q` passes with a test that lifts control64 through the CLI at the ABI level standalone and finds the data global in the output.
        files: apps/ras/src/lift.rs apps/ras/src/main.rs crates/ra-asm-lift/src/lib.rs
  - 40 ‹milestone› I: 32-bit x86 {c4} [in_progress]
      objective: The lift, the raising passes and the C backend handle 32-bit x86 PE programs, so the old pipeline's test programs and projects port through `ras`.
      acceptance: The x86 fixtures pass the execution oracle at every level, and a test program of the old pipeline builds and prints what the reference prints.
      notes: Placed before milestone H on 2026-11-10 (project decision 7). Decision 8, 2026-11-11: the port of a 32-bit program is a 64-bit executable that keeps the program's 32-bit pointers. It is linked without large-address awareness, so the loader keeps every address of the process (heap, stack, environment block, kernel32) below 2 GB and a pointer truncated to 32 bits is still the pointer; its data words that hold addresses are written at startup, since no compiler folds a truncated address into a static initializer. The levels that map the image at its base (machine, promoted) run as plain processes instead, with the driver's stack placed low: the loader of a non-large-address-aware process puts its own heap and environment block at the image's base. The 32-bit lift (41) and the x86 calling conventions (42) are accepted and pruned to rhipper.done.md.
    - 43 ‹task› The first rhtest program: core {c4} [accepted]
        objective: The old pipeline's test programs (`testdata/rhtest/src`, sixteen programs over `rhtest.h` and `rhtest_main.c`, built with MSVC 14.51 for x86 as `<name>-O2` and `<name>-Od`, with a link map each) become fixtures here, starting with `core`: `testdata/rhtest/core-O2.exe` and `core-Od.exe` port through `ras emit` as C and as Rust, build, and print `testdata/rhtest/core.txt`; the emit oracle runs them like the clang fixtures (`run_oracle_32` with the rhtest expected file), the testdata README gets a row for `rhtest/`, and the card's report records what the engine needed and the limitations (the machine and promoted levels cannot run a 32-bit CRT program; an integer constant in the image's address range of a stripped image may be taken for an address).
        acceptance: `cargo test -p ra-asm-emit -q` passes with core-O2 and core-Od in the emit oracle as C and as Rust, each printing the 54 lines of `core.txt`.
        notes: Done 2026-11-11 by the agent's worker (claude-fable-5-1, 323 turns, $24.60, the first card run in this project); the one lint warning it left (a `match` on one pattern in `structure.rs`) and two more clippy found are fixed in the commit, and its look-backs became cards 135 (`ras lift --standalone`) and 136 (partial register writes); `ras imports` already lists the imports. The earlier increment that got the CRT startup running is in commit 6b63348. The worker's report: `core-O2.exe` and `core-Od.exe` port through the emit path as C and as Rust, build, and print the 54 lines of `core.txt` (the exit line included, compared against the port's exit code). `cargo test -p ra-asm-emit -q`: 22 passed, 2 ignored (the `_traced` debugging aids). `cargo test -q` over the workspace: all green; the clang fixtures still structure with no `goto`. Both ports now emit with no lift warning and no `goto` at all (from 41 and 57).\n\n**What the engine needed** (in the order the ports failed):\n- `rhipper.h`: a diagnostic pragma — the uniform `uint64_t(uint64_t…)` import prototypes clash with clang's builtins for `exit`/`_exit` (`-Wincompatible-library-redeclaration`); general for every CRT import.\n- `raise/stack.rs`: the stack analysis tracked only the `rsp` slot symbolically, so every `ebp`-framed function (MSVC `/Od`, and MSVC's own CRT at `/O2`) lost its stack parameters — `__scrt_initialize_crt` read an uninitialized slot and fell through `RA_UNREACHABLE` into lld's `int3` padding. It now tracks every general register across blocks, callee-saved ones surviving a call.\n- `x86.rs` `flat_address`: `[eax + 0x4058e8]` (a base register plus an in-image displacement, standalone mode) dropped the displacement and stored through `eax` alone — `put` wrote to address 0.\n- `raise/abi.rs` + `promote.rs`: a callee that preserves `eax` (`__security_check_cookie`) was modelled as clobbering it, so `make_box`/`sort_check`, which set their result before that call, returned 0. `Signature.preserves` is the caller-saved set a function and its callees never write (pessimistic start, refined by the fixpoint); the call model, the liveness and `after_call` honour it, and a callee returns in `eax` only if it writes it.\n- `prototypes.rs`: `_crt_atexit` had arity 0 (the host registered garbage and `exit` jumped into `sec_data`) → 1, with `_crt_at_quick_exit`; `InitializeSListHead` becomes a shim (`driver.c`, `rhipper.rs`, `rhipper.h`): the 64-bit host writes 16 bytes into the 8-byte list head and raises `STATUS_DATATYPE_MISALIGNMENT`.\n- `ra-asm-analysis/jumptable.rs`: the `/Od` `vowels` switch keeps its index in a stack slot between the `cmp` and the table read; `bound_of` follows memory places as well as registers.\n- `ra-asm-emit/common.rs`: a load may be inlined when no side effect sits between it and its use, and a pure expression embedding a load may not cross one either — the `cvtpd2ps xmm0, xmm0` conversion read its source after the store that zeroed the register (`pun` was −0.0f); this also reconciles the structurer's "pure condition block" with the printer.\n- `structure.rs` (for the Rust port, which cannot express `goto`): blocks that never continue are no exits for the joins (a trap `default` left a switch join-less); a second post-dominator map without the return blocks gives the join of an early-return `if`; the join is taken from the last folded condition; a condition block that feeds phis of the arm it falls through to folds with `Stmt::Moves`; a short placed block whose exit the context can name is written again (`Stmt::Copy`, chains up to three, a conditional latch as an `if`); `loop_exit` walks the post-dominator chain out of the loop. `simplify.rs` folds `x ^ x` and `x - x`.\n- `tests/oracle.rs`: `Fixture` paths under `testdata/`, rhtest helpers, the `exit = N` line, `-g -gcodeview` with a crash symbolized through `llvm-symbolizer`, the first differing line in the message, and ignored `_traced` tests (`-DRA_TRACE`). `driver.c` also reports the misalignment and in-page exceptions.\n\n**Limitations / open ends**: the machine and promoted levels cannot run a 32-bit CRT program (standalone only); a stripped image's in-range integer constants are taken for addresses (no reloc evidence); a sub-register write (`mov cl, …`) reads the full register in the model, so `put` gets a phantom `uint8_t ecx` parameter and `imin` an `ecx` one — a liveness refinement for partial writes is a card; the fall-through switch is expressed by copied case tails, not C fall-through; the stack-argument arithmetic around universal calls still reads poorly (task 47). No JSON shape changed; the workbench is unaffected.\n\n## Faster next time\n- [machine] The shell hook refuses `ras.exe`, `clang`, `llvm-symbolizer` and env-prefixed cargo; every port had to go through `cargo run -p ras` and the oracle itself — a note in the context that only `cargo …` runs would have saved ~8 refused turns.\n- The pass that dropped the displacement was in the lift, not the raise; comparing `ras lift` (non-standalone) output to the oracle's standalone IR misled me for ~10 turns — `ras lift` should take `--standalone` so the CLI reproduces what the oracle lifts.\n- `ras info` has no import listing; a `--imports` flag (or the import list in `info`) would have shown the UCRT-DLL layout at once.
        files: crates/ra-asm-lift/src/raise/known.rs crates/ra-asm-lift/src/raise/simplify.rs crates/ra-asm-lift/src/raise/promote.rs crates/ra-asm-lift/src/x86.rs crates/ra-asm-lift/src/raise/prototypes.rs crates/ra-asm-emit/tests/oracle.rs runtime/driver.c testdata/rhtest/src/rhtest.h testdata/README.md
        session: da6500e0-c6e7-41e9-a618-d17c3db10369 3cc23a7ae6149474 1794385541
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
    - 67 ‹task› Thread-local storage {c3} [defined]
        objective: A program with `__declspec(thread)` variables ports: the TLS directory's template and callbacks become the runtime's per-thread block, `fs:[0x2c]` and `gs:[0x58]` (the thread-local array) read the port's own block, `__tls_index` resolves, and the callbacks run at startup as the loader runs them.
        acceptance: `cargo test -p ra-asm-lift -q` passes with a fixture that reads and writes a thread-local counter from two threads and prints both values.
        files: crates/ra-asm-lift/src/x86.rs crates/ra-asm-lift/src/state.rs runtime/driver.c runtime/rhipper.rs crates/ra-asm-pe/src/tls.rs
    - 136 ‹task› Partial register writes in the liveness {c2} [defined]
        objective: A write to a sub-register (`mov cl, …`, `mov ax, …`, `setcc al`) counts as a read of the full register in the model, so a function that only ever defines `cl` before using `ecx` shows a phantom `ecx` parameter (`put` gets a `uint8_t ecx`, `imin` an `ecx`, in the rhtest core port). The liveness tracks the bytes of a general register: a partial write defines its bytes, a read of the full register reads the bytes not yet defined, and a parameter is inferred only for the bytes read before written; the lift's merge of a partial write stays as it is.
        acceptance: `cargo test -p ra-asm-lift -q` passes with `ras signatures testdata/rhtest/core-O2.exe` giving `put` and `imin` no `ecx` parameter, and every oracle green.
        files: crates/ra-asm-lift/src/raise/liveness.rs crates/ra-asm-lift/src/raise/abi.rs crates/ra-asm-lift/src/x86.rs crates/ra-asm-lift/tests/signatures.rs
  - 44 ‹milestone› J: from a correct port to a readable one {c5} [defined]
      objective: The depth that makes the C read as source: types, names, idioms, a hand-port takeover per function, exceptions, resources, and a trace of the reference beside the port. Each task is a nice-to-have until the one before it is solid; the oracle and the fixtures grow with each.
      acceptance: A real program of a few thousand functions ports with its structures, its names and its strings, and a person can take over one function at a time without touching the rest.
      notes: Defined 2026-11-10 as the vision after milestones E to I; the order below is the suggested priority, and a task may move up when a program needs it.
    - 45 ‹task› Width and type recovery {c4} [defined]
        objective: Beyond the width narrowing of task 25: pointers typed by what they point to, structure fields from the offsets a base pointer is accessed at, arrays from indexed accesses, `bool` for values only compared and branched on, string literals as `const char *`, and the types of a PDB when the binary names one.
        acceptance: `cargo test -q` passes with a fixture of a struct of mixed fields that reads as a `struct` with named fields in the C.
        files: crates/ra-asm-lift/src/raise/widths.rs crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-emit/src/c.rs crates/ra-asm-ir/src/types.rs
        waits-on: 55
    - 46 ‹task› Names from the binary {c3} [defined]
        objective: Names from RTTI (classes, vtables), from a PDB when present, from the exports and from the CRT (a statically linked C runtime recognized by its code, so `printf` reads as `printf` and its body is not ported), and the Windows API prototype table generated from the SDK headers instead of written by hand.
        acceptance: `cargo test -q` passes with the classes fixture's names carried into the C, and a fixture with a static CRT ports with the CRT called, not lifted.
        files: crates/ra-asm-analysis/src/lib.rs crates/ra-asm-analysis/src/rtti.rs crates/ra-asm-lift/src/raise/prototypes.rs crates/ra-asm-pe/src/debug.rs crates/ra-asm-emit/src/functions.rs
    - 61 ‹task› Names from a link map beside the image {c2} [defined]
        objective: When `<exe>.map` (the MSVC or the lld form, which the fixture tests already parse) lies beside the image, or `rhipper.json` names one, the analysis names its functions from it, and the names reach the IR, the C comments, `functions.json` and `ras serve`, so a fixture's port and a `-DRA_TRACE` trace read by name without a hand lookup; a map that disagrees with the image (a different size or entry point) is refused with its path.
        acceptance: `cargo test -p ra-asm-analysis -p ras -q` passes with a test that names `testdata/rhtest/core-O2.exe`'s functions from its map and finds `_rhtest_run` in `ras functions --json`.
        files: crates/ra-asm-analysis/src/lib.rs crates/ra-asm-analysis/tests/fixtures.rs apps/ras/src/functions.rs apps/ras/src/project.rs
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
    - 64 ‹task› Devirtualization by vtable slot {c3} [defined]
        objective: A call through a vtable slot whose receiver's class is known (the vtable store the constructor made, or the only vtables that fill the slot) resolves to the slot's functions: the signature pass takes their parameters instead of the unknown callee's, the frames of the callers shrink to what the callees pop, and the C reads as a direct call where one class fills the slot and as a switch over the classes otherwise.
        acceptance: `cargo test -q` passes with the classes fixture's virtual calls read as direct calls in the C.
        files: crates/ra-asm-analysis/src/rtti.rs crates/ra-asm-lift/src/raise/abi.rs crates/ra-asm-lift/src/raise/promote.rs crates/ra-asm-emit/src/c.rs
    - 48 ‹task› The hand-port takeover {c4} [defined]
        objective: A port is something a person changes: `rhipper.json` (or a marker in the source) names the functions the hand-written sources own, `ras emit` writes them as declarations instead of definitions, the function table marks them, and the workbench shows and toggles the ownership per function.
        acceptance: `cargo test -p ras -q` passes with a fixture with one function taken over by hand that builds, runs, and prints what the reference prints, and a table that says which function is hand-written.
        files: apps/ras/src/project.rs apps/ras/src/emit.rs crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/src/functions.rs
    - 49 ‹task› Exceptions and unwinding {c3} [defined]
        objective: SEH and C++ exception handling from the x64 unwind data and handler tables, and from the x86 `_except_handler4` scope tables and the `FuncInfo` of the C++ frame handler: `__try` and `__except` regions, C++ landing pads and the cleanup of locals, so a program that throws ports instead of trapping.
        acceptance: `cargo test -q` passes with a fixture that throws and catches a C++ exception and prints what the reference prints.
        files: crates/ra-asm-pe/src/exception.rs crates/ra-asm-lift/src/x86.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-emit/src/c.rs runtime/driver.c
        waits-on: 55
    - 50 ‹task› Resources and typed data {c2} [defined]
        objective: PE resources (strings, dialogs, icons, manifests) extracted to files the port loads, and data sections written as typed arrays and strings where their uses say so, instead of byte blobs.
        acceptance: `cargo test -q` passes with a fixture with a string table resource that ports with the strings in a file, and whose data reads as arrays in the C.
        files: crates/ra-asm-pe/src/lib.rs crates/ra-asm-lift/src/data.rs crates/ra-asm-emit/src/c.rs apps/ras/src/emit.rs
        waits-on: 45
    - 69 ‹task› Windows GUI programs {c3} [defined]
        objective: A program with a window ports: the window procedure the OS calls back reaches the lifted function (the x64 trampolines and the x86 universal stubs take a `stdcall` callback of four arguments, with the pops the OS expects), the message loop and `PostQuitMessage` run, and the pipeline's run step closes the window after the timeout rather than hanging; a fixture registers a class, creates a window, paints once and quits.
        acceptance: `cargo test -p ra-asm-emit -q` passes with the window fixture, 64 and 32, printing its message log.
        files: runtime/driver.c crates/ra-asm-lift/src/raise/stubs.rs apps/ras/src/pipeline.rs testdata/build.ps1 testdata/README.md
    - 70 ‹task› DLL images {c3} [defined]
        objective: A PE DLL ports as a DLL: its exports (by name and ordinal, with forwarders) are the port's exports, `DllMain` runs from the port's, the relocations are honoured by the host's loader, and the pipeline builds it as a shared library the reference program (or a test host) loads; a game's DLLs then port beside its executable.
        acceptance: `cargo test -p ras -q` passes with the exports fixture ported as a DLL and a host that loads it and calls an export.
        files: apps/ras/src/emit.rs apps/ras/src/pipeline.rs apps/ras/templates/CMakeLists.txt crates/ra-asm-emit/src/c.rs runtime/driver.c
    - 51 ‹task› The trace beside the port {c4} [defined]
        objective: A recorded trace of the reference (calls, returns, and the values at each) and the same trace of the port, compared to the first point where they part; the basis of the workbench's side-by-side stepping.
        acceptance: `cargo test -p ras -q` passes with a port with one wrong function found by the trace comparison in one run, naming the function.
        files: runtime/driver.c runtime/rhipper.h apps/ras/src/pipeline.rs crates/ra-asm-emit/src/c.rs
        waits-on: 62
    - 134 ‹task› Switch fall-through as fall-through {c2} [defined]
        objective: A case whose block flows into the next case's block is written as a C fall-through (`case 1: … /* falls through */ case 2:`) and as a Rust match arm that calls the shared tail, instead of the copied case tails the structuring writes today (`Stmt::Copy` chains); the copy stays for the shapes that are not a fall-through (a shared return block two exits reach).
        acceptance: `cargo test -p ra-asm-emit -q` passes with the rhtest core `vowels` switch written with a fall-through and no copied tail, in C and in Rust.
        files: crates/ra-asm-emit/src/structure.rs crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/src/rust.rs
  - 37 ‹milestone› H: containers and hosts {c4} [defined]
      objective: The loader takes a second and a third container behind the same `Image` without changes to the analysis, the IR, the raise or the backends: the executables of Linux and macOS, then the images of the consoles as their architectures arrive; and `ras` itself runs on Linux and macOS.
      acceptance: An ELF x86-64 file loads, disassembles and ports on this machine, and a Mach-O file loads and disassembles.
      notes: Renamed 2026-11-11 from "other architectures and formats": the architectures have milestones of their own (K, L, M), this one is the containers and the hosts. The console containers (PSX-EXE, the PS2 ELF, PRX, DOL and REL, XBE and XEX, N64 ROMs) are cards of the architecture that needs each.
    - 39 ‹task› ELF {c3} [defined]
        objective: `ra-asm-format` grows the ELF container (32 and 64, little-endian first, big-endian for the consoles that need it): program headers mapped by virtual address, sections, the dynamic section's imports and exports by symbol name, relocations (REL and RELA, the x86-64 and i386 kinds first), the symbol table when present, and `.eh_frame` as the function extents; `ras info`, `imports`, `exports` and `functions` work on it as on a PE, and the x86 lift's import boundary gains the System V convention for a Linux binary.
        acceptance: `cargo test -q` passes with a Linux x86-64 hello built here by clang (`--target x86_64-linux-gnu`) whose sections, symbols and functions `ras` prints, and whose lift passes the IR gate.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-pe/src/image.rs crates/ra-asm-pe/src/lib.rs apps/ras/src/report.rs testdata/build.ps1
        waits-on: 55
    - 71 ‹task› Mach-O {c2} [defined]
        objective: The Mach-O container (thin and fat, 64-bit): load commands, segments and sections, the symbol table and the dynamic symbol binding (`LC_DYLD_INFO` and the chained fixups of today's linkers), `__unwind_info` as function extents; `ras info` and `functions` work on it.
        acceptance: `cargo test -q` passes with a macOS x86-64 and an arm64 hello (built here by clang with a macOS target and an SDK stub, or checked in) whose sections and functions `ras` prints.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-pe/src/image.rs apps/ras/src/report.rs
        waits-on: 39
    - 72 ‹task› `ras` on Linux and macOS {c2} [defined]
        objective: The engine builds and its tests pass on Linux and macOS: the Windows-only parts of the runtime and the driver behind `cfg`, the oracle's clang invocations by host, the ports of platform-neutral programs (a console game over milestone N, a Linux binary) built by the host's clang, and the release workflow publishing `ras-<version>-linux-x64`, `macos-x64` and `macos-arm64` beside the Windows build that the workbench already knows how to download.
        acceptance: `cargo test -q` passes under the release workflow's Linux job, and the three archives publish from a tag.
        files: .github/workflows/release.yml runtime/driver.c crates/ra-asm-lift/tests/oracle.rs crates/ra-asm-emit/tests/oracle.rs
        waits-on: 35 39
    - 73 ‹task› The Win32 port on another host {c4} [defined]
        objective: A Windows program's port runs on Linux and macOS: the imports the port makes (kernel32, user32, the C runtime) resolve against a thin Win32 layer of the runtime that maps files, memory, threads, time, the console and the message loop onto the host, with the rest reported as missing by `ras doctor --host`; the layer is a crate the port links, not an emulator, and a program that uses what it covers runs natively.
        acceptance: `cargo test -q` passes under the Linux job with the rhtest `core` and `calls` ports running over the layer.
        files: runtime/driver.c runtime/rhipper.h runtime/rhipper.rs apps/ras/templates/CMakeLists.txt
        waits-on: 72 55
  - 74 ‹milestone› K: ARM {c5} [defined]
      objective: The second architecture through the one IR: AArch64 first (Windows on ARM, Linux, macOS, Android, the Switch), then ARM32 and Thumb-2; the raise and the backends do not change, only a `Target` grows.
      acceptance: An ARM64 PE and an ARM64 ELF of the same C fixture port to C that prints what the references print, built and run on this machine.
    - 38 ‹task› ARM64 PE {c4} [defined]
        objective: An `Arch` implementation for AArch64 over a maintained decoder crate (`yaxpeax-arm` or `bad64`), the ARM64 exception table (`.pdata` with the packed and the full unwind codes) as function extents, and the lift of the integer subset (moves, arithmetic with and without flags, logic, shifts, the compare-and-branch forms, loads and stores with every addressing mode, pairs, the extends, multiply and divide, `csel` and its kin, `adrp` and `add` as one address) over a `%State` of 31 general registers, the stack pointer, the flags and 32 vector registers.
        acceptance: `cargo test -q` passes with a clang-built ARM64 PE fixture (`--target aarch64-windows-msvc`) that lifts, passes the IR gate and runs lifted through an ARM64 driver when the host is ARM64, or through the IR interpreter (task 107) otherwise.
        files: crates/ra-asm-arch/src/lib.rs crates/ra-asm-pe/src/exception.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/state.rs testdata/build.ps1
        waits-on: 55
    - 75 ‹task› The ARM64 raise {c4} [defined]
        objective: A `Target` for AAPCS64 and the Windows ARM64 variant: `x0` to `x7` and `v0` to `v7` as arguments, `x8` as the indirect result, the link register as the return address instead of a push, `x29` frames with paired stores, the callee-saved set, the stack's 16-byte rule, variadic calls; the frame pass follows `sp` through the pre- and post-indexed forms; the signatures of the fixture match its source and the ABI level runs standalone.
        acceptance: `cargo test -q` passes with the ARM64 fixture at the ABI level, and `ras signatures` on it matches the source.
        files: crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-lift/src/raise/abi.rs crates/ra-asm-lift/src/raise/stack.rs crates/ra-asm-lift/src/raise/frame.rs crates/ra-asm-lift/src/raise/promote.rs
        waits-on: 38
    - 76 ‹task› ARM64 ELF and Mach-O: Linux, Android, macOS {c3} [defined]
        objective: The same lift over the ELF and Mach-O containers: the PLT and GOT as the import boundary (a `bl` to a PLT stub is a call to the import), `.eh_frame` extents, the TLS and the thread pointer, and Android's bionic conventions; an Android game's native library (`libmain.so`) ports to a C library a host program loads.
        acceptance: `cargo test -q` passes with an ARM64 Linux hello and a shared library whose functions port and whose exported function a host calls with the expected result.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/raise/prototypes.rs testdata/build.ps1
        waits-on: 75 39 71
    - 77 ‹task› NEON and the vector lift {c3} [defined]
        objective: The Advanced SIMD subset: the arrangement-typed arithmetic, the lane moves and duplicates, the loads and stores of structures, the reductions, the comparisons and the conversions, as the IR's vector operations with the lane type the arrangement names; the scalar floating point (`fadd`, `fcvt`, `fcmp` and the condition-select forms) beside it.
        acceptance: `cargo test -q` passes with the float fixture built for ARM64 running lifted and matching its native output.
        files: crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/state.rs crates/ra-asm-ir/src/types.rs
        waits-on: 75
    - 78 ‹task› ARM32 and Thumb-2 {c4} [defined]
        objective: The 32-bit ARM instruction sets: ARM and Thumb-2 with the interworking the low bit of an address carries, the IT blocks as predicated instructions, the conditional execution of every instruction as a branch around it, the literal pools as data, the PC as a readable register; the AAPCS32 raise with `r0` to `r3`, the stack and the VFP registers; PE ARMNT (Windows CE and Windows RT), ELF (Linux, Android, the Nintendo DS and 3DS as containers of their own later) and the raw images of the Game Boy Advance.
        acceptance: `cargo test -q` passes with a clang-built Thumb-2 fixture (`--target armv7-linux-gnueabihf`) that lifts and runs through the IR interpreter with the expected output.
        files: crates/ra-asm-arch/src/lib.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/raise/mod.rs testdata/build.ps1
        waits-on: 75 107
  - 79 ‹milestone› L: MIPS and the PlayStation family {c5} [defined]
      objective: MIPS through the one IR, with the machines that made it matter: the PlayStation (R3000), the PlayStation 2 (the R5900 Emotion Engine with its 128-bit multimedia unit, the vector units and the IOP), the PSP (Allegrex) and the Nintendo 64 (VR4300); a game's executable decodes, lifts and ports to C over the platform runtime of milestone N.
      acceptance: A PlayStation 2 game's ELF lifts with no trap in its own code (the SDK recognized by signatures), and a homebrew PS2 program ports to a PC program that prints what the emulator prints.
      notes: The MIPS family shares a decoder and a `%State`: 32 general registers, `hi` and `lo`, the FPU, the coprocessor 0 as traps, with the width (32 or 64) and the extensions (MMI, VFPU) as features of the `Target`. The branch delay slot is the one shape the CFG builder does not have today: a branch's successor edges leave after the slot instruction, and a likely branch's slot belongs to the taken edge alone.
    - 80 ‹task› The MIPS decoder and the delay slot {c4} [defined]
        objective: An `Arch` implementation for MIPS I through MIPS IV with the R5900 and Allegrex extensions behind feature flags, over a decoder of this repository's own (the encoding is regular enough that a table and the formats fit in a crate), with the `Instr` record's flow kinds grown by the delay slot: the builder appends the slot instruction to the branch's block and makes the edges leave after it, a likely branch's slot goes to the taken edge, and a jump through `jr` with a table before it resolves as a jump table; `ras disasm` prints a MIPS function.
        acceptance: `cargo test -p ra-asm-mips -p ra-asm-analysis -q` passes with the decoder agreeing with `llvm-objdump` on the `.text` of a clang-built MIPS fixture (`--target mipsel-linux-gnu`) and a CFG test of a loop whose branch has a slot.
        files: crates/ra-asm-arch/src/lib.rs crates/ra-asm-analysis/src/builder.rs crates/ra-asm-analysis/src/jumptable.rs crates/ra-asm-x86/tests/objdump.rs
        waits-on: 55
    - 81 ‹task› The MIPS lift and the o32 and n32 raise {c4} [defined]
        objective: The integer and FPU subset of MIPS I to IV lifted over the MIPS `%State` (the zero register as the constant, `hi`/`lo` as fields, the unaligned loads and stores `lwl`/`lwr`/`swl`/`swr` as the byte merges they are, the FPU condition flag, the 64-bit registers of MIPS III with the sign extension of every 32-bit result), and a `Target` for the o32 and n32 conventions (`a0` to `a3`, the stack's home slots, `v0`/`v1`, the `gp` as the globals pointer the raise resolves to the data global, the return address in `ra`); the fixture runs standalone through the IR interpreter.
        acceptance: `cargo test -q` passes with the control fixture built for mipsel running through the interpreter at every level with the expected output.
        files: crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/state.rs crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-lift/src/raise/abi.rs crates/ra-asm-lift/src/raise/stack.rs
        waits-on: 80 107
    - 82 ‹task› PlayStation: PSX-EXE and the R3000 {c4} [defined]
        objective: The PSX-EXE container (the 2 KB header, the load address, the initial `pc`, `gp` and `sp`), the R3000 without an FPU, the GTE as coprocessor 2 (its registers as state, its commands as calls on the runtime), the BIOS calls through the `A0`/`B0`/`C0` tables by their function numbers with names and prototypes from the known tables, and the memory map (RAM at `0x80000000` mirrored, the scratchpad, the hardware registers at `0x1f80xxxx` as device accesses the platform runtime answers); a homebrew PSX-EXE (built here with a maintained PSX toolchain, or checked in with its source) lifts without a trap.
        acceptance: `cargo test -q` passes with the PSX fixture lifted, its BIOS calls named, and run through the interpreter with the console output the emulator shows.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-pe/src/image.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/raise/prototypes.rs testdata/README.md
        waits-on: 81
    - 83 ‹task› PlayStation 2: the Emotion Engine {c5} [defined]
        objective: The PS2 ELF (32-bit MIPS big in registers, little in memory, loaded at `0x00100000`), the R5900's additions to MIPS III (the 128-bit registers and the multimedia instructions as 128-bit vector IR, `mult`/`madd` into `hi`/`lo` with their pipeline-1 twins, the `ldl`/`ldr` and quadword loads and stores, the FPU with no denormals and no NaNs so the port's float arithmetic goes through a flush-to-zero runtime, the COP2 macro instructions as calls into the VU0 state of task 84), the EE kernel's `syscall` numbers with names and prototypes, and the memory map (RAM, the scratchpad at `0x70000000`, the hardware at `0x10000000`, the uncached and accelerated mirrors folded onto one address); a PS2 homebrew ELF lifts without a trap and runs through the interpreter.
        acceptance: `cargo test -q` passes with a PS2 homebrew fixture (built here with the ps2sdk toolchain when installed, else checked in with its source) lifted with its syscalls named and run through the interpreter with the expected output.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/state.rs crates/ra-asm-lift/src/raise/prototypes.rs crates/ra-asm-ir/src/types.rs
        waits-on: 81
    - 84 ‹task› The vector units: VU0 and VU1 microcode {c5} [defined]
        objective: The VU micro instruction set as an `Arch` of its own: the 64-bit dual-issue word with an upper (the four-lane float arithmetic over 32 vector registers, the accumulator, the broadcast fields, the flags) and a lower instruction (integer, load and store, branches with delay slots, `xgkick`), lifted so an upper-lower pair is two IR statements in the order the hardware observes; the micro programs found where the EE uploads them (the `vif` unpack and `mpg` packets in data, the `vcallms` targets of macro mode); VU0 macro mode as calls on the same state; a VU1 program ports to a C function the platform runtime runs when the EE kicks it, with its output buffer as the GIF packet the renderer takes.
        acceptance: `cargo test -q` passes with a VU1 micro program (from a homebrew fixture) decoded against the reference assembler's listing, lifted and run through the interpreter with the transformed vertices expected.
        files: crates/ra-asm-arch/src/lib.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/data.rs crates/ra-asm-analysis/src/lib.rs
        waits-on: 83
    - 85 ‹task› PlayStation 2: the IOP and the modules {c4} [defined]
        objective: The I/O processor side: IRX modules (relocatable ELF with the `iopmod` section, imports and exports by library name and ordinal, the known names from the SDK and the open tables), the R3000 lift shared with the PlayStation, the SIF remote procedure calls between the EE and the IOP recognized as calls (the EE's `SifCallRpc` with its function number against the IOP module's registered table), so an EE program's calls into its IOP modules read as calls; the modules a game ships port beside its executable.
        acceptance: `cargo test -q` passes with a homebrew IRX lifted with its imports and exports named and an EE caller's RPC resolved to the module's function in `ras functions --json`.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-analysis/src/lib.rs crates/ra-asm-lift/src/raise/prototypes.rs apps/ras/src/functions.rs
        waits-on: 83 82
    - 86 ‹task› PSP: PRX and the Allegrex {c4} [defined]
        objective: The PRX container (a relocatable ELF with the PSP's relocation kinds and the `.rodata.sceModuleInfo`, the imports and exports as NIDs resolved to names from the SDK tables), the Allegrex additions to MIPS II (the bit operations, `min`/`max`, the VFPU as a 128-register 4×4 matrix unit lifted to vector IR with its prefixes as lane selects), the kernel and user mode syscalls, and the memory map; a homebrew PRX lifts without a trap.
        acceptance: `cargo test -q` passes with a PSP homebrew fixture lifted with its NIDs named and run through the interpreter with the expected output.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/state.rs crates/ra-asm-lift/src/raise/prototypes.rs
        waits-on: 81
    - 87 ‹task› Nintendo 64: ROMs, the VR4300 and the RSP {c4} [defined]
        objective: The ROM container (`.z64`, with `.n64` and `.v64` byte orders folded, the header's entry point and the boot code, the segments a game copies into RAM found by their DMA table where one exists, overlays as images of their own), the VR4300 (MIPS III, 64-bit registers with 32-bit code, the FPU, the TLB as identity), the libultra calls recognized by signatures from the known SDK builds, and the RSP microcode (the vector unit of the signal processor, an `Arch` like the VU) for the graphics and audio tasks the game uploads.
        acceptance: `cargo test -q` passes with a homebrew N64 ROM (built here with libdragon when installed, else checked in) lifted with its libultra or libdragon calls named and run through the interpreter with its console output.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-arch/src/lib.rs crates/ra-asm-analysis/src/lib.rs
        waits-on: 81
  - 88 ‹milestone› M: PowerPC and the rest {c4} [defined]
      objective: The architectures of the other consoles through the same IR: PowerPC for the GameCube, the Wii and the Xbox 360; SuperH for the Dreamcast and the Saturn; the original Xbox as the x86 lift under a new container; and, after enough of these, decoders generated from a machine description so an architecture costs a description and a `Target`, not a crate.
      acceptance: A GameCube homebrew DOL and a Dreamcast homebrew binary each lift without a trap and run through the interpreter with their expected output.
    - 89 ‹task› PowerPC: the decoder and the lift {c4} [defined]
        objective: The 32-bit PowerPC instruction set with the Gekko and Broadway paired-single extensions and the Xenon's 64-bit mode and VMX128, over the PowerPC `%State` (32 general registers, 32 float registers, the condition register's eight fields as separate flags, `lr`, `ctr`, `xer`), with the record forms writing `cr0`, the rotate-and-mask instructions as the shifts and masks they are, the paired singles as two-lane vectors, and the SysV and the Xbox 360 conventions as `Target`s.
        acceptance: `cargo test -q` passes with a clang-built PowerPC fixture (`--target powerpc-linux-gnu`) decoded against `llvm-objdump`, lifted and run through the interpreter at every level.
        files: crates/ra-asm-arch/src/lib.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/state.rs crates/ra-asm-lift/src/raise/mod.rs
        waits-on: 81
    - 90 ‹task› GameCube and Wii: DOL and REL {c4} [defined]
        objective: The DOL container (seven text and eleven data sections with their load addresses, the BSS, the entry point) and the REL relocatable modules (the section table, the import tables with their relocation lists against the DOL and other RELs, resolved into one image), the SDK and the Metrowerks runtime recognized by signatures, the hardware registers of the Flipper and Hollywood as device accesses, and the OS calls named; a homebrew DOL (devkitPPC) lifts without a trap.
        acceptance: `cargo test -q` passes with a homebrew DOL and a REL lifted with their OS calls named and run through the interpreter with the expected output.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-pe/src/image.rs crates/ra-asm-lift/src/raise/prototypes.rs testdata/README.md
        waits-on: 89
    - 91 ‹task› Xbox 360: XEX {c3} [defined]
        objective: The XEX2 container (the header's optional fields, the compressed and the encrypted basefile under the published keys, the PE inside, the import libraries by name and ordinal with the kernel's names from the known tables), the Xenon's 64-bit PowerPC with VMX128 over the lift of task 89, and the big-endian data model through the data analysis; a homebrew XEX lifts without a trap.
        acceptance: `cargo test -q` passes with a homebrew XEX unpacked, lifted with its kernel imports named and run through the interpreter with the expected output.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-pe/src/lib.rs crates/ra-asm-lift/src/data.rs crates/ra-asm-lift/src/raise/prototypes.rs
        waits-on: 89
    - 92 ‹task› The original Xbox: XBE {c2} [defined]
        objective: The XBE container (the header with its entry point and kernel thunk table XOR-encoded by the known keys, the sections, the library versions, the TLS), the kernel imports by ordinal with the names from the known table, and the x86 lift of today over it, so a homebrew XBE lifts and ports as a 32-bit x86 program over the platform runtime; the D3D and the SDK recognized by signatures.
        acceptance: `cargo test -q` passes with a homebrew XBE lifted with its kernel calls named and run through the interpreter with the expected output.
        files: crates/ra-asm-format/src/lib.rs crates/ra-asm-pe/src/image.rs crates/ra-asm-lift/src/raise/prototypes.rs testdata/README.md
        waits-on: 55 107
    - 93 ‹task› SuperH: Dreamcast and Saturn {c3} [defined]
        objective: The SH-4 (the 16-bit fixed-width encoding, the delay slots as in MIPS, the two register banks, the FPU with its single and double modes and the vector forms `fipr`/`ftrv`) and the SH-2 of the Saturn, over a SuperH `%State`; the Dreamcast's `1ST_READ.BIN` (a raw image at `0x8c010000`, with the scrambled form of a disc image unscrambled) and the KallistiOS and the SDK calls recognized by signatures; a homebrew program lifts without a trap.
        acceptance: `cargo test -q` passes with a Dreamcast homebrew fixture (KallistiOS, built here when installed, else checked in) decoded, lifted and run through the interpreter with its console output.
        files: crates/ra-asm-arch/src/lib.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-format/src/lib.rs crates/ra-asm-lift/src/raise/prototypes.rs
        waits-on: 81
    - 94 ‹task› Decoders from a machine description {c4} [defined]
        objective: After three decoders by hand (x86 through iced, MIPS, PowerPC), a description language of this repository's own (the fields of an instruction word, the formats, the mnemonics, the operand kinds, the flow, the registers read and written, and the lift of each instruction as IR builder calls), compiled by a build step into the decoder and the lift of an architecture, so SuperH, ARM32 and the next architecture cost a description file; the hand-written MIPS decoder is regenerated from a description and agrees with itself on every fixture.
        acceptance: `cargo test -q` passes with the MIPS decoder generated from its description agreeing with the hand-written one instruction for instruction over every MIPS fixture.
        files: crates/ra-asm-arch/src/lib.rs crates/ra-asm-lift/src/lib.rs Cargo.toml
        waits-on: 89 80
  - 95 ‹milestone› N: platform runtimes, a console program as a PC program {c5} [defined]
      objective: What makes the ports of milestones L and M run: the hardware a program touches (registers, DMA, the graphics and sound processors, the controllers, the discs and the memory cards) and the calls it makes (BIOS, kernel, SDK) become calls on a `Platform` interface of the port, with a runtime crate per machine, in C and in Rust, that implements the interface over the host's graphics, audio and input; a PlayStation 2 homebrew program, and then a game, runs as a native program of this machine with no emulator in the process.
      acceptance: A PlayStation 2 homebrew program that draws with the GS and reads the pad ports to a PC program that draws and reads the keyboard, built by the pipeline; a commercial PS2 game's title screen renders through its own ported code.
      notes: The runtime is where emulation knowledge goes, but the shape is not an emulator: the program's code runs natively as the port, and only the devices are implemented. The graphics processors (the GS, the PS1 GPU, the N64 RDP, the Flipper) are command-stream machines, so each is a decoder of its command format and a renderer over one graphics abstraction of the runtime; the vector and signal processors (VU, RSP) are lifted as code by their architecture cards.
    - 96 ‹task› The platform interface and the device accesses {c5} [defined]
        objective: The data analysis and the lift classify every access to a machine's hardware range (by the memory map the container's `Target` declares) as a device access: a load or store at a named register becomes a call on the `Platform` interface (`gs_write(reg, value)`, `pad_read(port)`) and a DMA kick becomes the transfer it describes, with the register names and the packet formats from the machine's description; a BIOS, kernel or SDK call recognized by task 82, 83 or 103 becomes a call on the same interface; the interface is a C header and a Rust trait the port's `main` binds to a runtime; `ras emit` writes a stub runtime that logs every call, which is what the interpreter and the oracle run against.
        acceptance: `cargo test -q` passes with the PS2 homebrew fixture ported to C over the stub runtime, its device calls logged in the order the emulator's trace shows.
        files: crates/ra-asm-lift/src/data.rs crates/ra-asm-lift/src/lib.rs runtime/rhipper.h runtime/rhipper.rs apps/ras/src/emit.rs
        waits-on: 83 108
    - 97 ‹task› Graphics: the command streams rendered {c5} [defined]
        objective: The runtime's renderer over one graphics abstraction (wgpu in Rust, with a C binding for the C ports): the PS2 GS register and GIF packet stream (primitives, textures from the GS memory model with its swizzles, the frame buffer as a texture, the alpha and depth tests), then the PS1 GPU commands, the N64 RDP display lists (with the RSP microcode run lifted by task 87), and the GameCube's Flipper command processor; each as a crate of the runtime behind the `Platform` interface, with a headless mode that writes frames to files for the oracle.
        acceptance: `cargo test -q` passes with the PS2 homebrew fixture's first frame rendered headless and compared by image to the emulator's frame within a tolerance.
        files: runtime/rhipper.rs runtime/rhipper.h apps/ras/templates/CMakeLists.txt crates/ra-asm-emit/src/rust.rs
        waits-on: 96 84
    - 98 ‹task› Audio, input, storage {c3} [defined]
        objective: The SPU and SPU2 (ADPCM voices, the reverb, the mixing to a host audio stream), the N64's audio interface, the controllers (the PS1 and PS2 pads, the N64 and GameCube controllers) mapped to the host's gamepads and keyboard, the memory cards and the disc (a game's files read from an ISO or a directory) behind the `Platform` interface, each with a headless mode for the oracle.
        acceptance: `cargo test -q` passes with a homebrew fixture that plays a sound and reads the pad producing the expected samples and reacting to a scripted input in the headless runtime.
        files: runtime/rhipper.rs runtime/rhipper.h
        waits-on: 96
    - 99 ‹task› Threads, interrupts and DMA ordering {c4} [defined]
        objective: The EE kernel's threads, semaphores and alarms as host threads and primitives of the runtime; the interrupt handlers a program registers (VBlank, DMA completion, timers) called by the runtime at the moments the devices complete, with the ordering the program relies on (a DMA that completes before the next frame's kick) preserved by the runtime's scheduler; the `sync` and cache instructions as the barriers they imply.
        acceptance: `cargo test -q` passes with a homebrew fixture that uses two threads and a VBlank handler running through the runtime with the expected output over ten frames.
        files: runtime/rhipper.rs runtime/driver.c crates/ra-asm-lift/src/lib.rs
        waits-on: 96
    - 100 ‹task› The game, not the homebrew {c5} [defined]
        objective: A commercial PlayStation 2 game ports to its title screen: the SDK libraries recognized by signatures (task 103) and implemented by the runtime where the lifted versions would need hardware timing (the `sce` graphics, pad, sound and file libraries), the IOP modules the game ships ported beside it (task 85), the game's own VU programs lifted (task 84), the disc read from its ISO, and the pipeline building and running it as one project; what the port cannot do yet is reported by function and by device, in the function table, not by a crash.
        acceptance: `cargo test -q --ignored` passes with the person's own PS2 game (named by a local path the test reads from an environment variable, skipped when absent) building and rendering its first frame headless.
        files: apps/ras/src/pipeline.rs apps/ras/src/project.rs runtime/rhipper.rs crates/ra-asm-emit/src/functions.rs
        waits-on: 97 98 99 85 103
    - 101 ‹task› The browser {c3} [defined]
        objective: A platform-neutral port (a console program over milestone N, or a program with no Win32) builds with Emscripten to WebAssembly, with the runtime's renderer over WebGL or WebGPU and its audio over WebAudio, so a ported game runs in a browser tab; the pipeline gains a `wasm` host and the workbench a "run in browser" step.
        acceptance: `cargo test -q` passes with the PS2 homebrew fixture built to WebAssembly (when Emscripten is on the PATH, else skipped) and its first frame rendered in a headless browser matching the native frame.
        files: apps/ras/src/pipeline.rs apps/ras/templates/CMakeLists.txt runtime/rhipper.rs runtime/rhipper.h
        waits-on: 97
  - 102 ‹milestone› O: signatures and libraries {c4} [defined]
      objective: Library code recognized before the lift, so a program's own code is what gets ported and the libraries are called by name: signatures generated from the libraries themselves (the MSVC C runtimes by version, the Windows SDK, the PS2 and PSP SDKs, libultra, the console toolchains' runtimes), the toolset of a binary fingerprinted, and a store of signatures shared across projects and published with the releases.
      acceptance: A VS2010 `/MT` program and a PS2 game both lift with their runtime and SDK functions named and not ported, with under one percent of the library functions unrecognized.
    - 103 ‹task› Library recognition from the libraries {c4} [defined]
        objective: Signatures generated from `.lib`, `.a` and `.obj` files (COFF and ELF object archives, the relocations masked out of the bytes, the called names as the second-level tiebreak, as FLIRT does but in this repository's own format), applied to an image's functions before the lift: a match names the function, gives it the prototype the library's headers say when the header table has it, and marks its body as not ported; `ras signatures` reports the matches and the collisions.
        acceptance: `cargo test -q` passes with signatures generated from the MSVC 14.51 `libcmt.lib` recognizing the CRT of the rhtest `-O2` programs' static builds, and the VS2010 `core` build's CRT functions all named.
        files: crates/ra-asm-analysis/src/lib.rs crates/ra-asm-lift/src/raise/prototypes.rs apps/ras/src/signatures.rs crates/ra-asm-pe/src/lib.rs
        waits-on: 46 56
    - 104 ‹task› Toolset fingerprinting {c2} [defined]
        objective: The compiler and linker of a binary read from the rich header of a PE (the product ids and builds), the CRT startup's shape, the padding bytes, the section names and the SDK strings; the fingerprint selects the prototype table, the idiom set, the signature libraries and the C++ ABI details the lift uses, and `ras info` prints it.
        acceptance: `cargo test -p ra-asm-pe -p ras -q` passes with the fixtures' toolsets named (clang/lld, MSVC 14.51, MSVC 16.00) from their binaries alone.
        files: crates/ra-asm-pe/src/headers.rs crates/ra-asm-analysis/src/lib.rs apps/ras/src/report.rs
    - 105 ‹task› The signature store {c2} [defined]
        objective: Signatures live outside the repository as versioned files under the user's `.rhipper/signatures` (and a project's own under `rhipper.json`), generated by `ras signatures --build <lib>…` and published with the releases for the toolsets the person does not have; a project's fingerprint picks the files, and the workbench shows which are loaded and offers to generate the missing ones from an installed SDK.
        acceptance: `cargo test -p ras -q` passes with a store built from a fixture library, saved, reloaded and applied with the same matches.
        files: apps/ras/src/signatures.rs apps/ras/src/project.rs .github/workflows/release.yml
        waits-on: 103
  - 106 ‹milestone› P: the IR interpreter and dynamic analysis {c5} [defined]
      objective: The machine IR runs directly: an interpreter of this repository's own with a memory model, the imports bound to the host or to stubs, and a trace; it proves a lift without clang or a matching host, runs the console architectures on this machine, feeds the static analysis the facts only execution gives, and is the engine of the trace comparison and the unpacker.
      acceptance: Every fixture's oracle passes through the interpreter as it does through clang, and the ARM64 and MIPS fixtures pass through it alone.
    - 107 ‹task› The IR interpreter {c4} [defined]
        objective: An interpreter over the IR crate's modules: values of every type the IR has, the instruction subset, a flat memory of pages (the image mapped at its base, a stack, a heap for the imports that allocate), calls between lifted functions and to the imports through a binding table (the host's own DLLs on Windows by the existing adapters, stubs elsewhere), traps reported with the address, and a run limit; `ras run <exe>` runs a program through it and prints what it prints.
        acceptance: `cargo test -q` passes with every fixture (64 and 32, every level) printing its expected output through the interpreter, and `ras run testdata/bin/control64.exe` matching `testdata/expected/control.txt`.
        files: crates/ra-asm-ir/src/lib.rs crates/ra-asm-ir/src/module.rs crates/ra-asm-lift/src/lib.rs apps/ras/src/main.rs crates/ra-asm-lift/tests/oracle.rs
        waits-on: 55
    - 108 ‹task› The instruction oracle {c3} [defined]
        objective: Each x86 instruction form the lift covers is checked against the processor: a generator produces instruction encodings with random operands and random register and memory states, a harness runs each natively (in a guarded page, the faults caught) and through the lift and the interpreter, and a difference reports the instruction, the input state and the field that differs (the flags by name); every arm of the lift's match has at least one form in the generator, and the oracle runs in the suite with a fixed seed and nightly with a random one.
        acceptance: `cargo test -p ra-asm-lift -q` passes with the instruction oracle over every integer and SSE form at the fixed seed.
        files: crates/ra-asm-lift/tests/oracle.rs crates/ra-asm-lift/src/x86.rs crates/ra-asm-lift/src/x87.rs crates/ra-asm-x86/src/decode.rs
        waits-on: 107
    - 109 ‹task› Trace-driven facts {c4} [defined]
        objective: The interpreter's trace of a run (the targets of every indirect call and jump, the vtables stored, the values of the registers at each call) becomes facts the static analysis takes on the next pass: indirect calls resolved to the functions seen, jump tables bounded by the indices seen, functions discovered that no seed reached, and signatures confirmed by the arguments observed; the facts are kept under `build/` with the input that produced them and shown in the workbench as "seen at run time".
        acceptance: `cargo test -q` passes with a fixture whose function-pointer dispatch is resolved by a traced run and whose port then reads as direct calls.
        files: crates/ra-asm-analysis/src/lib.rs crates/ra-asm-lift/src/lib.rs apps/ras/src/pipeline.rs crates/ra-asm-emit/src/functions.rs
        waits-on: 107
    - 53 ‹task› Packed and protected binaries {c2} [defined]
        objective: Recognize the common packers by their entry section and import table, and dump the unpacked image through the interpreter's run to the original entry point, with the imports it resolved at run time as the dumped image's import table, so the pipeline starts from the real program.
        acceptance: `cargo test -q` passes with a UPX-packed fixture that ports as its unpacked program.
        files: crates/ra-asm-pe/src/image.rs crates/ra-asm-analysis/src/lib.rs apps/ras/src/pipeline.rs
        waits-on: 107
    - 110 ‹task› Record and replay {c4} [defined]
        objective: The trace of task 51 recorded two ways, by the interpreter over the reference's IR and by a native tracer (the driver's `RA_ENTER` hooks grown to the arguments and the results, or an emulator's trace for a console program), replayed over the port function by function in the workbench, with the first divergence shown as the function, the call and the value that differs; the replay drives the side-by-side stepping of the workbench.
        acceptance: `cargo test -p ras -q` passes with a recorded trace of control64 replayed over a port with one function changed, naming the function and the value.
        files: apps/ras/src/serve.rs apps/ras/src/pipeline.rs runtime/driver.c crates/ra-asm-ir/src/lib.rs
        waits-on: 51 107
    - 111 ‹task› Obfuscation undone {c3} [defined]
        objective: Opaque predicates evaluated by the known-values pass and the interpreter, control-flow flattening (a dispatcher loop over a state variable) rebuilt into the original graph by following the state assignments, strings decrypted at lift time by running their decryptor in the interpreter and replacing the ciphertext with the literal, and the stubs of the common protectors recognized; each as a pass with a fixture obfuscated by a known tool.
        acceptance: `cargo test -q` passes with a flattened fixture reading as its original structure and its encrypted strings as literals.
        files: crates/ra-asm-lift/src/raise/known.rs crates/ra-asm-emit/src/structure.rs crates/ra-asm-analysis/src/lib.rs crates/ra-asm-lift/src/data.rs
        waits-on: 107 47
  - 112 ‹milestone› Q: decompilation depth {c5} [defined]
      objective: Past the C that is correct: the C++ a program was written in, reconstructed from what the binary keeps (classes, inheritance, containers, exceptions), the enumerations and constants named, the compiler's work undone where it hides the source, and floating point exact to the last bit.
      acceptance: The rhtest `cpp`, `stl` and `eh` programs port to C++ whose classes, containers and exceptions read as the sources' do, and run.
    - 113 ‹task› Classes reconstructed {c5} [defined]
        objective: A class model from the binary: the vtable each constructor stores and the base constructors it calls give the hierarchy, the offsets `this` is accessed at give the fields, the functions that take the same `this` type are its methods, the destructors by the vtable stores in reverse, the `new`/`delete` pairs the allocation sizes; emitted by the C++ backend (task 29) as `class` definitions with their methods, the virtual ones `override`, the fields typed by task 45.
        acceptance: `cargo test -p ra-asm-emit -q` passes with the rhtest `cpp` program emitted as classes whose methods and fields match the source's, and running.
        files: crates/ra-asm-analysis/src/rtti.rs crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/src/lib.rs
        waits-on: 29 64 45
    - 114 ‹task› The STL and the templates {c4} [defined]
        objective: The inlined shapes of a toolset's standard library recognized as the containers they are: `std::string`'s small-buffer and heap forms, `std::vector`'s three pointers, `std::map`'s tree nodes, the iterators and the algorithms, each by the toolset's version (MSVC's and libstdc++'s differ), so a function over a vector reads as one and the port includes the headers instead of the inlined code; the recognizer is driven by signatures (task 103) and shapes, per toolset.
        acceptance: `cargo test -p ra-asm-emit -q` passes with the rhtest `stl` program's containers read as `std::` types in its C++ port, which runs.
        files: crates/ra-asm-lift/src/raise/simplify.rs crates/ra-asm-analysis/src/lib.rs crates/ra-asm-emit/src/c.rs crates/ra-asm-lift/src/raise/prototypes.rs
        waits-on: 113 103
    - 115 ‹task› Enumerations, flags and named constants {c2} [defined]
        objective: A switch's case values and a value compared against a small set become an `enum`; a value only tested by masks becomes flags; a constant passed to a known API in a parameter of a known enumeration takes its name from the SDK (window messages, error codes, `GENERIC_READ`), with the tables generated from the headers beside the prototypes of task 46; the names reach the C and the workbench.
        acceptance: `cargo test -q` passes with the control fixture's switch read over an `enum` and a fixture's `CreateFileA` call reading with the SDK's names.
        files: crates/ra-asm-lift/src/raise/known.rs crates/ra-asm-lift/src/raise/prototypes.rs crates/ra-asm-emit/src/c.rs
        waits-on: 45
    - 116 ‹task› Inlining and splitting undone {c3} [defined]
        objective: A code shape repeated across functions (the same statements over different values, as an inlined helper is) is outlined as a function with the parameters the differences imply and a name the person can change; a function split by the compiler into hot and cold halves, or a loop peeled and unrolled, is rejoined where the structuring can show it as one; each as a pass over the raised IR with the oracle green.
        acceptance: `cargo test -q` passes with a fixture built at `-O2` with an inlined helper reading as a call to one function in the C.
        files: crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-emit/src/structure.rs crates/ra-asm-analysis/src/lib.rs
        waits-on: 47
    - 117 ‹task› Floating point to the last bit {c3} [defined]
        objective: The port's floating point matches the reference bit for bit: the x87 at 80 bits (task 58) where the program used it, the denormal and rounding modes of `mxcsr` honoured by the runtime, the fused multiply-adds where the hardware fused them, the transcendental functions of the CRT by their version (the results of `sin` differ between CRTs), and an FP oracle over a numeric corpus of the rhtest `float` and `fpenv` programs at every optimization level.
        acceptance: `cargo test -q` passes with the rhtest `float` and `fpenv` programs printing every digit of the reference's output.
        files: crates/ra-asm-lift/src/x87.rs crates/ra-asm-lift/src/x86.rs runtime/rhipper.h runtime/rhipper.rs
        waits-on: 58 55
  - 118 ‹milestone› R: the port as a codebase {c4} [defined]
      objective: The output of the engine looks like a project a team wrote: the original's module boundaries and names, idiomatic and lint-clean C, C++ and Rust, the build systems people use, Rust that is safe where the engine can prove it, and a round trip that keeps a person's edits across regeneration.
      acceptance: A ported program opens in the workbench as a tree of modules with headers, passes `-Wall -Wextra` and clippy, and keeps every hand-written function across a regeneration from a new build of the reference.
    - 119 ‹task› Module boundaries {c3} [defined]
        objective: The sources split by the object files a map or a PDB names (each becomes a `.c`/`.cpp` with its header and the originals' names), and, without either, by the clusters the call graph and the string references suggest; the data likewise, by the module that owns each global; the function table and the workbench show the tree.
        acceptance: `cargo test -p ra-asm-emit -q` passes with the rhtest `core` port split into the map's object files, each building.
        files: crates/ra-asm-emit/src/lib.rs crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/src/functions.rs apps/ras/src/emit.rs
        waits-on: 61
    - 120 ‹task› Safe Rust where it is provable {c4} [defined]
        objective: The Rust output shrinks its `unsafe`: an array with known bounds (task 45) becomes a slice and its accesses indexed, a pointer that never escapes and is never null becomes a reference, a nullable one an `Option`, a local struct a value, a string literal a `&str`; what cannot be proven stays raw behind an `unsafe` block that names why in a comment; the oracle stays green and clippy is clean.
        acceptance: `cargo test -p ra-asm-emit -q` passes with the control fixture's Rust port under half its unsafe blocks and clippy-clean.
        files: crates/ra-asm-emit/src/rust.rs crates/ra-asm-lift/src/raise/mod.rs runtime/rhipper.rs
        waits-on: 45
    - 121 ‹task› Formatting, lint and the readability grade {c2} [defined]
        objective: The written sources pass through `clang-format` and `rustfmt` when on the PATH, build under `-Wall -Wextra -Werror` and `cargo clippy` with no warning, and the function table carries a readability grade per function (gotos, casts, unresolved calls, raw stack accesses, lines) the workbench shows beside the trap count, so the readability work has a number to move.
        acceptance: `cargo test -p ra-asm-emit -q` passes with every fixture's port warning-free under the strict flags and graded in `functions.json`.
        files: crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/src/rust.rs crates/ra-asm-emit/src/functions.rs apps/ras/src/emit.rs
    - 122 ‹task› Build systems and cross builds {c2} [defined]
        objective: Beside the CMake and Cargo projects: a Visual Studio solution for a C++ port, a Meson file on request, the host's clang on Linux and macOS for a platform-neutral port, and a cross build from Windows to the WebAssembly host of task 101; `rhipper.json` names the generator and the pipeline drives each.
        acceptance: `cargo test -p ras -q` passes with the control fixture built through the Visual Studio generator when `msbuild` is on the PATH and through CMake otherwise.
        files: apps/ras/src/pipeline.rs apps/ras/templates/CMakeLists.txt apps/ras/src/project.rs
        waits-on: 72
    - 123 ‹task› The round trip {c3} [defined]
        objective: A regeneration from a new build of the reference (a patch, a different optimization level) keeps the person's work: the hand-written functions (task 48) stay, the names, types and comments they attached (task 125) reattach by function identity (the signature and the call graph position, not the address), and the diff of the regenerated functions is shown so the person sees what moved.
        acceptance: `cargo test -p ras -q` passes with a port of control64 regenerated from the `-O2` build keeping a hand-written function and its names.
        files: apps/ras/src/emit.rs apps/ras/src/project.rs crates/ra-asm-emit/src/functions.rs crates/ra-asm-analysis/src/lib.rs
        waits-on: 48 125
  - 124 ‹milestone› S: the workbench end game {c5} [defined]
      objective: What the workbench becomes on top of the engine: binary diffing, annotations as a shared project, plugins over the engine's API, the AI tools that answer from the facts and the runs, and live debugging of the port beside the reference.
      acceptance: Two versions of a game are diffed function by function, a team's names merge through git, and a function is stepped in the port and the reference in lockstep from the workbench.
    - 125 ‹task› Annotations as a project {c3} [defined]
        objective: Names, types, comments, takeovers and the person's notes per function live in sidecar files under the project (`rhipper/annotations/*.json`, keyed by function identity), under git with the sources, read by `ras emit` and `ras serve` and written by the workbench's rename and retype actions; two people's files merge as text, and a conflict is a function, not a file.
        acceptance: `cargo test -p ras -q` passes with a rename through `ras serve` landing in the sidecar and in the next emit.
        files: apps/ras/src/serve.rs apps/ras/src/project.rs apps/ras/src/emit.rs crates/ra-asm-emit/src/functions.rs
        waits-on: 48
    - 126 ‹task› Binary diffing {c4} [defined]
        objective: Two images matched function by function (the names where both have them, else the structure of the CFG, the called functions, the strings and the constants, in rounds that use the matches of the last), with the changed functions ranked by how much changed and their raised IR diffed; `ras diff a.exe b.exe` prints the report and `ras serve` answers it for the workbench's diff view; a patch to a game is explained as the functions it touched.
        acceptance: `cargo test -p ras -q` passes with the `-O2` and `-Od` builds of rhtest `core` matched by name-free structure for every function the maps agree on.
        files: apps/ras/src/main.rs apps/ras/src/serve.rs crates/ra-asm-analysis/src/lib.rs crates/ra-asm-ir/src/lib.rs
        waits-on: 103
    - 127 ‹task› Plugins over the engine's API {c4} [defined]
        objective: The engine's API (the image, the functions, the IR, the facts, the emit) exposed to WebAssembly plugins with a stable interface: a plugin can add a name source, a raise pass, a signature library, an output language or a device model for the platform runtime, loaded by `ras` from the project or the user's plugin directory and listed by the workbench; the first plugin is one of the engine's own passes moved out, as the proof.
        acceptance: `cargo test -p ras -q` passes with a plugin that names functions from a text file loading and its names reaching `functions.json`.
        files: apps/ras/src/main.rs crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-analysis/src/lib.rs
        waits-on: 107
    - 128 ‹task› The AI tools over the facts {c3} [defined]
        objective: The MCP tools the workbench gives a model (and `ras serve` behind them) answer from everything the engine knows: the facts of the analysis, the raised IR, the signatures, the traced values of task 109, the diff of task 126; and gain the questions a model asks in a port ("what does this function do", "find the function that writes this global", "name this from its callers and strings", "which functions trap and why"), each a composition of the engine's answers with no model call inside `ras`.
        acceptance: `cargo test -p ras -q` passes with the new `serve` methods answered on a fixture and the workbench's tool list reading them.
        files: apps/ras/src/serve.rs apps/ras/src/xrefs.rs apps/ras/src/functions.rs
        waits-on: 109
    - 129 ‹task› Live debugging in lockstep {c5} [defined]
        objective: The port under the host's debugger (the workbench's CLion-derived debugger over LLDB) and the reference under an emulator with a debug protocol (PCSX2, DuckStation, or the native tracer for a Windows program), stepped in lockstep by function from the recorded trace of task 110, with the registers and memory of both shown side by side and the divergence stopped at; the workbench's end-game view.
        acceptance: `cargo test -p ras -q` passes with the lockstep protocol driving the interpreter and a port of control64 to the same breakpoint with equal states.
        files: apps/ras/src/serve.rs apps/ras/src/pipeline.rs runtime/driver.c
        waits-on: 110
  - 130 ‹milestone› T: scale and release {c4} [defined]
      objective: The engine at the size of real programs and on every host: incremental and parallel, bounded in memory, a smoke corpus that catches a crash before a person does, the loaders fuzzed, and the releases for Windows, Linux and macOS the workbench installs.
      acceptance: A binary of a hundred thousand functions analyzes, lifts and emits in under a minute on this machine, and a nightly run over the smoke corpus is green.
    - 52 ‹task› Incremental and parallel {c3} [defined]
        objective: The analysis cached under `build/`, the lift and the emit of only the functions a change touched, and the lift of independent functions in parallel, so a program of tens of thousands of functions turns around in seconds.
        acceptance: `cargo test -p ras -q` passes, a second `ras pipeline` run on an unchanged program finishes in under a second per step, and a large program lifts on every core.
        files: apps/ras/src/pipeline.rs crates/ra-asm-lift/src/lib.rs crates/ra-asm-analysis/src/lib.rs
    - 131 ‹task› Memory for the large {c3} [defined]
        objective: The IR of a function is lifted, raised, emitted and dropped before the next, with the cross-function facts (signatures, widths, known values) kept in compact tables and the data globals shared; a function's IR is spilled to `build/` when the workbench asks for it later; a 100k-function binary runs the pipeline in under 4 GB.
        acceptance: `cargo test -p ras -q --ignored` passes with a large local binary (named by an environment variable, skipped when absent) through the pipeline under the memory bound, measured by the test.
        files: crates/ra-asm-lift/src/lib.rs crates/ra-asm-emit/src/lib.rs apps/ras/src/pipeline.rs
        waits-on: 52
    - 132 ‹task› The smoke corpus {c2} [defined]
        objective: `ras smoke <dir>` runs the analysis, the lift and the emit over every executable and library under a directory (`system32`, a games folder) with a time limit each, never crashes, and writes a report of the coverage (task 60) summed, the traps by mnemonic, the slowest files and the panics with their backtraces; run nightly by the release workflow over a corpus the runner has and locally by the person over theirs.
        acceptance: `cargo test -p ras -q --ignored` passes with `ras smoke` over the Windows system directory's executables finishing with no panic.
        files: apps/ras/src/main.rs apps/ras/src/lift.rs .github/workflows/release.yml
        waits-on: 60
    - 133 ‹task› Fuzzing the loaders and the decoders {c2} [defined]
        objective: `cargo fuzz` targets over the PE, ELF, Mach-O and console containers and over each decoder, with a corpus seeded from the fixtures, run by a workflow on a schedule and locally on request; every refusal is a message with an offset, never a panic, and a found case becomes a regression test.
        acceptance: `cargo test -p ra-asm-pe -p ra-asm-x86 -q` passes with the regression tests of the first fuzzing run, and the fuzz targets build.
        files: crates/ra-asm-pe/src/lib.rs crates/ra-asm-format/src/reader.rs crates/ra-asm-x86/src/decode.rs .github/workflows/release.yml
