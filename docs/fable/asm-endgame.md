# Rhipper Asm: the endgame

Written 2026-11-13 against main at `2a0eda3`, from `rhipper.context.md`, the roadmap `rhipper.md` (its milestones, open cards and their notes) and the code of the eleven crates, `apps/ras` and `runtime/`.

Part 1 is the destination. Part 2 is the architecture that reaches it from today's crates. Part 3 is how the agent builds it without rerunning most of its runs. Part 4 is the road: eleven phases, then the cards of the first three as an outline for `rc import`, and the later phases as milestones and sections.

The endgame in one sentence:

> Any binary goes in. What comes out is a codebase a team could have written: it builds, it runs, it behaves as the original, every function carries the evidence for that claim, and beside it lives a model of the program that a person and a model keep working in until the port is theirs.

Three properties, which no tool holds together today:

1. **It runs from the first hour**, as a static recompilation does.
2. **It reads like source by the end**, as a hand decompilation does.
3. **It knows at every moment which function is at which stage and why it believes so**, as nothing does.

Everything below follows from keeping all three at once. Gambit g1 (exact or it stops) is the seed of the third property and stays the first rule: asm never guesses silently. What changes at the endgame is that "stops" is replaced by "falls back and says so": a function asm cannot raise still runs at a lower level, and the model records the reason.

---

## Part 1. The endgame

### 1.1 A session, years out

A person has a PlayStation 2 game from 2002 on a disc they own, and wants it as a program on their machine that they can read and change.

```
ras new game.iso --out ports/game
```

Asm opens the disc as a tree of files. It finds the boot executable (an ELF for the Emotion Engine), eleven IOP modules, and forty overlays the game loads by name. It identifies the compiler of each (ee-gcc 2.95.2 for the game, a Sony build for the libraries), matches 2 300 library functions against the signature store by version, and recognizes two middleware packages. It lifts every function it can reach, raises each as far as its checks allow, and writes a project.

Twelve minutes later there is a directory with C sources split along the original object files' seams, a build file, the PS2 platform runtime linked in, and a status:

```
functions     14 212    lifted 14 212    trapping 0    residual 37
evidence      proved 9 811    tested 2 950    traced 604    unverified 847
assumptions   31 440 discharged    1 207 checked at run time    12 open
source        gotos 0.8/kloc    casts 41/kloc    named 22%    typed 38%
memory        guest-flat 100%    liberated 0%
```

`cmake --build` produces an executable. It boots to the title screen and plays. This is the *faithful port*: the program's own memory layout, its own 32-bit pointers, its own byte order, its devices answered by the platform runtime. It is ugly in places and correct everywhere it has evidence, and the status says where it has none.

Then the port converges. Four things move it, and they can all run at once.

**Playing it.** The first build is a recording build. Every function entered records a few of its entry states and effects (a *function vector*, part 3). Every assumption asm made is an assertion in that build. Code that was reached only at run time (a computed jump the analysis could not bound, a module loaded late, code the game wrote into memory) runs through the *residual engine*, a lifter and interpreter linked into the port, and is recorded. An hour of play turns `traced 604, unverified 847` into `tested`, discharges assumptions by observation, and gives the next static build the 37 residual functions.

**The agents.** `ras cards` writes the port's own roadmap in the format `rc import` reads: one card per function or cluster of functions, bottom-up over the call graph, each with the objective "name, type and rewrite `F_0012a4c0` as its author would have" and the acceptance `ras verify F_0012a4c0`, which replays the function's vectors against the rewritten source and prints one line. A model proposes; the oracle decides. A proposal that changes behaviour is rejected by a machine, in seconds, without a person reading it. The same walk that builds asm builds the port.

**The person.** They open the workbench, see a function the agents left awkward, and take it over by hand: rename, retype, rewrite. Their version is kept as long as it passes the function's oracle; if a later change to a callee breaks it, they are told which vector differs. They can also go the other way: write a struct declaration for an object they understand, and watch four hundred functions re-raise against it in a second.

**Other builds of the same program.** The European release has different addresses and 3% different code. The demo disc shipped with debug symbols. A PC version of the same engine exists. `ras relate` matches functions across them by content and structure and carries names, types and hand-written functions across, each re-verified against its own binary.

Months later the status reads:

```
functions     14 249    proved 13 902    tested 347    unverified 0
source        gotos 0/kloc    casts 2/kloc    named 100%    typed 100%
memory        guest-flat 4% (the display-list builder)    liberated 96%
matching      11 407 functions compile to the original bytes with ee-gcc 2.95.2
```

The codebase now uses host pointers and host structs, builds for three operating systems, has a test suite generated from the recorded sessions, and the 4% that still sees guest memory is listed by name. It is no longer a port of a binary. It is the program's source, with a proof of where it came from.

The same session works for a 1996 DOS game, a Windows XP-era PE (today's ground and gambit g7's goal), an Xbox 360 XEX, an Android native library, a router's firmware, and a Game Boy cartridge. What differs between them is data asm loads, not code asm forks.

### 1.2 What goes in

Any binary of any platform and era. Asm reaches that by four plug-in axes, each of which is mostly data:

| Axis | Answers | Examples |
|---|---|---|
| **Container** | which bytes live at which addresses, when | PE, ELF, Mach-O, XBE, XEX, DOL/REL, PSX-EXE, PRX, N64 ROM, NSO, MZ/NE/LE, COFF and a.out objects, raw blob with a memory map |
| **Machine** | what the bytes do | x86 (16/32/64), ARM (A64, A32, Thumb), MIPS (I to IV, R5900, Allegrex), PowerPC (Gekko, Broadway, Xenon, Cell), SH-2/SH-4, 68000, Z80/SM83, 6502/65816, RISC-V, the vector units and DSPs that sit beside them |
| **Platform** | what the world outside the program is | Win32, Linux, macOS, Android, iOS, DOS, each console's kernel, SDK libraries and hardware, a microcontroller's peripherals |
| **Toolchain pack** | what the compiler and language left behind | MSVC 6 to 2022, GCC 2.95 to today, Clang, CodeWarrior, SN, Borland, Watcom; C++ RTTI and exception tables, Objective-C and Swift metadata, Go's pclntab, Rust's panic strings and vtables, Delphi's RTTI, IL2CPP metadata |

What "any" means concretely:

- **A medium, not a file.** A disc image, a cartridge, a firmware update, an APK or an installer is a tree of files, some of which are images. Asm walks the tree, loads every image, and links them into one program: the executable, its dynamic libraries, its overlays and late-loaded modules.
- **Address spaces with time.** Overlays and bank switching mean the same address holds different code at different moments. An image is a set of segments, each in an address space and an *epoch* (a bank, an overlay slot's occupant). Functions are identified by segment and offset, never by address alone.
- **Code without a compiler.** The 8- and 16-bit machines and much of the 32-bit console era hold hand-written assembly: no calling convention, flags as return values, jumps into the middle of functions, tables of code and data interleaved. Asm infers a convention per function from its uses instead of assuming a platform ABI, and accepts functions with several entries.
- **No headers at all.** For a firmware blob asm infers the architecture from instruction statistics, the load address from absolute pointers and the vector table, and the memory map from accesses, and states its confidence.
- **Packed and transformed code.** A program that unpacks itself is run in the interpreter until it has; the unpacked image is the input. A protector is recognized and named. Asm works on what the person gives it and what that program does on its own when run; it carries no keys and bypasses no online check.
- **Managed code, honestly scoped.** For pure bytecode (JVM, CIL, Dalvik) good decompilers exist and asm does not compete. Its value is the mixed program: the native half of an IL2CPP game with its metadata, the JNI library of an app, the AOT image.

### 1.3 What comes out

**A codebase.** Not pseudo-code for reading: a project.

- Source in C, C++ or Rust, chosen per project and changeable later, in the idiom of the language: `for` loops, `switch`, structs with named fields, enums, classes with virtual methods where the binary had them, `std::string` where the binary used it, `Result` and slices in Rust where the proof allows.
- Files and directories along the original translation units where they can be recovered (from link order, from static functions' locality, from debug paths left in strings), along call-graph clusters otherwise.
- Headers with the types and the prototypes, one definition each.
- Names from every source there is: symbols, exports, RTTI, assertion strings, format strings, API usage, sibling builds, the signature store, and for the rest a model's proposal marked as such.
- A build that works on a clean machine (CMake or Cargo), with the platform runtime as a library dependency.
- A test suite: the recorded sessions as end-to-end tests and the function vectors as unit tests, so that a person who changes the port has what the original team had, or more.
- Comments where evidence exists and is useful: "matches `inflate_fast` of zlib 1.1.3", "proved under: no caller passes overlapping buffers".

**Behaviour equal to the original**, with the claim made per function and per level, not once for the whole program. Part 1.6 says what the claim rests on.

**A living model.** The program database is the product as much as the source is:

- Every byte of the input is classified (code of function F, data object D of type T, padding, unknown) with the provenance of that classification.
- Every function exists at every level it reached, from machine semantics to source, with the evidence between each pair of levels.
- Every fact has a source: which analysis, which trace, which person, which model, and what it depends on. Retracting a fact re-derives what stood on it.
- Recorded sessions, coverage, value profiles and heap shapes from running the program.
- The person's and the models' annotations as text files that merge in git.
- A query interface (`ras serve`, versioned, also offered as an MCP server) fast enough to sit behind an editor: what calls this, what writes this field, what is the type of this argument at this call, why does asm believe this is a jump table, what would change if this were a `Vec3`.

### 1.4 How a person and a model work with it

The division of labour is fixed by what each is good at:

- **Asm does what can be decided**: decoding, control flow, data flow, layout, equivalence checking, bookkeeping.
- **A model does what must be guessed**: a name, the intent of a function, which of four equivalent loop forms an author would have written, which three fields are a vector. Models are good at this and unreliable at it.
- **The oracle makes the guess safe.** A model's output is never trusted; it is a *proposal* in a typed form (a rename, a type for a value, a replacement body for a function) that asm checks before it enters the model of the program. A rename is checked for collisions and consistency. A type is checked against the constraints the code imposes. A replacement body is checked against the function's vectors and, where it can, proved equivalent. The cost of a wrong proposal is one rejected card.
- **The person decides what matters**: which subsystem to understand first, what a concept is called, when the faithful port is good enough and when to liberate memory. Their statements are facts of the highest rank, and asm tells them when the binary contradicts one.

Three surfaces serve this:

1. **The workbench** (`../rhipper-ide`): source, disassembly, IR levels and evidence side by side for a function; lockstep debugging of the original against the port (already begun in `lockstep.*`); a status per function that is the same line the agent sees.
2. **The API**: everything the workbench does is a JSON-RPC call, so a model's tools and a plugin use the same door.
3. **The cards**: `ras cards` emits work as cards with machine-checkable acceptance, so that Rhipper Agent, or any walker of cards, improves a port unattended, and its progress is a number.

### 1.5 What no other tool does

| Tool | What it gives | What it does not |
|---|---|---|
| **Ghidra, IDA with Hex-Rays, Binary Ninja** | An interactive view: disassembly, pseudo-C, a database of names and types kept by hand, many architectures | The pseudo-C is for reading. It does not compile, nobody runs it, and nothing checks it against the bytes. Every type is entered by a person. |
| **RetDec, McSema and Remill, rev.ng, llvm-mctoll** | A lift to LLVM IR that can be recompiled; sometimes it runs | Output that no one would maintain: an emulated machine state in C. No path from there to source, no per-function evidence. |
| **Static recompilations (N64Recomp, XenonRecomp and their kin)** | A real game running natively, quickly, with a hand-written runtime | One platform each, a configuration written by hand per game, C that is a transcript of instructions in macros, correctness by playing it. |
| **Matching decompilations (the decomp projects, with decomp.me, m2c, objdiff, splat)** | Real source, proved byte for byte with the original compiler | Human-years per game, the original compiler required, one platform's tooling each. |
| **Neural decompilers** | Plausible, readable source in seconds | No relation to the binary that anyone can check. |

Asm at the endgame is the composition, with the missing piece added:

- It **runs like a recompilation** from the first hour, on every platform through one IR, with the configuration inferred instead of written.
- It **reads like a decompilation** by convergence, with models doing the guessing that cost the decomp projects their years.
- It is **proved like a matching project** where the compiler is known, and by translation validation where it is not.
- And it keeps an **evidence ledger**: per function, per level, what is proved, what is tested and with what coverage, what is assumed. No other tool can answer "how much of this output should I trust, and which part should I look at next" with a list.

Four further things that follow and that none of the above offers:

- **Automatic matching.** Where a toolchain pack includes the original compiler, asm searches for source that compiles to the original bytes, function by function, starting from its own raised output. The decomp projects' central activity becomes a background job.
- **One program across builds and platforms.** Functions are content-addressed; knowledge moves between versions, regions, ports and other games on the same engine.
- **Liberation.** A port that stops being an emulation of the original's memory: host pointers, host structs, host byte order, safe Rust, each step checked against the faithful port.
- **A model of the program that outlives the session**, queryable by tools, mergeable in git, with provenance.

### 1.6 The hard parts, specifically

#### Correctness

There is no proof that a whole port equals a whole binary; the problem is undecidable and the platform is part of it. What asm offers is a chain of narrower claims, each stated with its kind of evidence, and a ledger that adds them up.

*The trusted base*, stated once: the machine descriptions (tested against processors and emulators, derived from a formal specification where one exists), the IR interpreter, the host compiler, the solver, and the platform runtime's fidelity. Everything else is checked.

*Per instruction.* The semantics of every instruction form is tested against a reference: the native processor where the host has it, `qemu-user` or a console emulator where not, hardware-captured test suites where the community has them. The tests are committed as vectors and replayed hermetically. Today's instruction oracle does this for x86 on an x86 host; the endgame does it for every row of every machine, on any host.

*Per function, per step.* Every step from the lifted function to its source is validated for that function, not assumed from the pass's correctness. This is translation validation, and it has three strengths:

- **Tested.** The function's vectors (entry state, memory read, callee answers, effects) are replayed before and after the step. Vectors come from recorded runs, from generated inputs for functions that are pure enough, and from mutation of both, guided by block coverage. The ledger records the count and the coverage.
- **Proved, bounded.** For a loop-free step, or a loop with matching structure on both sides, both versions are encoded for an SMT solver and proved equal over all inputs (bit-vectors and a memory array, as Alive2 does for LLVM). The solver is an external tool, optional as clang is today.
- **Proved by matching.** The source is compiled by the original compiler with the original flags and the bytes compared with the original function, relocations aside. When this succeeds it proves equivalence through the compiler and proves something stronger about readability: the source is one the compiler accepts as the origin of that code.

The final step, source text to behaviour, is included: the emitted C is compiled by the host compiler and the vectors replayed against the compiled function, with the undefined-behaviour sanitizer on in the checked build, because a port that relies on signed overflow is not correct however it tests.

*Assumptions.* Raising is the act of assuming things the machine level does not promise: that a callee preserves the callee-saved registers, that the return address is not modified, that an index stays inside its jump table, that nothing writes to code, that a local whose address is not taken is not reached through a stray pointer, that the stack pointer is restored. Today these are implicit in the passes. At the endgame every step lists the assumptions it used, and each is in one of three states: *discharged* (a static analysis proved it), *checked* (an assertion in the checked build, never fired over the recorded sessions), or *open*. A function is "proved under A" and A is a list a person can read.

*Per program.* Recorded sessions of the original (a native run, an emulator's log, a hardware capture) are replayed against the port and compared at the platform boundary: the same calls with the same data, the same device writes in the same order. Lockstep runs the two side by side and stops at the first difference with the function and the value.

#### Types and data structures

Today's `raise/types.rs` finds structs from constant offsets and merges them across call parameters. The endgame recovers what a header file holds.

- **Constraint-based inference with subtyping.** Each function yields a type scheme (in the manner of Retypd): constraints between its parameters, results and the memory they reach, polymorphic where the function is (a `memcpy` does not unify its callers' types). Schemes are solved bottom-up over the call graph's strongly connected components, so the work is per function and cached.
- **Pointers apart from integers** by value-set analysis and by use, with the 32-bit-pointer port as the hard case: a pointer that is hashed, tagged in its low bits or stored in a `DWORD` field is found by where it flows.
- **Aggregates**: arrays from strided accesses and loop bounds, unions from conflicting uses of one extent, recursive types from self-reference, bitfields from mask-and-shift idioms, enums and flag sets from compared and or-ed constants.
- **Shapes**: linked lists, trees, hash tables and intrusive containers from the pattern of allocation and linkage, statically where possible and from heap snapshots of recorded runs otherwise. The standard library's containers by toolchain version, as layouts in the toolchain pack.
- **Data sections cut into objects**: every global is a typed object with a size and an initializer, tables of structs printed as tables of structs, strings in their encoding, resources in their format.
- **Classes**: vtables, constructors, inheritance from RTTI and from vtable prefix relations without it, member functions grouped, the `this` parameter typed.
- **Evidence from outside the function**: API prototypes pushed inward from every call to a known function; types from a sibling build with debug information; a person's declaration; a model's proposal. All are constraints with a rank. A proposal that contradicts the code's own constraints is refused with the instruction that contradicts it.

The measure is not taste. On programs with debug information, compare the recovered types with DWARF or the PDB: field recall and precision, pointer-versus-integer accuracy, parameter count and type accuracy.

#### Self-modifying and generated code

Four tiers, each sound, each reported:

1. **Code that unpacks once.** Run it in the interpreter until control enters memory the program wrote; snapshot; the snapshot is the image. (Card 53's ground.)
2. **Code that patches itself in place** (an operand rewritten before a loop, common in 8- and 16-bit programs and old DOS code): the patched bytes become a variable when they are an operand, and a switch over the finite set of observed versions when they are an opcode. The assumption "no other version exists" is a checked one.
3. **Code generated at run time** (a JIT, a dynamic recompiler inside an emulator, generated shaders or blitters): the **residual engine**. The lifter and the interpreter are a library linked into the port. Control that reaches code with no static counterpart is lifted on demand from the port's own memory and interpreted, with a cache keyed by the bytes. What it saw is recorded, and the next static build includes every version that was stable. The port always runs; the residue shrinks toward the code that is truly different each time.
4. **A virtual machine inside the program** (a scripting engine, a bytecode protector): the frontier. Asm recognizes the fetch-decode-dispatch shape, treats the bytecode as a second machine whose description it derives from the handlers, and specializes the lifted interpreter over each bytecode routine. For a game's scripting VM this yields the scripts as source. It will not always work, and when it does not, the VM is simply a correct port of an interpreter.

#### Platform runtimes and hardware

A console program is half software and half machine. Asm must not become an emulator, and must not need one at run time.

- **Devices as data.** A device description holds the register map, the bitfields, the command and packet formats. From that one description come: names in the source (`GS_CSR.VSINT` instead of `*(u32*)0x12001000 & 8`), the classification of every access, a stub runtime that logs, and a trace decoder. Writing a description is a card a model can do from public documentation.
- **Two levels of runtime, chosen per library.** *High level*: where the program calls an SDK library asm recognizes by signature, the library function is replaced by a host implementation (the pad library by host input, the sound library by a host mixer). *Low level*: where the program drives hardware registers itself, the device is implemented at its register interface. Most real games need both, and the choice is visible in the status.
- **Time.** A virtual clock advances by the cycle cost of each block (a column of the machine description). The cost is emitted only where some device observes time; most functions carry none. Interrupts, DMA completion and thread switches are delivered at deterministic points of that clock, so a run is reproducible and recorded sessions replay exactly.
- **The boundary is the fixture.** The platform boundary (API calls on a PC, device accesses and interrupts on a console) is where sessions are recorded and replayed. A recorded session from an emulator or from hardware is a test of the port that needs neither afterwards.
- **Graphics honestly.** Rendering a console's command stream on a modern GPU with the original's quirks is a project of its own size per console. Asm's part is the interface and a reference software renderer that is correct and slow; a fast renderer is a plug-in, and the open-source ones can be adapted behind the same interface.

#### Scale

A late console game or a modern desktop application is 50 to 300 MB of code and half a million functions. Three properties are required:

- **Nothing is whole-program that can be per-function.** Every analysis and every raise step takes one function and summaries of what it calls, and gives back the function and its own summary. Whole-program facts are fixpoints over summaries.
- **Everything is memoized by content.** A function's result is keyed by the hash of its bytes, the summaries it used and the facts it read. Renaming a field re-emits the functions that mention it and nothing else. Opening a project reads results, not the binary.
- **Memory is bounded.** The pipeline streams functions through to disk; the largest thing in memory is the largest function plus the index.

Targets to hold asm to: first port of a 100 MB image in under an hour on a workstation with 16 GB; a change to one function visible in source in under a second; a second version of the same program ported in minutes, by reuse.

#### Readability

A compiler's optimizations are the main reason decompiled code reads badly, and they are a finite list. Asm undoes them as *verified rewrites*: each has a recognizer, a replacement, and goes through the same per-function validation as any other step.

- **Inlining undone**: repeated bodies and the signature store's functions recognized inside their callers and outlined (begun in `raise/outline.rs`).
- **Loops restored**: unrolled bodies re-rolled, vectorized loops returned to scalar form with the remainder loop merged, rotated loops (`do-while` under a guard) returned to `while` and `for`, induction variables that were strength-reduced into pointers returned to indices.
- **Arithmetic restored**: multiply-and-shift back to division and modulo, `lea` chains to arithmetic, bit tricks to their names (`abs`, `min`, `bswap`, `popcount`).
- **Control restored**: jump-threaded and tail-merged code separated, if-converted selects turned back into branches where the branch reads better, `switch` rebuilt from compare trees and from tables, hot/cold splits rejoined, shared tails given back to each of their owners (today's `COPY_LIMIT`).
- **Structuring without gotos** as the default result and gotos as a counted defect.
- **Expression shape**: temporaries folded into expressions up to a readable size, casts removed by getting the types right instead of hiding them.

Where the order of rewrites matters, which it does, asm stops ordering passes by hand: the rewrites populate an equality graph of the function and the extraction picks the cheapest form under a readability cost. The cost model is fitted against real source.

The measure, again, is not taste. On the ground-truth corpus (open-source programs compiled by the matrix of compilers and levels): gotos and casts per thousand lines, the share of functions whose statement count is within a factor of the original's, the share of locals and fields with the original's type, names recovered, and the two hard ones: *the port compiles and passes the original project's own test suite*, and *the original compiler turns the port back into the original bytes*.

#### Memory, the last compromise

Today's 32-bit port is a 64-bit program that keeps the original's 32-bit pointers below 4 GB (decision 8). It is the right first step and the wrong last one: the source is full of integer-typed addresses, a big-endian program swaps on every access, and nothing can be handed to a host library without a copy.

**Liberation** moves an object from guest memory to host memory when its type is known and its layout does not matter to the program: no code depends on its size, its field offsets, its address as a number, or its bytes on disk. Asm proves that per type (every access goes through a typed path, no pointer to it is stored as an integer or compared with a constant, it is not read from or written to a file or a device as raw bytes) and then changes the representation: host pointers, host alignment, host byte order. Objects that fail the proof stay guest-flat and are listed with the reason, which is a card for a person or a model. The check is lockstep between the liberated port and the faithful one over the recorded sessions.

The Rust backend climbs the same ladder further: raw pointers, then references where aliasing is proved, then `Box`, `Vec`, slices and `Option` where ownership and bounds are. The measure is the share of functions with no `unsafe`.

### 1.7 What it will not claim

- Code that was never reached, statically or in any recorded run, and whose bytes asm cannot classify stays bytes, with a trap at any entry it knows of.
- "Proved" always names its assumptions and its trusted base. A function tested by forty vectors is called tested.
- Timing is faithful as far as the program can observe it through devices, not to the cycle everywhere.
- A model's name or comment is marked as a model's until a person or evidence confirms it.
- Some programs will keep a residue: a JIT's output, a protector's VM, a renderer whose behaviour depends on a GPU's rounding. The residue is listed, small, and runs.

---

## Part 2. The architecture

### 2.1 What stays

Most of what is on main is the right foundation, and the endgame keeps it:

- **Exact or it stops** (g1). The lift of an instruction is complete or it is a trap that names its address. The endgame adds the fallback (a lower level, the residual engine) but never an approximation.
- **The neutral `Instr` record** of `ra-asm-arch` and the `Arch` trait. It grows a delay-slot flow and a mode field; its shape is right.
- **iced-x86 under `ra-asm-x86`**, and the hand-written x86 lift. It passes the instruction oracle; rewriting it as a description would buy nothing.
- **The IR as an LLVM-IR subset** with its own printer, and clang as the checker of the text. Being a subset of something with a reference implementation is worth more than any custom design.
- **The interpreter** (`ra-asm-ir/src/interp`). It moves to the centre: it is the reference semantics of every level, the engine of facts and recordings, the replayer of vectors, and, linked into a port, the residual engine.
- **Levels** (`Level::{Machine, Promoted, Abi}`). The idea is right and is extended upward.
- **Language-neutral structuring** (`structure.rs`: one `Stmt` tree, three printers).
- **The `ra_*` runtime protocol** and the universal entries `U_<address>`: the adapters between a raised function and machine-level code are exactly what mixed-level ports and the residual engine need.
- **Facts from traced runs** (`Facts`, `ras run --facts`, record and replay), the hand takeover (`hand`), annotations as a project, binary diffing, lockstep, the signature store, the wasm plugins, `ras serve`.
- **The whole-program oracles, fuzzing and the smoke corpus**, as the last line of checking, run at landing and nightly instead of on every card.

### 2.2 What is replaced

Ten debts. Each is cheap on six fixtures and fatal at the endgame; the roadmap's own card notes already show the first four hurting.

| # | Today | Why it fails | Replaced by |
|---|---|---|---|
| 1 | `Image` is a PE: `xbe`, `elf` and `macho` are `Option` fields and other containers synthesize PE headers | A PS2 disc, an overlay, a banked cartridge and a firmware blob have no honest PE shape; one image per program | A neutral `Program` of images, segments, address spaces and epochs (2.3); containers as modules behind one trait |
| 2 | `Target { bits, isa }` answers ABI questions through `is_arm64()`/`is_arm32()` chains; `state_fields_for(isa)` fixes `%State`; `promote.rs` assumes the result register is `rax` (where card 81 ran out of context) | Every new machine edits every pass | `MachineSpec`, `StateSpec` and `Abi` as values the passes read (2.6) |
| 3 | One match arm per mnemonic in files of 750 to 2 600 lines, a decoder and a lift written separately per machine | Twenty machines by hand, each with its own bugs, each needing its own oracle | Machine descriptions as table rows, from which decoder, lift, assembler and vectors derive (2.6). The x86 and ARM lifts that exist stay |
| 4 | `Lifted` and `Analysis` are whole-module values; `raise(module, roots, options)` mutates the module in place | No incrementality, no per-function check, memory proportional to the program (card 131), merge conflicts in every pass | Function units and summaries in a store (2.4) |
| 5 | The raise is a hand-ordered list of passes with implicit preconditions | Order bugs; a pass cannot be tested alone; nothing records what a pass assumed | An explicit tower of levels with a checker each, and steps that record assumptions and evidence (2.5) |
| 6 | The Win32 prototypes are 9 900 lines of Rust inside `ra-asm-lift` | Every edit of the lift recompiles them; a second platform doubles it | Platform packs: data, loaded (2.7) |
| 7 | The oracles compile and run whole programs and compare whole outputs | Slow, all-or-nothing, the failing function unnamed (part 3) | Per-function evidence from vectors, with the oracles above them (2.8) |
| 8 | `raise/types.rs`: 345 lines, structs by constant offset | See 1.6 | `ra-asm-types` (2.9) |
| 9 | Readability passes (`flatten`, `outline`, `strings`, `conditions`, `simplify`) are ordered by hand and trusted | Interactions; no measure; no validation per application | Verified rewrites over a typed level, and extraction by cost (2.10) |
| 10 | One runtime (`rhipper.h`, `driver.c`, `rhipper.rs`) that is the Win32 host, and a second implementation of the same host in `run/host.rs` (1 600 lines) for the interpreter | Two sources of truth per platform, times every platform | One boundary description per platform, from which the interpreter's host, the port's runtime and the recorder are built (2.7) |

### 2.3 The data, stage by stage

```
 medium ──open──────► Program   { images, address spaces, segments, epochs, symbols, relocations }
                        │ decode      (MachineSpec)
                        ▼
                      Instr     the neutral record, per address, per mode
                        │ discover    (seeds, descent, tables, facts from runs)
                        ▼
                      Graph     { functions, blocks, edges, data objects }, each with provenance
                        │ lift
                        ▼
              L0  machine       void F(ptr %state): exact semantics, traps for the uncovered
                        │ promote     assumes: stack discipline
              L1  promoted      registers are SSA values, flags are conditions
                        │ abi         assumes: a convention, callee-saved kept, return address kept
              L2  abi           F(args) -> result, locals, direct calls
                        │ type        (schemes solved over the call graph)
              L3  typed         typed values, field paths, typed globals
                        │ idiom       verified rewrites, extraction by cost
              L4  idiom         loops, division, switches, outlined inlines as the author wrote them
                        │ structure
              L5  tree          statements and expressions, no control-flow graph
                        │ print
                      source    C, C++ or Rust files, a build, tests
                        │ build, run, record
                        ▼
                      traces, vectors, facts  ───────────► back into the store
```

**`Program`.** A set of images. An image is a set of *segments*; a segment is bytes (or zero fill) with a base address in an *address space* (a CPU's virtual space, an IOP's, a coprocessor's microcode memory) and an *epoch* (always, or "bank 3", or "while overlay `stage2.bin` is loaded"). Symbols, relocations and imports are neutral lists. Container-specific data (the PE's exception directory, the ELF's dynamic section, the Mach-O's Objective-C sections) stays behind the container's own type, reachable by the toolchain pack that knows what to do with it. A function's identity is `(image, segment, offset)`; its address is a property that an epoch can share with another function.

**`Graph`.** What `Analysis` holds today, with two changes: every function, edge and data object carries its *provenance* (seed kind, the trace that entered it, the table that pointed at it), so a wrong fact can be retracted with its consequences; and it is produced by queries (2.4), so a new fact from a run extends it instead of recomputing it.

**The function unit.** The thing that flows from lift to print is one function with the summaries of what it calls:

```
unit     { id, bytes hash, body at each level reached, summary at each level, steps }
summary  L0: callees, traps, self-modification suspects
         L1: registers read before written, registers written, stack delta
         L2: signature, memory effects by class (own frame, arguments, globals, unknown)
         L3: type scheme
step     { from, to, pass, inputs (hashes), assumptions, evidence }
```

A step either succeeds, producing the next body with its assumptions and evidence, or fails with a reason, and the function stays where it was. **A program's source is a mix of levels**: most functions at L5 from L4, some emitted from L2, a few as machine-level C from L0, with the adapters between them that `U_<address>` and the wrappers already are. This is what replaces "stops": the port always builds and runs, and the status counts the functions at each level.

### 2.4 The store

A new crate, `ra-asm-db`, with one job: memoize queries and know what they depended on.

- A **query** is a pure function of its key and of other queries: `instr(image, address)`, `function_graph(id)`, `body(id, level)`, `summary(id, level)`, `type_of(id)`, `source(file)`.
- Results are stored on disk under their content hash; a project directory holds an index and objects, and is safe to delete.
- **Early cutoff**: when a recomputed result is equal to the stored one, its dependents are not rerun. A changed function body with an unchanged summary re-raises nobody else.
- **Facts from outside** (a person's rename, a model's accepted proposal, a trace's observed call target) are inputs like the bytes are, kept as text files in the project so they merge and review in git. Today's annotations project is this already.
- Whole-program results are fixpoints of per-function queries over the call graph's strongly connected components, bottom-up, in parallel across independent components.

It is written by hand and small. The queries are coarse (a function at a level, not an expression), so the bookkeeping is a hash map and a dependency list per result; a framework would cost more to fit than to avoid. Card 52's incremental pipeline (per-step stamps) is the same idea at file granularity and folds in.

### 2.5 The tower and its checkers

Each level has an invariant, and each invariant has a checker that runs after every step in debug builds and is a unit test by itself:

| Level | Invariant |
|---|---|
| L0 | Every block ends in a terminator; every register access is a field of `%state` named by the `StateSpec`; calls are direct to `F_*` or through `ra_call`; no `undef` |
| L1 | No load or store of a register field except where state is synchronized (calls to unraised code, entry, exit) |
| L2 | No `%state` parameter; every direct call matches the callee's signature; every stack access is a named local or flagged |
| L3 | Every load and store goes through a typed path or is flagged raw; every integer-pointer conversion is flagged |
| L4 | L3's invariant (rewrites are closed over the level) |
| L5 | The tree is well formed: every break and continue has a target, every goto a label; gotos are counted |

The "flagged" exits are the honest part: a function with a flagged access is still at that level, and the flags are what the readability grade counts (`Grade { gotos, casts, unresolved_calls, stack_accesses }` today).

A level's checker is also the contract a card works against: "after this card, `check_l2` accepts every function of fixture X" is an acceptance line that takes a second to run.

### 2.6 Machines as descriptions

A machine is three values and a table:

- **`MachineSpec`**: byte order, the register file (names, widths, sub-register aliases, fixed-zero registers, banks), modes (ARM and Thumb, MIPS16), delay-slot rules, coprocessors as sub-machines with their own register files (the x87 stack, the GTE, VU0 in macro mode), the address spaces.
- **`StateSpec`**: the layout of `%State` for that machine, derived from the register file. It replaces `state_fields_for`.
- **`Abi`** (several per machine): integer and float parameter slots, return slots, callee-saved set, stack alignment and home space, where the return address lives (stack or link register), variadic rules, the global-pointer register. It replaces `Target`'s method chains; `WIN64`, the x86 conventions, AAPCS64 and AAPCS32 become four constants with the answers the chains give now, and o32 is a fifth rather than an edit to fifteen files.
- **The instruction table**: one row per instruction form.

```rust
row!(addu,  mask 0xfc00_07ff, bits 0x0000_0021, [rd, rs, rt], Next,
     |b, f| b.set(f.rd, b.sext(32, b.add(b.lo32(b.r(f.rs)), b.lo32(b.r(f.rt))))),
     cycles 1),
row!(beq,   mask 0xfc00_0000, bits 0x1000_0000, [rs, rt, rel16], Delayed(CondJump),
     |b, f| b.branch_if(b.eq(b.r(f.rs), b.r(f.rt)), f.rel16),
     cycles 1),
```

From a row the mechanism derives, with no further code per machine:

- the **decoder** (a mask-and-match tree built once), and the disassembly text;
- the **lift** (the closure runs against the IR builder);
- the **register and memory access summary** of `Instr` (the closure runs against a recording builder);
- an **assembler** for that form (fields in, bytes out), used only by tests;
- the **vector enumerator** (every row, with boundary and random field values);
- the **cycle cost** the platform clock uses.

A row whose semantics are long (a GTE command is a few hundred IR instructions, a packed multiply a hundred) is marked `outlined`: the mechanism builds it once as an IR function over the state fields it touches and lifts each use as a call. The backends then print it like any function of the program, which is why no backend ever learns a machine, and the idiom engine can later replace the call by a host intrinsic under proof.

Decisions inside this:

- **Rows are Rust first.** A table of `const` rows with closures is reviewed, type-checked and debugged with the tools the project already has. An external description language is built only when someone outside the project needs to write a machine.
- **What exists and passes its oracle stays.** x86 stays on iced and its match arms. ARM64, ARM32 and Thumb stay as they are until a reason appears.
- **The hand-written MIPS decoder on branch `card/80` lands** and becomes the differential reference for the table's decoder: two independent decoders that must agree on every word of every fixture. Sunk work becomes a check.
- **Card 94 (decoders from a machine description) moves from the end of milestone M to the front of the MIPS work**, and its scope grows from decoders to lifts. MIPS is written as rows by instruction group; PowerPC is the first machine born with no hand-written half.
- **Published specifications are sources, not dependencies.** Sail models (ARM, RISC-V, MIPS), QEMU's decodetree files and SLEIGH can seed rows mechanically; every seeded row passes the same vectors as a hand-written one. Using SLEIGH files would touch gambit g4 (no Ghidra compatibility) in spirit if not in letter, so it is the person's decision, not the architecture's.
- **Hand-written assembly machines** (6502, Z80, 68000) add nothing to the mechanism; they change `Abi` from a constant to something inferred per function (2.9, summaries at L1 are the input).

### 2.7 Platforms

A platform is a **pack** of data and a **runtime** behind one boundary.

The pack: the ABIs in use, the system call or BIOS tables, API prototypes (today's `win32.rs` and `win32_names.rs`, generated from SDK headers as now, but written as a data file that is loaded, not as Rust that is compiled), structure layouts per pointer width, the memory map, device descriptions, and the signature sets of SDK libraries by version.

The boundary: three kinds of event, the same for every platform.

```
call(function, arguments) -> result, memory effects        an API, a syscall, a BIOS call, a recognized SDK function
access(device, register, read | write, value)              a device register, by the device description
deliver(interrupt | callback | thread switch) at clock t   what the world does to the program
```

Everything that touches the outside goes through it, which gives four things from one definition:

1. **the interpreter's host** (today `run/host.rs`), for running at any level without building;
2. **the port's runtime** (today `runtime/`), as a library per platform over a small common core;
3. **the recorder and the replayer**: a session is the sequence of boundary events, and replay answers them from the file. This is the fixture format for whole programs and the source of function vectors;
4. **the comparison point** between the original and the port in lockstep and in the per-program evidence.

Win32 is the first platform and already has every part in some form. Its hard case today is the 32-bit program inside a 64-bit process: structures with pointers cross the boundary and need adapters (23 tests of the emit oracle are ignored today, most for this reason). The pack's per-width structure layouts are what makes those adapters generated rather than written, and where a boundary carries C++ object layouts (the x86 `msvcp140`), the answer is to port the dependency with the program, as a second image, rather than adapt it.

### 2.8 The verifier

A new crate, `ra-asm-verify`, owns evidence.

**Instruction vectors.** A text file per machine and group: bytes, the registers and memory in, the registers and memory out. Generated by a reference (the native processor through today's guarded-page harness, `qemu-user` in WSL through a small guest harness, an emulator's or the community's test suites) and committed. Replayed through decode, lift and the interpreter on any host in seconds.

**Function vectors.** The unit of per-function testing:

```
vector {
  function         id
  entry            the registers the function reads before writing (its L1 summary)
  memory in        the bytes it reads before writing, by address
  calls            in order: callee, the arguments it was given, the result and memory effects it returned
  exit             the registers it leaves live, the bytes it wrote outside its own dead frame, how it left
}
```

Recorded by the interpreter (and by the recording build of a port) at function entry and exit, a few per function per run, chosen for block coverage. Replay loads the entry, answers every call from the record instead of running the callee, and compares the exit. Because callees are answered, **a function is checked alone**: a bug in one function fails that function's line and no other, and a function is replayed in microseconds.

The same vector is replayed at every level: at L0 and L1 as a state; at L2 and above by mapping the entry through the function's signature to arguments; against the built source through a generated harness that links the compiled function with a mock for its callees. A step is *tested* when the vectors pass before and after it.

**Equivalence.** `equiv(before, after, function)` runs the vectors, then generated inputs for functions whose L2 summary says they touch only their arguments and frame, then, where a solver is present and the shapes allow, a bounded proof. It returns the strongest evidence it reached.

**The ledger.** Per function and step: the evidence (`proved`, `matched`, `tested(n vectors, blocks covered / blocks)`, `traced`, `none`) and the assumptions with their state. It is a file in the project, a field of `functions.json`, a column in the workbench, and the body of `ras verify`.

**Matching.** Where the toolchain pack has the original compiler (run as an external tool, like clang), `match(function)` compiles the function's source with it and compares bytes modulo relocations. Search over source variants (the decomp projects' permuter, automated and driven by the idiom engine's alternatives) is a background job per function.

### 2.9 Types

A new crate, `ra-asm-types`, between L2 and L3. Per function: generate constraints from the L2 body (a load at offset 8 of a value means that value points to something with a field at 8 of the loaded width; a value passed to `CreateFileA`'s first parameter is a `const char *`), reduce them to a scheme over the function's parameters and result, store the scheme as the L3 summary. Per strongly connected component: instantiate callees' schemes, solve. Per program: unify the solved shapes into a table of named types, which is a file a person can edit and whose edits are constraints of the highest rank.

The L3 body is the L2 body with loads and stores rewritten to field paths of those types. No behaviour changes between L2 and L3 (types are a view), so the step's validation is the level checker plus the vectors.

Calling conventions for hand-written assembly are the same machinery one level down: the L1 summaries (read before written, written, stack delta) of a function and of its call sites are constraints, and the signature is their solution, per function instead of per platform.

### 2.10 The idiom engine

A new crate, `ra-asm-idiom`, between L3 and L4, replacing the hand-ordered readability passes one by one.

A rewrite is a pattern over a typed function, a replacement, and side conditions. Every application is validated for the function it was applied to (2.8), so a wrong rule costs a rejected application, not a wrong port. Rules are small and many (the list of 1.6), and a rule is the natural size of a card.

Ordering is first a fixed schedule, as now. When the schedule becomes the source of bugs, the function is put into an equality graph, rules add alternatives instead of replacing, and extraction picks the form with the lowest readability cost. That change is invisible to the rules.

Names are the same engine's last stage: each source of names (2.7's packs, RTTI, strings, sibling builds, models) proposes with a rank, and the highest consistent proposal wins.

### 2.11 The residual engine

A new crate, `ra-asm-residual`: the decoder, the lift and the interpreter built as a static library with a C entry, linked into a port's runtime.

- The port's `ra_call` with a target that has no static function, and any jump into memory the program wrote, enters it.
- It decodes from the port's own memory (the faithful port keeps the image at its addresses, so the bytes are there), lifts to L0, and interprets, with a cache keyed by address and byte hash.
- Calls from residual code into static code go through the universal entries; the platform boundary is the port's own runtime.
- On exit it writes what it entered as facts. `ras pipeline` reads them, and the next build has those functions statically. A fixture's acceptance is the line `residual: 0 entered` on the second build.

It makes "runs from the first hour" true for real programs, where code discovery is never complete on the first pass, and it is the whole of tier 3 in 1.6.

### 2.12 Liberation

Not a crate but a property of L3 and the emitters. A type (and the allocation sites and globals of that type) has a *representation*: guest (today's only one: the original's layout at the original's address width and byte order, inside the guest memory block) or host. The analysis that decides it is a set of checks over L3 paths (1.6). The emitters print a guest type as they do now and a host type as an ordinary struct with ordinary pointers; a pointer that crosses between the two is a conversion the checker counts. `layout.rs` and `safety.rs` in `ra-asm-emit` are the beginning of both halves.

### 2.13 The crates, today and at the endgame

| Today | At the endgame |
|---|---|
| `ra-asm-format` | Stays; gains a big-endian cursor |
| `ra-asm-pe` | Splits: `ra-asm-image` (the neutral `Program`, `Image`, `Segment`), the containers as modules of a `ra-asm-containers` crate (PE, ELF, Mach-O, XBE and the rest), `ra-asm-media` (disc file systems, archives, ROM headers) |
| `ra-asm-arch` | Stays and grows `MachineSpec`, `StateSpec`, `Abi` |
| `ra-asm-x86`, `ra-asm-arm64` | Stay |
| (new) | `ra-asm-machine`, the row mechanism; one crate per described family: `ra-asm-mips`, `ra-asm-ppc`, `ra-asm-sh`, `ra-asm-m68k`, `ra-asm-8bit` |
| `ra-asm-analysis` | Keeps the discovery algorithms; its results move into the store; signatures and matching become `ra-asm-sigs` |
| `ra-asm-ir` | Stays; gains the text parser, and in the interpreter the observer, the armed memory log and the single-block runner that recording and the tree interpreter stand on. The level checkers live with the passes whose contract they are |
| `ra-asm-lift` | Splits along its directories: the lifts (hand-written ones here, described ones in their machine crates), `ra-asm-raise` (L0 to L2), `ra-asm-types`, `ra-asm-idiom`, `ra-asm-host` (the interpreter's side of the platform boundary), `ra-asm-protos` (then the platform packs) |
| `ra-asm-emit` | Stays: structuring and printers; gains the tree interpreter and the representations of 2.12 |
| `ra-asm-testing` | Grows: the failure digest, required tools, the vector formats, the manifest |
| `ra-asm-wasm` | Stays |
| (new) | `ra-asm-db`, `ra-asm-verify`, `ra-asm-residual`, `ra-asm-platform` with the packs under `platforms/<name>/` |
| `apps/ras` | Stays the one binary. `serve` becomes a versioned API; new subcommands `new`, `verify`, `bisect`, `case`, `score`, `cards`, `relate`, `match`, `why` (`report.rs` stays what it is today, the table-or-JSON formatter of every subcommand) |
| `runtime/` | A common core (`ra_*`) and one library per platform |

Two rules shape the split. A crate never knows a crate above it, so the build after an edit is short. And a card's files lie in one crate, so its check is that crate's tests.

### 2.14 Each hard problem, and where it is solved

| Hard problem (1.6) | The mechanism | Where | First delivered |
|---|---|---|---|
| An instruction's semantics are right | Vectors from a reference processor or emulator, committed, replayed through decode, lift and the interpreter | `ra-asm-testing` (format), each machine's `tests/vectors.rs` | Phase 1 (x86, x87), phase 3 (MIPS, ARM) |
| A raise step keeps a function's behaviour | Function vectors with callees answered, replayed before and after the step; then generated and mutated inputs; then a bounded proof | `ra-asm-verify` | Phase 1 (replay), phase 2 (generation, coverage), phase 5 (solver) |
| What a step took for granted | Assumptions returned by each step, discharged from summaries or asserted in the checked build | `raise/assume.rs`, the ledger, `ra_assume` | Phase 2 |
| The printed text behaves as the IR | The tree interpreter; snapshots; vectors against the compiled function with mocked callees | `ra-asm-emit/src/tree.rs`, `ras verify --built` | Phases 1 and 2 |
| The whole program behaves as the original | Sessions: the boundary's events recorded and replayed, compared event by event | `ra-asm-host` (then `ra-asm-platform`) | Phase 2 (Win32), phase 7 (devices, clock) |
| Types and structures | Constraint schemes per function, solved over the call graph, seeded by the boundary's prototypes, scored against debug information | `ra-asm-types` | Phase 4 |
| Code nobody found statically | The residual engine in the port; its facts feed the next build | `ra-asm-residual` | Phase 2 |
| Code that unpacks, patches or generates code | Stop-at-written-memory and re-image; epochs; variants as a switch; the residual engine with a cache; a derived machine for bytecode | `ra-asm-host`, `ra-asm-image`, `ra-asm-residual`, `ra-asm-machine` | Phase 2 (unpacking), phase 8 (the rest) |
| A new machine | Rows: decoder, lift, access summary, assembler, cases and cycle cost from one table; `Abi` and `StateSpec` as values | `ra-asm-machine`, `ra-asm-arch` | Phase 3 |
| A new platform, with hardware | One boundary (`call`, `access`, `deliver`), device descriptions, a clock from the rows' cycles | `ra-asm-platform`, `platforms/<name>/` | Phase 3 (the first tables and one device), phase 7 |
| A 32-bit program in a 64-bit host | Structure adapters generated from layouts at both widths; COM thunks; a dependency with C++ layouts ported as a second image | `ra-asm-protos`, `ra-asm-host` | Phase 2 |
| Scale | Function units, summaries, a query store with early cutoff, bounded memory | `ra-asm-db` | Phase 6 (measured from phase 2) |
| Readability | Rewrites as rules with a proof obligation, structuring checked by the tree interpreter, names by ranked evidence, matching with the original compiler | `ra-asm-idiom`, `ra-asm-emit`, toolchain packs | Phase 5 |
| Guest memory in the source | Representation per type, proved per type, asserted in the checked build, lockstep against the faithful port | L3 and the emitters | Phase 10 |
| Knowing where the work is | The ledger, the scoreboard, the pending lists and census, `ras cards` | `ra-asm-verify`, `apps/ras` | Phases 1 and 2 |

---

## Part 3. How the agent builds it efficiently

### 3.1 What the record says

The walks reran 97% of their runs and passed 16 of 202 reviews. The notes on the roadmap's own cards say why, and none of the causes is the model's skill:

1. **The worker writes blind.** `builds = 0`: a worker compiles and runs nothing ("nothing compiled or run here" recurs in the reports). The first compiler of its code is the check, so a type error, or one pedantic clippy line (`format_push_string`, `write_with_newline`, `struct_field_names` each cost a rerun), is a whole run.
2. **The check is everything.** `cargo test -q` runs every oracle: every fixture lifted at three levels, compiled by clang, run, and again as C, C++ and Rust. It needs a 30-minute timeout. A card that edits one arm of the lift waits for all of it.
3. **The check is all-or-nothing.** A failure anywhere fails the card, including failures another card's checkpoint left on main (card 139 existed to repair seven of those). The card's own change is not what is being measured.
4. **The failure does not say where.** The oracles compare a program's whole output; the check's output was truncated before the failing test's panic line, and card 111 spent three sessions guessing.
5. **Cards are too big.** Card 81 (the MIPS lift and the o32 raise, c4) reached the 160 000-token context after 47 turns, stopped inside `promote.rs`'s assumption that a result lives in `rax`, and left a branch.
6. **Cards wait on a person and are run anyway.** Card 80 ran nine times, card 53 five, card 140 nine, against a tool that was not installed or a branch that was not landed.
7. **Checks pass without checking.** A test that skips when a tool or an environment variable is absent reports `ok`.
8. **Breadth is accepted on one fixture.** Milestones I and J read "all tasks accepted", while 23 tests of the emit oracle are `#[ignore]`d with reasons such as "needs the Win32 structure adapters (a card of milestone I)". Ten of the sixteen programs of `testdata/rhtest` (misc, sys, win, seh, eh, cpp, com, stl, stlio, engine) do not run as C, and those are the programs that look most like the target of g7. A capability was called done when its smallest program passed.

### 3.2 The principle

A card changes one function of the pipeline in the mathematical sense: the semantics of an instruction, a pass, a printer rule, a loader's field. **Its check is that function applied to stored inputs and compared with stored outputs.** Nothing compiled by clang, no process run, nothing another card can break, and a failure that names the function and the first difference.

The whole-program oracles answer a different question, whether the parts compose, and are asked when a card lands and every night, not as each card's first compiler.

The targets: a card's check under 60 seconds; the check runnable by the worker; a failing line that names fixture, function and level; and, as the result, a first-run pass rate above 70% where it is 3% now.

### 3.3 Four tiers

| Tier | What | Budget | Who runs it, when |
|---|---|---|---|
| **T0** | `cargo check -q -p <crate> --all-targets`, `cargo clippy -q -p <crate> --all-targets` | seconds | the worker, after every edit |
| **T1** | the card's acceptance: one test target of one crate, or one `ras` subcommand on a committed fixture | under 60 s | the worker before reporting; the check |
| **T2** | `cargo test -q`: every test that needs no external tool | under 5 min | the landing of a card |
| **T3** | the clang oracles in three languages, the pipeline tests, the smoke corpus, the random-seed instruction oracle, the fuzz regressions, fresh csmith seeds | tens of minutes | nightly, and before a milestone is called done |

The mechanism is one line of Cargo per slow test target: `required-features = ["oracle"]`. After that, `cargo test -q`, which `rhipper.context.md` already names as the default check, *is* T2, and T3 is `cargo test -q -p <crate> --features oracle`. No card text needs to change for the default to become fast.

A T3 failure does not fail the card that was being checked. It files a card: `ras bisect` (3.4) reduces the failing program to the first function and step whose vectors differ, and that pair, saved as a pass case, is the new card's fixture and acceptance.

### 3.4 Per-function oracles

Seven kinds of check, each fast, each local, each with committed inputs:

| Oracle | Checks | Input | Says on failure |
|---|---|---|---|
| **Instruction vectors** | one row or arm of a lift, through decode, lift and the interpreter | `testdata/vectors/<machine>/<group>.vec`, from a reference processor or emulator | `addu 0x00a62021: v0 expected 0xffffffff80000000 got 0x0000000080000000` |
| **Function vectors** | one function at one level, callees answered from the record | `testdata/vectors/fn/<fixture>.fv`, recorded by the interpreter | `control32.exe F_00401230 abi: vector 3: eax expected 7 got 0` |
| **Pass cases** | one pass on one function: the interpreter runs the function before and after on the case's calls, and optionally the result is compared with a snapshot | `testdata/cases/<pass>/<name>.ll` | `simplify/select_of_cmp: call 2: expected i32 1 got i32 0` |
| **Level checkers** | the invariant of a level (2.5) on every function of a fixture | the fixtures | `float64.exe F_140001100: L2: load of %state field 3 at block 4` |
| **The tree interpreter** | structuring: the `Stmt` tree of a function executed against its vectors, no compiler | function vectors | `dispatch64.exe F_1400015d0 tree: vector 1: 2 calls expected, 1 made` |
| **Printer snapshots** | the text a printer writes for a case's tree | `testdata/cases/emit/<name>.{c,cpp,rs}` | a line diff; `RA_BLESS=1` rewrites the snapshot and the diff is reviewed |
| **Conformance tables** | breadth: one row per structure, API, syscall, BIOS call or container field | `testdata/conformance/<table>.toml`, rows marked `pass` or `pending` | the row. A `pass` row that fails is a failure, and so is a `pending` row that passes: the card that fixes it flips it |

Three things make these possible and are the first cards of the road:

- **An IR text parser.** `ra-asm-ir` prints `.ll` and cannot read it. With a parser, any function at any level is a text file: a test input, a bug report, a snapshot.
- **A function-vector recorder** in the interpreter: entry state, bytes read before written, each call's arguments and results, the exit effects (2.8).
- **An interpreter for the structured tree**, so the largest untested-except-by-clang component, structuring, is checked in milliseconds.

And one tool closes the loop: **`ras bisect <image>`** replays a failing program's function vectors at each level from L0 upward and prints the first function and step that differ, then writes that function before the step as a pass case. A whole-output difference, which today is a session of guessing, becomes a file and a line.

Printer snapshots can bless a mistake, so T3 keeps them honest: nightly, every snapshot's source is compiled and run against the case's calls.

### 3.5 Fixtures that cannot lie

- **Reference artifacts are committed, with a manifest.** `testdata/MANIFEST.toml` lists every binary, vector file and expected output with its hash, the command that made it, and the versions of the tools. A T2 test checks the hashes and that nothing in `testdata` is unlisted. Regenerating is a script the manifest names; running it is a person's or a building worker's action, never a side effect of a test.
- **Expected output comes from a reference**: the native run, `qemu-user`, an emulator's log, hardware. A fixture whose expected output is "what the source should print according to the SDK's documentation" is marked `evidence: source` in the manifest, and replaced when a reference exists.
- **No test skips.** A test that needs a tool fails with the tool's name. The tiers make this cheap: only T3 needs tools.
- **Comparison normalizes line endings** (a CRLF expected file has cost a run) and nothing else.
- **No network, no clock, no unprinted seed.**

### 3.6 Failures that say where

- The first line of a failure is at most 200 characters and holds the fixture, the function, the level and the first difference.
- The details (expected, actual, the function's IR before and after) are written under `out/failed/<test>/`, and one line per failure is appended to `out/failed/digest.txt`. A worker reads the digest first; no truncation can hide it.
- A comparison of long outputs prints the first differing line with three lines of context, not both outputs.
- A crash in a built port prints the lifted function through the symbolizer, as the emit oracle's helper already does.

### 3.7 The rules of a card

**Size.**

- One crate carries the change. A second crate may appear for one file (a call site, a manifest line, a flag of `ras`), and data under `testdata/`, `platforms/` or `runtime/` belongs to the crate whose test reads it. At most four files, tests and data directories included, listed exactly: the reviewer reads the files the card names, and a stale list hides the change. The one exception is a move (below), whose diff is renames and import paths.
- About 300 changed lines, generated data aside.
- c1 to c3. A c4 or c5 is a section or a milestone and is never run.
- What the card must read fits in a third of the context. Files above 1 500 lines (`x86.rs` 2 633, `c.rs` 2 557, `run/host.rs` 1 607, the emit oracle 1 603, `pipeline.rs` 1 313, `promote.rs` 1 254) are named with the region to read, or split by an earlier card.
- One reason to fail. A card that adds a mechanism converts one user of it; the cards that convert the others change no mechanism.

**Acceptance.**

- One cargo command in backticks (the check takes its command from there) that names a package and a test target: `cargo test -q -p <crate> --test <target> [filter]`, or `cargo run -q -p ras -- <subcommand> <committed fixture>`.
- Exactly one command. With two in backticks the check runs one and the card is accepted on half of what it claims; a card that needs two checks is two cards.
- A missing test target is an error, but a filter that matches no test passes. So the target's tests assert their own counts ("at least 400 vectors replayed"), and the acceptance says what is printed.
- A filter is a substring, and cargo takes one before `--` (several go after it, and any of them matching runs a test). A test's name is therefore chosen so that no other test of its target contains it: `integer` and `fp_ops`, never `arith` and `fp_ops`.
- Under 60 seconds on a warm build. Only committed files as input.
- It checks this card's change and cannot be failed by another card's.

**Shapes that pass the first time.**

- *Fixture first.* One card commits the fixture, its reference output and a `pending` row; the next makes the row pass. The second card starts with a failing check it can run.
- *Mechanism, then rows.* The table and one row; then rows in groups of ten to thirty. Breadth is data, and a card of rows cannot break the mechanism.
- *Move, then change.* Splitting a file or a crate is its own card, with `cargo check -q --workspace --all-targets` as the acceptance and no behaviour changed; the landing's T2 run is what shows the moved tests still pass.
- *Shape, then content.* A card that defines a struct or a file format ships its reader, its writer and a round-trip test, and nothing that uses it.

**The run of a card**, which the context file should state once so that no card has to:

1. Read the files the card names, by the regions its objective points at, and nothing else until those are read.
2. Run the acceptance before editing. It fails, or its target does not exist yet; either way the worker has seen what the check will print and that the tools it needs are there. A card whose acceptance already passes is reported as such and not "done".
3. Edit. After each file, T0 for the crate (`cargo check -q -p <crate> --all-targets`, then clippy for the same crate).
4. Run the acceptance. On a failure read `out/failed/digest.txt` before anything else.
5. Report: what changed, the acceptance's last line, and every pending line added, with the reason each is not this card's to fix.

A card that cannot pass because of something outside its files stops at step 2 or 4 and says so in one line. That is a `blocked` card found in two minutes, where today it is found by the ninth run.

**People.** An action only the person can take (install a tool, land or drop a branch, fetch a file under a licence, choose between designs) is not a card the walk runs. It is listed as a person's action with the cards that wait on it, and those cards are `blocked` until it is done. Part 4 lists the ones the first three phases need.

**The worker's settings** (the person's to change; this document changes nothing):

- `builds = 1` while `parallel = 1`. One worker that can run T0 and T1 removes cause 1 outright. The machine's memory is the reason for `builds = 0`, and `-p <crate>` builds with `CARGO_BUILD_JOBS=6` are the small ones.
- `check_timeout` back from 1 800 toward 600 once the default check is T2.
- Add to the allowlist what T0 and T1 need (`cargo check`, `cargo clippy`, `cargo test -p`, `cargo run -p ras`), and keep T3 for the orchestrator.

### 3.8 Fixtures and corpora

| Corpus | What it is for | Where it lives |
|---|---|---|
| **Today's fixtures** (`testdata/src`, `testdata/rhtest`) | The first function vectors, level checks and the scoreboard's baseline | Committed |
| **Instruction vectors** | Every row and arm of every lift | Committed, a few megabytes per machine; the nightly generates fresh ones with a random seed |
| **Csmith and YARPGen programs** | Self-checking C (the program prints a checksum of its state), across clang, GCC and MSVC, `-O0` to `-O3`, each machine. The widest net for lift and raise bugs, with no expected file to write | A seed list and the built binaries for the committed seeds; fresh seeds nightly; a failing seed is reduced by C-Reduce and becomes a fixture and a card |
| **c-testsuite, the GCC torture tests** | Small conforming programs with known results | Fetched by a script, not committed |
| **Ground-truth programs** | Open-source programs with their own test suites and debug information (zlib, Lua, SQLite, libpng, a small game engine): the measure for types, names and readability, and "the port passes the original's tests" | A script builds them over the compiler matrix; binaries cached outside the repository, hashes in the manifest |
| **Distribution packages** | Debian's multi-architecture packages with their `dbgsym` packages: the same source built for x86, ARM, MIPS, PowerPC, RISC-V and s390x, runnable under `qemu-user`, with DWARF as ground truth | Fetched |
| **Console SDK samples and homebrew** | Each console's first programs, built with the WSL toolchains (PSn00bSDK, ps2sdk, pspdev, libdragon, KallistiOS) | Source and built images committed |
| **Community test suites** | Per-instruction and per-device tests written against hardware (the processor single-step collections, the GTE, VU, GPU and timer test programs of each emulator scene) | Fetched; each licence read before any file is committed |
| **Public matching decompilations** | Ground truth for commercial games: a function-by-function original source for a ROM the person owns | The source fetched; the ROM never in the repository, named by an environment variable in an ignored test |
| **The person's own programs** | The real target of g7 and of each console milestone | Never committed; an environment variable and an ignored test that prints the scoreboard line |
| **Smoke corpora** | Robustness at scale: System32 (today), a distribution's `/usr/bin` per architecture | On the machine |
| **Recorded sessions** | Whole-program fixtures: the platform boundary's events of a run | Committed for the fixtures; beside the program for the person's own |
| **Open firmware** | Firmware built from source for common microcontrollers: the blob path with ground truth | Built by a script |

### 3.9 The scoreboard

`ras score testdata/bin --write` writes `docs/scoreboard.md`: per fixture and in total, the functions at each level, the traps, the evidence counts, gotos and casts per thousand lines, the lengths of the pending lists, and the number of ignored oracle tests. The file is committed. A T2 test fails when a number is worse than the committed one.

Three effects: every card's diff shows what it moved; a regression fails on the card that causes it, not three cards later; and each phase's measure in part 4 is a line of this file, not an opinion.

### 3.10 An example: card 81 as it should have been cut

Card 81, "The MIPS lift and the o32 raise {c4}", asked for a lift of a whole instruction set, a new ABI through passes that assumed x86 and ARM, and a fixture run through the oracle, with `cargo test -q` as its check. In phase 3 below it is some thirty cards (seven that turn the ABI and the state into data, twelve of rows, a dozen that carry a program from image to C). Typical ones:

- "Rows: shifts" {c1}: `sll srl sra sllv srlv srav`, acceptance `cargo test -q -p ra-asm-mips --test vectors shift`, files: the row file and the vector file.
- "promote.rs and abi.rs read results and parameters from the `Abi`" {c2}, acceptance `cargo test -q -p ra-asm-verify --test fixtures`: every function vector of every fixture, which must not change, in under a minute. This is the edit card 81 stopped inside.
- "o32 is an `Abi` constant" {c2}, acceptance `cargo test -q -p ra-asm-arch --test abi o32`: a table test that needs no MIPS code at all, and a diff that touches no pass.

Each is checked in seconds by something that exists before the card starts.

### 3.11 The method is measured too

The claim of this part is a number, and the walk already records what is needed to check it.

- **First-run pass rate**, per phase: cards accepted on their first run over cards run. Today the walk reruns 97% of its runs and 16 of 202 reviews passed. The target for phase 1's own cards is 70% accepted on the first run, and phase 1 is not called done below it: if the proving ground's cards rerun, the cut is wrong and is fixed before the 132 cards of phases 2 and 3 are walked on it.
- **Runs per accepted card.** Today's cards 80, 53 and 140 took nine, five and nine. The target is under 1.5.
- **Check time**, the median of the acceptance commands: under 60 seconds. A card whose acceptance takes longer is re-cut, not given a longer timeout.
- **Why a rerun happened**, as one of five words in the card's note: `compile`, `lint`, `logic`, `environment`, `scope`. The first two should vanish with `builds = 1`; `environment` means a person's action was missed in 4.3; `scope` means the card was two cards. Only `logic` is the model's, and it is the only one this part does not claim to remove.

After each phase the regeneration card reads these four numbers before it cuts the next phase, and changes the rules of 3.7 where they point.

---

## Part 4. The road

### 4.1 Eleven phases, three tracks

```
 engine    1 proving ground ───────────────► 6 store and scale ──► 8 dynamic code ──► 11 living model
               │                                  ▲    │
 depth         ├──► 2 the real program, proven ──► 4 types ──► 5 the source ─────────► 10 liberation
               │                                       │
 breadth       └──► 3 machines from descriptions ──────┴──► 7 platform runtimes ──► 9 every machine
```

The numbers are the order one worker should walk them in. The tracks say what may be reordered: a phase needs the earlier phases of its own track and the ones its row below names, nothing else. In particular **phases 2 and 3 are independent** once phase 1 is done (different crates, different fixtures). Gambit g7 says the PE port comes first; the roadmap regenerated on 2026-11-12 puts MIPS next. This document puts the proving ground before both, then leaves the order of 2 and 3 to the person, and the two could be walked by two workers at once.

| # | Milestone | Capability it delivers | The measure that proves it | Depends on |
|---|---|---|---|---|
| 1 | **The proving ground** | Any change is checked in under a minute by a per-function oracle the worker can run; a whole-program failure is reduced to a function and a step by a tool | `cargo test -q` runs with no external tool in under 5 minutes; `ras verify` prints a verdict for every function the fixtures' runs enter, at L0, L1, L2 and the tree; the x86 instruction vectors replay on any host; 32 csmith seeds at two widths pass in the interpreter at three levels; **over this phase's own cards the first-run pass rate is above 70%** | nothing |
| 2 | **The real program, proven** | Gambit g7, with evidence: the old pipeline's programs all port; a game-sized 32-bit PE builds, runs its recorded session and has a ledger line per function; the residual engine closes what analysis missed | Ignored tests in the emit oracle: 23 to 0 for C; on the person's program: no trap on the executed path, the session replays equal at the Win32 boundary, `residual: 0 entered` on the converged build, 90% of entered functions `tested` at 80% block coverage or better, and gotos and casts per thousand lines below `../CrystalCore`'s | 1 |
| 3 | **Machines from descriptions: MIPS, the PlayStation and the EE** | A machine is a table. MIPS I to III, the R3000A with its GTE and the R5900 are lifted from rows; a MIPS Linux program, a PlayStation program and a PlayStation 2 EE program port and print what the reference prints; the ARM lifts get vectors | Every row has vectors from a reference or is marked `manual`; the table's decoder agrees with the hand-written one and with objdump on every fixture; MIPS `hello` and 16 csmith seeds pass T2 at three levels and T3 as C; adding o32 edits no pass (`is_arm64()` and `is_arm32()` chains in `raise/`: 0) | 1 |
| 4 | **Types and data** | What a header holds: structs, arrays, unions, enums, pointers apart from integers, classes, typed globals | Against DWARF and PDB on the ground-truth corpus: field recall above 80% at precision above 90%, parameter counts above 95%, pointer-versus-integer above 98%; every byte of the data sections classified; casts per thousand lines on the real program halved | 2 |
| 5 | **The source** | Output that reads as written: de-optimization as verified rewrites, structuring without gotos, names from every source, and automatic matching for the first old toolchain | On ground truth: gotos below 1 per thousand lines, statement counts within 1.5 times the original for 80% of functions; the ports of zlib, Lua and SQLite pass those projects' own test suites; half the functions of an `-O2` corpus for one old compiler match its bytes with no person involved | 4 |
| 6 | **The store and scale** | Per-function, memoized, parallel, bounded in memory; interactive | A 100 MB image ported in under an hour and under 8 GB; one function changed, its source back in under a second; a second build of the same program reuses 90% of the results | 1; easier after 4 |
| 7 | **Platform runtimes and hardware** | A console program is a PC program with picture, sound and input, from device descriptions: the PlayStation first, then the PlayStation 2 (EE, VUs, IOP, GS) | Per device, the community's hardware tests pass; SDK samples reach the reference's framebuffer hashes at checkpoints; a commercial game the person owns reaches play and replays a recorded ten minutes deterministically | 3, 2's boundary, 6 for full games |
| 8 | **Dynamic code** | The four tiers of 1.6: unpacking, patched code, generated code, virtual machines | Packed fixtures port equal to their unpacked twins; a self-patching corpus converges to no residue; a small open-source dynamic recompiler ports with residue only in what it generates; one bytecode VM (Lua 5.1 on a script) is specialized into source that passes that script's tests | 2, 6 |
| 9 | **Every machine, every container** | Breadth by rows and packs: the rest of the MIPS family (PSP, Nintendo 64), PowerPC (GameCube, Wii, Xbox 360, PS3), SH-4 and SH-2, 68000, Z80, 6502 and 65816, ARM7TDMI, RISC-V, real-mode x86 and DOS; mobile packages; firmware; language packs | Per machine every row vectored and the csmith set green where a C compiler exists; per container a fuzz-clean loader and an SDK sample ported; **a new RISC machine costs under 25 cards, none above c3, none editing a pass** | 3, 7 for the runtimes |
| 10 | **Liberation** | Ports on host memory: host pointers, host layouts, host byte order, safe Rust | 90% of types host-represented on ground truth, the liberated port in lockstep with the faithful one over the recorded sessions; 60% of Rust functions without `unsafe`; a big-endian program with byte swaps only in listed guest types | 4, 5 |
| 11 | **The living model** | The port improves unattended: `ras cards`, checked proposals, knowledge carried across builds, the API as an MCP server | On a ground-truth program stripped of symbols, an agent walk ends with every function proved or tested and reports its proposals' acceptance rate; every one of a set of seeded wrong rewrites is rejected by the oracle; a new version of a program inherits 95% of the old one's names and types | 5, 6 |

### 4.2 Where today's milestones are reordered, split or merged

1. **Insert phase 1 before everything, and stop walking L until it is done.** Every card walked on today's checks pays the 97%. The proving ground is 49 small cards, none above c3, and pays for itself inside phase 2 or 3: cards 80, 53 and 140 alone account for twenty-three runs.
2. **Reopen milestone I inside phase 2.** I reads "all tasks accepted"; its real tail is the 23 ignored tests of `crates/ra-asm-emit/tests/oracle.rs` (the Win32 structure adapters, structured exception handling, the x86 frame handler, the C++ runtime under x86 names, COM behind x86 vtables). Ten of the sixteen `rhtest` programs do not run as C. Each ignore reason becomes a section of small cards, and "ignored: 0" is the measure.
3. **Pull card 94 (decoders from a machine description) from the end of M to the front of the MIPS work, and widen it to lifts.** Written last it would describe machines already written by hand; written first it is how they are written.
4. **Split milestone L in three.** The MIPS core, the PlayStation and the EE main processor are phase 3. The vector units and the IOP (cards 84, 151, 85) are second processors that mean nothing without their platform and move to phase 7. The PSP and the Nintendo 64 with its RSP (cards 86, 147, 87, 148, 152) are more MIPS rows and two containers and open phase 9.
5. **Replace L's large cards by phase 3's small ones.** After the import, cards 80, 141, 81, 144, 82, 145, 83, 146, 150 and 142 are cancelled: their ground is covered card by card below. Branch `card/80` (the hand-written decoder) is landed by the person before phase 3's MIPS section, whose first card makes its objdump check hermetic; the decoder becomes the table's reference. Branch `card/81` is not merged: its lift arms are the source the row cards copy from, and its ABI edits are redone as data.
6. **Milestone M waits and shrinks.** PowerPC (89, 149, 90) is written from rows in phase 9. The SH-4 cards (143, 93, 153) were blocked on KallistiOS, which is now built; they unblock, but as rows in phase 9. Card 91 (XEX) stays blocked on its inputs.
7. **Merge N's platform interface (card 96) with P's record and replay (card 110) into one boundary.** It is built once on Win32 in phase 2, where there are fixtures and a reference, and reused by every console in phase 7.
8. **Card 100 ("the game, not the homebrew") stops being a card.** It is the measure of phase 7. Card 97 (graphics, c5) becomes one section per console, starting with a reference software renderer. Card 101 (the browser) becomes the last section of phase 7: one more backend of the platform description.
9. **Split card 53 (packed binaries) into four cards and card 131 (memory for the large) into a measuring card in phase 2 and the store in phase 6.**
10. **Merge the tails of J, Q and R and re-cut them by measure** into phases 4 and 5. They were accepted against fixtures; they continue against ground truth. Card 114 (the STL) stays in review as it is; its follow-ups are rows of a container table in phase 4.
11. **H and K were accepted on `hello`-sized fixtures.** Phase 3 gives the ARM lifts instruction vectors from the same QEMU harness as MIPS, which is K's missing evidence at the cost of two cards. Their depth (real Android and iOS programs) is phase 9.
12. **Gambits.** g7 stays active and is phase 2's objective. g6 (one IR, one raise, one set of backends; parked) is what phase 3 builds and should be unparked when phase 3 starts. g3 is reopened by item 2. g1, g2, g4 and g5 are unchanged, and g5 gains a meaning: "correct" is a ledger line.

### 4.3 What the person does before the walk

None of these is a card. The cards that wait on each are named in the outline.

1. **Decide `builds = 1`** for the single worker (3.7). Without it, a card that commits a generated file (vectors, a built fixture) needs the person to run the generator, and the outline says which those are.
2. **Install in WSL**: `qemu-user-static` (it carries `qemu-mipsel-static`, `qemu-mips-static`, `qemu-mips64el-static`, `qemu-aarch64-static` and `qemu-arm-static`, the five the outline runs), `csmith`, `creduce`. Phase 1's csmith section and phase 3's vector sections wait on them. Add the two generator scripts the outline creates, `testdata/vectors/harness/build.ps1` and `testdata/csmith/build.ps1`, to the allowlist of `rhipper.toml` beside `testdata/build.ps1`.
3. **Name the real program**: an environment variable `RA_GAME` with the path of the program gambit g7 means, and `RA_GAME_SESSION` with a recorded session of it, for phase 2's two ignored tests. The program is never committed.
4. **Dispose of the branches**: land `card/80` on main before phase 3's MIPS section (its objdump test needs WSL until that section's first card commits the listings); keep `card/81` unmerged as a source.
5. **Read two licences** before phase 3 commits anything from them: a GTE test suite and a PlayStation 2 instruction test suite from the emulator communities. Without them those rows are marked `manual`.
6. **Optionally install a PlayStation emulator with a headless log.** Without one, the PlayStation fixtures' expected output is `evidence: source` (3.5).
7. **Choose the order of phases 2 and 3**, or walk them side by side.
8. **Later, two choices of design** that are the person's and not a card's: whether the solver of phase 5 is an external process (Z3, one more tool in T3) or a small one written here for the bit-vector fragment the rewrites need; and whether phase 9 imports machine descriptions under other projects' licences (gambit g4 decides).

### 4.4 The outlines

Appendix A holds the cards of phases 1, 2 and 3, one fenced block per phase, in the format `rc import` reads. Appendix B holds phases 4 to 11 as milestones and sections with one-line objectives.

Conventions of the cards:

- **Order is dependency.** Within a phase the cards are in walking order; a card may rely on every card above it in its phase and on nothing below. Where a card relies on another phase, its objective says so.
- **Acceptance commands that mention `--features oracle` are T3 commands** for one fixture; all others are T2 or faster.
- **`(generates)` in an objective** marks a card that commits a file made by running a tool: a building worker runs it, or the person does (4.3, item 1).
- **Test counts in "prints" are floors**, not exact numbers: `test result: ok` with at least the named tests passed and none ignored.
- New files named in `files:` do not exist yet; the card creates them. Existing files were checked against main at `2a0eda3`.
- **A directory in `files:`** means the files the card creates or moves there (a new crate, a set of vector files, a fixture's sources).

The size of what is imported:

| Phase | Cards | c1 | c2 | c3 |
|---|---|---|---|---|
| 1 The proving ground | 49 | 13 | 27 | 9 |
| 2 The real program, proven | 53, and those `ras cards --pending` writes from phase 1's pending lists | 5 | 29 | 19 |
| 3 Machines from descriptions | 79 | 13 | 48 | 18 |

No card is above c3. Phase 2 has the largest share of c3 because it is the phase closest to today's code, whose files are the long ones (3.7); its cards name the region to read.

Before an outline is imported, here or after a regeneration, four things are checked by reading its lines, and Appendix A was checked this way:

1. Every `acceptance:` begins with exactly one cargo command in backticks. (All 181 do.)
2. No test filter is contained in the name of a test it does not mean to run. (A filter such as `printer` that deliberately takes in `printer_c99` says so in its card.)
3. Every card has at most four entries in `files:`, in one crate, with at most one file of a second. (The two moves of "Build time and ownership" are the exceptions: a move crosses crates by its nature, changes no behaviour, and the host move names five entries.)
4. No card is above c3, and each c3 names the region of each long file it reads.

What reading cannot check is whether a test name a card invents will be the name the worker gives it: the acceptance is the contract, so the worker names the test after the filter, and step 2 of the run of a card (3.7) shows a filter that matches nothing as `0 passed`, which is a failure and not a pass. The check that says so ("an acceptance that runs no test fails") belongs to the agent, and is the one change this document asks of it.

---

## Appendix A. The cards of phases 1 to 3

### A.1 Phase 1: the proving ground

```markdown
## The proving ground

### A check that says where

- A comparison fails in one line: the digest of ra-asm-testing {c1}
  objective: The oracles print both whole outputs on a difference and the check's log is cut before the line that matters. One helper gives the first differing line with three lines of context in at most 200 characters, writes both sides under out/failed/<test>/, appends a line to out/failed/digest.txt, and compares with line endings normalized.
  acceptance: `cargo test -q -p ra-asm-testing --test digest` prints `test result: ok` with at least 5 tests: a CRLF and an LF text are equal, a difference at line 400 is reported as line 400, the message is one line under 200 characters, the two files and the digest line exist.
  files: crates/ra-asm-testing/src/digest.rs crates/ra-asm-testing/src/lib.rs crates/ra-asm-testing/tests/digest.rs

- A missing tool fails by name {c1}
  objective: A test that prints "clang is not on the PATH: the oracle is skipped" and returns is a pass that checked nothing. `ra_asm_testing::tool("clang")` returns the path or panics with the tool's name; RA_ALLOW_SKIP=1 restores the skip for a person on a bare machine and prints SKIPPED in capitals.
  acceptance: `cargo test -q -p ra-asm-testing --test tools` prints `test result: ok` with at least 3 tests: a tool that exists is found, a tool that does not panics with its name, the variable turns the panic into a skip.
  files: crates/ra-asm-testing/src/tools.rs crates/ra-asm-testing/src/lib.rs crates/ra-asm-testing/tests/tools.rs

- The lift oracle reports through the digest and requires its tools {c1}
  objective: The first user of the two helpers: every comparison of crates/ra-asm-lift/tests/oracle.rs goes through the digest, and every "is skipped" return for a missing clang becomes `tool("clang")`. The skips for a foreign host (a Windows program on Linux) stay, behind one function that says so.
  acceptance: `cargo test -q -p ra-asm-lift --test oracle a_difference_is_one_line` prints `test result: ok. 1 passed`: the test feeds the comparison a wrong expected text and asserts the message's shape and the digest line.
  files: crates/ra-asm-lift/tests/oracle.rs

- The emit oracle reports through the digest and requires its tools {c1}
  objective: The same for the C, C++ and Rust oracle, whose failures are the longest: the build's stderr goes to out/failed/<test>/build.txt and the message names the fixture, the language and the first line.
  acceptance: `cargo test -q -p ra-asm-emit --test oracle a_difference_is_one_line` prints `test result: ok. 1 passed`.
  files: crates/ra-asm-emit/tests/oracle.rs

### The default check is the fast tier

- The interpreted runs of the lift oracle get their own target {c2}
  objective: The runs through the IR interpreter (helloa64, floata64, thumb32, hello_linux_a64, xbe32 and the x86 fixtures) need no clang and belong to the fast tier; today they sit in the same file as the clang runs. Move `run_interpreted` and its tests to tests/interp.rs, unchanged.
  acceptance: `cargo test -q -p ra-asm-lift --test interp` prints `test result: ok` with at least 5 tests and no child process started (the file does not import std::process).
  files: crates/ra-asm-lift/tests/interp.rs crates/ra-asm-lift/tests/oracle.rs

- The emit oracle sits behind the `oracle` feature {c1}
  objective: `cargo test -q` is the default check of a card, and it must not compile every fixture in three languages. The target gets `required-features = ["oracle"]`; the crate's unit tests stay in the default run.
  acceptance: `cargo test -q -p ra-asm-emit` prints `test result: ok` and finishes in under 60 seconds with no clang started; the manifest declares the feature `oracle` and the target `oracle` with it required, so the same command with `--features oracle --test oracle` is the nightly's.
  files: crates/ra-asm-emit/Cargo.toml

- The lift's clang oracle sits behind the `oracle` feature {c1}
  objective: The same for crates/ra-asm-lift/tests/oracle.rs. tests/interp.rs, the signature tests and the instruction oracle stay in the default run for now.
  acceptance: `cargo test -q -p ra-asm-lift` prints `test result: ok` for the library, `interp`, `signatures` and `instruction_oracle`, and no line for `oracle`: without the feature cargo leaves that target out.
  files: crates/ra-asm-lift/Cargo.toml

- The pipeline and smoke tests of ras sit behind the `oracle` feature {c1}
  objective: apps/ras/tests/pipeline.rs builds and runs ports; smoke.rs walks System32. Both are landing and nightly checks. The other ras tests (serve, functions, lift, diff, elf, macho, plugins, report) stay in the default run.
  acceptance: `cargo test -q -p ras` prints `test result: ok` for each remaining target and finishes in under 120 seconds.
  files: apps/ras/Cargo.toml

- The workflows and the context file name the tiers {c1}
  objective: build.yml runs `cargo test -q` (the fast tier) on every push; nightly.yml runs the three crates with `--features oracle`. rhipper.context.md's "Build and checks" says which is which, that a card's acceptance is its check, and that `cargo test -q` is the landing gate.
  acceptance: `cargo test -q` prints `test result: ok` for every target of the workspace and finishes in under 5 minutes on a warm build, with no clang on the PATH.
  files: .github/workflows/build.yml .github/workflows/nightly.yml rhipper.context.md

### Build time and ownership

- The import prototype tables move to ra-asm-protos {c2}
  objective: raise/prototypes/win32.rs and win32_names.rs are 9 900 lines of tables that every edit of the lift recompiles. They move, with prototypes.rs's lookup and the two tests that regenerate them (tests/win32_protos.rs, tests/win32_names.rs), to a crate that changes only when the SDK tables do; ra-asm-lift depends on it and re-exports the lookup under its old path. No behaviour changes.
  acceptance: `cargo check -q --workspace --all-targets` prints nothing and exits 0.
  files: crates/ra-asm-protos crates/ra-asm-lift/src/raise/prototypes.rs crates/ra-asm-lift/Cargo.toml Cargo.toml

- The interpreter's host moves to ra-asm-host {c2}
  objective: src/run (host.rs 1 607 lines, mod.rs, os.rs, f80.rs, printf.rs) is the Win32 side of the platform boundary, not the lift. It moves to its own crate above ra-asm-lift, and tests/interp.rs moves with it to crates/ra-asm-host/tests/interp.rs; the three files of ras that import `ra_asm_lift::run` (run.rs, lockstep.rs, pipeline.rs) and the two clang oracles change one path each. Phase 2's boundary work, the recorder below and the residual engine then have a home, and an edit of the lift stops rebuilding the host. No behaviour changes.
  acceptance: `cargo test -q -p ra-asm-host --test interp` prints `test result: ok` with the same tests the lift's target had (at least 5).
  files: crates/ra-asm-host crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/run apps/ras/src Cargo.toml

### Instruction vectors

- The instruction vector file: one case a line, read and written {c2}
  objective: The form every machine's instruction tests take: the bytes, the registers and memory in, the registers and memory out, in hexadecimal text, grouped in files by mnemonic group; only the registers that differ from zero in, and that changed out. A reader, a writer and their round trip, with no knowledge of any machine.
  acceptance: `cargo test -q -p ra-asm-testing --test vectors` prints `test result: ok` with at least 6 tests: round trip of a case with memory, a 128-bit register, an expected fault, a malformed line refused with its line number.
  files: crates/ra-asm-testing/src/vectors.rs crates/ra-asm-testing/src/lib.rs crates/ra-asm-testing/tests/vectors.rs

- The x86-64 instruction oracle writes its samples as vector files {c2}
  objective: (generates) The oracle already computes, per form and sample, the state in and the processor's state out. An ignored maintenance test `write_vectors` writes them at the fixed seed, eight samples a form, under testdata/vectors/x86_64/<group>.vec, with the flags the manual leaves undefined masked in the file.
  acceptance: `cargo test -q -p ra-asm-lift --test instruction_oracle -- --ignored write_vectors` prints `test result: ok. 1 passed`, and testdata/vectors/x86_64 holds a file per group whose case count the test prints with --nocapture (at least 8 000 cases).
  files: crates/ra-asm-lift/tests/instruction_oracle.rs testdata/vectors/x86_64

- The x86-64 vectors replay through the lift and the interpreter on any host {c2}
  objective: The hermetic half: tests/vectors.rs reads the committed files and runs each case through decode, lift and the IR interpreter, on Windows, Linux or macOS, in seconds. A difference names the form, the bytes and the first register. With it in the default run, the native oracle (126 seconds, Windows only) moves behind the `oracle` feature.
  acceptance: `cargo test -q -p ra-asm-lift --test vectors x86_64` prints `test result: ok. 1 passed`; the test asserts at least 8 000 cases replayed and finishes in under 30 seconds.
  files: crates/ra-asm-lift/tests/vectors.rs crates/ra-asm-lift/Cargo.toml

- A pending list beside each vector directory {c1}
  objective: New vectors will find wrong arms, and the card that adds vectors must not be the card that fixes the lift. testdata/vectors/<machine>/PENDING.txt names the forms that differ today with their first difference; the replay passes when exactly those differ, and fails when a pending form passes, so the fixing card removes its line.
  acceptance: `cargo test -q -p ra-asm-lift --test vectors pending` prints `test result: ok` with at least 2 tests: a listed form that differs is accepted, a listed form that passes is a failure that says "remove from PENDING.txt".
  files: crates/ra-asm-lift/tests/vectors.rs testdata/vectors/x86_64/PENDING.txt

- A 32-bit harness runs cases on the processor {c3}
  objective: (generates) Gambit g7's machine is 32-bit x86, and the instruction oracle is a 64-bit process: the 32-bit-only forms are in its NOT_RUN list and the x87 unit is not run at all. A small C program built by clang for i686 runs under WOW64 on this machine: it reads a case file, runs each case in a guarded page with a vectored handler, and writes the state out. No emulator is involved.
  acceptance: `cargo test -q -p ra-asm-lift --test vectors_gen -- --ignored harness_runs_three_cases` prints `test result: ok. 1 passed`: `add eax, ebx`, `pushad` and a faulting load give the states written in the test.
  files: testdata/vectors/harness/x86_32.c testdata/vectors/harness/build.ps1 crates/ra-asm-lift/tests/vectors_gen.rs

- The 32-bit integer forms as vectors {c3}
  objective: (generates) The generator of the instruction oracle (iced's code table filtered by the lift's arms) at 32 bits, its cases run by the harness, written under testdata/vectors/x86/<group>.vec. Forms that differ from the lift go to PENDING.txt with their difference; fixing them is phase 2.
  acceptance: `cargo test -q -p ra-asm-lift --test vectors x86_32` prints `test result: ok. 1 passed`; the test asserts at least 6 000 cases replayed and that pushad, popad, pushfd, popfd and the one-byte inc and dec forms are among them.
  files: crates/ra-asm-lift/tests/vectors_gen.rs crates/ra-asm-lift/tests/vectors.rs testdata/vectors/x86

- The x87 forms as vectors {c3}
  objective: (generates) The x87 unit (x87.rs, 790 lines) has no instruction-level check. Cases over the register stack, the status and control words: loads and stores in every width, arithmetic, comparisons, fistp and its rounding modes, fxch, the constants. The transcendental forms are generated too and compared to the last bit, since the reference is the processor.
  acceptance: `cargo test -q -p ra-asm-lift --test vectors x87` prints `test result: ok. 1 passed`; the test asserts at least 2 000 cases replayed at the 80-bit unit.
  files: crates/ra-asm-lift/tests/vectors_gen.rs crates/ra-asm-lift/tests/vectors.rs testdata/vectors/x87

### Function vectors

- The interpreter reports each call's entry and exit to an observer {c2}
  objective: Per-function testing starts with seeing functions: an `Observer` trait on the Vm with `enter(function, depth)` and `leave(function, depth)`, called for every IR function the interpreter runs and for every host call, costing nothing when none is set.
  acceptance: `cargo test -q -p ra-asm-ir --lib interp::observe` prints `test result: ok` with at least 3 tests: a module of three functions reports its calls in order with depths, a host call is reported, a run without an observer is unchanged.
  files: crates/ra-asm-ir/src/interp/observe.rs crates/ra-asm-ir/src/interp/mod.rs

- The memory reports the bytes read before they were written while armed {c2}
  objective: A function vector's "memory in" is what the function read that it had not written itself. `Memory::arm()` starts a log of first reads (address, bytes) and of writes; `disarm()` returns both. One log at a time is enough: one activation is recorded at a time.
  acceptance: `cargo test -q -p ra-asm-ir --lib interp::memory::armed` prints `test result: ok` with at least 4 tests: a read is logged once, a read after a write is not, overlapping reads are merged, the log is empty when not armed.
  files: crates/ra-asm-ir/src/interp/memory.rs

- The crate ra-asm-verify and the function vector's file {c2}
  objective: The home of evidence, starting with the unit of per-function testing: the entry registers, the memory in, the calls in order with their arguments, results and effects, the exit registers, the memory out, the way it left. A text format, a reader, a writer, a round trip; nothing that records or replays yet.
  acceptance: `cargo test -q -p ra-asm-verify --test format` prints `test result: ok` with at least 5 tests.
  files: crates/ra-asm-verify/Cargo.toml crates/ra-asm-verify/src/lib.rs crates/ra-asm-verify/src/vector.rs crates/ra-asm-verify/tests/format.rs

- A run records function vectors, chosen by block coverage {c3}
  objective: The recorder: over an interpreted run at the machine level, arm at a function's entry, log until its exit with nested calls kept as answered calls, and keep a vector when it covers a block no kept vector of that function covers, up to eight a function.
  acceptance: `cargo test -q -p ra-asm-verify --test record` prints `test result: ok` with at least 3 tests: control64.exe yields a vector for every function its run enters, a function called a thousand times keeps at most eight, a recursive function's inner activations are answered calls.
  files: crates/ra-asm-verify/src/record.rs crates/ra-asm-verify/src/lib.rs crates/ra-asm-verify/tests/record.rs

- A vector replays at the machine and promoted levels with its callees answered {c3}
  objective: The replayer: load the entry, run the one function in the interpreter with a host that answers each call from the record in order (a different callee or different arguments is a difference), compare the exit. A callee the record does not hold, which outlining can introduce, is run.
  acceptance: `cargo test -q -p ra-asm-verify --test replay stateful` prints `test result: ok` with at least 4 tests (the tests of the two levels that take a state are named `stateful_*`): every vector of control64.exe replays at both levels, and a module with one instruction changed fails with a line naming that function, the vector and the register.
  files: crates/ra-asm-verify/src/replay.rs crates/ra-asm-verify/src/lib.rs crates/ra-asm-verify/tests/replay.rs

- A vector replays at the ABI level through the function's signature {c3}
  objective: Above the machine level a function takes arguments. The entry state is mapped through `abi::Signature` to the arguments, the result back to the registers, and the comparison keeps to what the signature says is live out and to memory outside the function's dead frame.
  acceptance: `cargo test -q -p ra-asm-verify --test replay signature` prints `test result: ok` with at least 4 tests over control64.exe and control32.exe (cdecl, stdcall and a float result among them).
  files: crates/ra-asm-verify/src/replay.rs crates/ra-asm-verify/src/signature.rs crates/ra-asm-verify/tests/replay.rs

- `ras verify` prints one line per function {c2}
  objective: The command a card's acceptance and a person both use: record or read the vectors of an image, replay them at each level, print `F_<address> machine ok 3 promoted ok 3 abi ok 3` or the first difference, and a last line `verified N of M functions, V vectors`. A filter takes one function.
  acceptance: `cargo run -q -p ras -- verify testdata/bin/control64.exe` prints a line per function entered and ends with `verified N of N functions` (N at least 10); the exit code is 1 when a function differs.
  files: apps/ras/src/verify.rs apps/ras/src/main.rs apps/ras/Cargo.toml apps/ras/tests/verify.rs

- The fixtures' vectors are committed and replayed in the default run {c2}
  objective: (generates) `ras verify --write` stores the vectors of each program of testdata/bin under testdata/vectors/fn/<name>.fv. The test replays the committed files at three levels with no recording, so a raise change is checked against every function of every fixture in seconds, and a difference is one line.
  acceptance: `cargo test -q -p ra-asm-verify --test fixtures` prints `test result: ok`; the test asserts at least 20 programs and 400 vectors replayed and finishes in under 60 seconds.
  files: crates/ra-asm-verify/tests/fixtures.rs testdata/vectors/fn apps/ras/src/verify.rs

### Pass cases

- The IR text parser: types, globals and declarations {c2}
  objective: ra-asm-ir prints `.ll` and cannot read it, so no function can be a test input. The first half of a parser for exactly what printer.rs writes: the lexer, types, named struct types, globals with initializers, declarations and attributes. A refusal names the line and the token.
  acceptance: `cargo test -q -p ra-asm-ir --test parse headers` prints `test result: ok` with at least 8 tests, each printing a module's header, parsing it and comparing the modules.
  files: crates/ra-asm-ir/src/parse.rs crates/ra-asm-ir/src/lib.rs crates/ra-asm-ir/tests/parse.rs

- The IR text parser: instructions and function bodies {c3}
  objective: The second half: every instruction of inst.rs, the operands (locals, integers to 128 bits, floats, globals with offsets, null, undef, zero), phis, the terminators, labels. One test per instruction kind built with builder.rs, printed, parsed and compared.
  acceptance: `cargo test -q -p ra-asm-ir --test parse bodies` prints `test result: ok` with at least 30 tests.
  files: crates/ra-asm-ir/src/parse.rs crates/ra-asm-ir/src/parse/body.rs crates/ra-asm-ir/tests/parse.rs

- Every fixture's module survives the round trip {c1}
  objective: The parser's own oracle: lift each fixture at the three levels, print, parse, print again, and compare the two texts. What the builder tests missed shows here, by line.
  acceptance: `cargo test -q -p ra-asm-lift --test roundtrip` prints `test result: ok`; the test asserts at least 20 programs at three levels and finishes in under 60 seconds.
  files: crates/ra-asm-lift/tests/roundtrip.rs

- The raise's steps have names {c1}
  objective: A case names the pass it tests. `raise::step(name)` returns the function for "promote", "abi", "stack", "frame", "conditions", "simplify", "dce", "widths", "known", "flatten", "outline", "strings", and `raise::steps(level)` the order `raise` runs them in, which `raise` itself then uses.
  acceptance: `cargo test -q -p ra-asm-lift --lib raise::steps` prints `test result: ok` with at least 2 tests: every name resolves, and the list for the ABI level is the order `raise` runs.
  files: crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-lift/src/raise/steps.rs

- A pass case runs: the function before and after agree in the interpreter {c2}
  objective: A case is a `.ll` file with a header of comments: `; pass: simplify`, one `; call: @f(i32 7, i32 9)` per input, optionally `; after:` and the expected text. The runner parses it, interprets each call, runs the pass, interprets again and compares results and memory effects; with `after` it also compares the text. Every case is an equivalence test without anyone writing an assertion.
  acceptance: `cargo test -q -p ra-asm-lift --test cases` prints `test result: ok`; three seed cases under testdata/cases/simplify pass, and a fourth test proves a deliberately wrong pass is caught with the case's name and the call.
  files: crates/ra-asm-lift/tests/cases.rs testdata/cases/simplify

- `ras case` writes a function before a step as a case {c2}
  objective: Turning a failure into a case must cost one command: `ras case <image> <function> --pass <name>` runs the raise up to that step, prints the function and what it references as a module, and takes its `call:` lines from the function's vectors.
  acceptance: `cargo test -q -p ras --test case` prints `test result: ok` with at least 2 tests: the case written for a function of control64.exe before "conditions" parses and passes the runner's check.
  files: apps/ras/src/case.rs apps/ras/src/main.rs apps/ras/tests/case.rs

- The first cases: conditions, simplify, dce, widths {c2}
  objective: (generates) Six cases a pass, written by `ras case` from the fixtures and chosen for the shapes each pass exists for (a compare feeding a branch through flags, a dead store to a register, a narrowed multiply), with `after` snapshots. From here a change to one of these passes has a check in milliseconds.
  acceptance: `cargo test -q -p ra-asm-lift --test cases` prints `test result: ok`; the test asserts at least 24 cases in four directories.
  files: testdata/cases/conditions testdata/cases/simplify testdata/cases/dce testdata/cases/widths

- The first cases: promote, abi, stack, frame {c2}
  objective: (generates) The same for the four passes that carry the levels, where a case's calls are states, not arguments, and the comparison goes through the signature as the replayer's does.
  acceptance: `cargo test -q -p ra-asm-lift --test cases` prints `test result: ok`; the test asserts at least 48 cases in eight directories.
  files: crates/ra-asm-lift/tests/cases.rs testdata/cases

- `ras bisect` names the first function and step that differ {c3}
  objective: A whole-program difference becomes a card by a command: record the vectors at the machine level, replay each function after each step in order, stop at the first function and step that differ, print them with the vector and the value, and write the case. This is what a nightly failure runs before it files anything.
  acceptance: `cargo test -q -p ras --test bisect` prints `test result: ok` with at least 2 tests: with a fault injected into one step by an environment variable, bisect names that step and a function it changed, and the case it writes fails the runner.
  files: apps/ras/src/bisect.rs apps/ras/src/main.rs apps/ras/tests/bisect.rs crates/ra-asm-lift/src/raise/steps.rs

### Level checkers

- The checker of the machine level {c2}
  objective: An invariant that is code: every block ends in a terminator, every register access is a field the state's layout names, every call is direct to a lifted function or through `ra_call`, nothing is undef. `check(function, Level::Machine)` returns the first violation with its block.
  acceptance: `cargo test -q -p ra-asm-lift --test levels machine` prints `test result: ok` with at least 5 tests: a lifted fixture passes, and four broken functions written as text each fail with their own violation.
  files: crates/ra-asm-lift/src/raise/check.rs crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-lift/tests/levels.rs

- The checkers of the promoted and ABI levels {c2}
  objective: Promoted: no load or store of a register field outside the synchronization points. ABI: no state parameter, every direct call matches its callee's signature, every stack access is a local or is flagged. The flagged count is returned, since the grade counts it.
  acceptance: `cargo test -q -p ra-asm-lift --test levels raised` prints `test result: ok` with at least 8 tests (`raised_promoted_*` and `raised_abi_*`: a fixture passes at each level, and three broken functions per level each fail with their own violation).
  files: crates/ra-asm-lift/src/raise/check.rs crates/ra-asm-lift/tests/levels.rs

- Every fixture's functions pass their level's checker {c2}
  objective: The checkers over the corpus: each program of testdata/bin at three levels, with a pending list of the functions that violate today and why. In debug builds `raise` runs the checker after the last step of each level.
  acceptance: `cargo test -q -p ra-asm-lift --test levels fixtures` prints `test result: ok`; the test asserts at least 20 programs, and that testdata/levels/PENDING.txt lists exactly the functions that fail.
  files: crates/ra-asm-lift/tests/levels.rs crates/ra-asm-lift/src/raise/mod.rs testdata/levels/PENDING.txt

### The structured tree runs

- The interpreter runs one block and reports its exit {c2}
  objective: The structured tree is statements over the IR's blocks. To execute a tree, the Vm must run the instructions of one block in a frame and say how it would leave (the branch taken, the switch value, a return) without following it.
  acceptance: `cargo test -q -p ra-asm-ir --lib interp::block` prints `test result: ok` with at least 4 tests: a conditional branch, a switch, a return with a value, a call inside the block.
  files: crates/ra-asm-ir/src/interp/block.rs crates/ra-asm-ir/src/interp/mod.rs

- An interpreter for the structured tree {c3}
  objective: Structuring is checked today only by compiling its output. An interpreter for `Stmt` and `Cond` (blocks, copies, moves, if, loop, break, continue, goto, switch, return) over the Vm's block runner executes a function's tree directly, so a structuring bug shows in milliseconds and names the function.
  acceptance: `cargo test -q -p ra-asm-emit --test tree` prints `test result: ok` with at least 8 tests: one small function per statement kind, each run as a graph and as a tree with the same result.
  files: crates/ra-asm-emit/src/tree.rs crates/ra-asm-emit/src/lib.rs crates/ra-asm-emit/tests/tree.rs

- Every fixture's trees replay their function vectors {c2}
  objective: The structuring's corpus check: for each program of testdata/bin, structure every function at the ABI level and replay its committed vectors through the tree interpreter, with a pending list. A fourth column joins `ras verify`: `tree ok 3`.
  acceptance: `cargo test -q -p ra-asm-emit --test tree fixtures` prints `test result: ok`; the test asserts at least 400 vectors replayed through trees and finishes in under 60 seconds.
  files: crates/ra-asm-emit/tests/tree.rs crates/ra-asm-emit/Cargo.toml testdata/vectors/fn/TREE_PENDING.txt apps/ras/src/verify.rs

- Snapshots of the C printer {c2}
  objective: A printer change today is checked by compiling everything. Twelve small functions as `.ll` cases under testdata/cases/emit, each with the C the printer writes for it as a committed text; a difference is a line diff, and RA_BLESS=1 rewrites the snapshots so the review reads the diff.
  acceptance: `cargo test -q -p ra-asm-emit --test snapshots printer_c99` prints `test result: ok. 1 passed`; the test asserts at least 12 snapshots compared.
  files: crates/ra-asm-emit/tests/snapshots.rs testdata/cases/emit

- Snapshots of the C++ and Rust printers, compiled nightly {c2}
  objective: The same cases through cpp.rs and rust.rs. A snapshot can bless a mistake, so behind the `oracle` feature one test compiles each snapshot with clang or rustc and runs the case's calls against it.
  acceptance: `cargo test -q -p ra-asm-emit --test snapshots printer` prints `test result: ok` with 3 tests passed and at least 36 snapshots compared; the compiled half is a test of the same file under `#[cfg(feature = "oracle")]`, which the nightly runs.
  files: crates/ra-asm-emit/tests/snapshots.rs crates/ra-asm-emit/Cargo.toml testdata/cases/emit

### Programs that check themselves

- The csmith build script and 32 seeds at both widths {c2}
  objective: (generates; needs csmith in WSL) Csmith writes C programs that print a checksum of their own state, so a wrong lift shows without an expected file written by hand. A script generates seeds 1 to 32 with pointers, structs, bitfields and 64-bit arithmetic on, builds each with clang for x86-64 and i686 at -O0 and -O2, runs them natively and writes the checksum as the expected output. Sources, csmith's runtime headers with their licence, binaries and outputs are committed.
  acceptance: `cargo test -q -p ra-asm-pe --test csmith_loads` prints `test result: ok`; the test asserts 128 programs under testdata/csmith/bin, each parsed as an image and each with an expected file of one checksum line.
  files: testdata/csmith/build.ps1 testdata/csmith/src testdata/csmith/bin crates/ra-asm-pe/tests/csmith_loads.rs

- The csmith programs run in the interpreter at three levels {c2}
  objective: The widest net for the lift and the raise in the fast tier: each program lifted, raised to each level and interpreted, its checksum compared. Programs that differ go to a pending list with the level at which they first differ; each is a card of phase 2, opened by `ras bisect`.
  acceptance: `cargo test -q -p ra-asm-host --test csmith` prints `test result: ok`; the test asserts 128 programs at three levels and that testdata/csmith/PENDING.txt lists exactly those that differ.
  files: crates/ra-asm-host/tests/csmith.rs testdata/csmith/PENDING.txt

- The csmith programs through the C output, nightly {c1}
  objective: The same programs through the emit oracle's build-and-run, behind the `oracle` feature, with their own pending list. Fresh seeds are the nightly's; the committed ones are the regression set.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test csmith` prints `test result: ok`; the test asserts 128 programs built and run.
  files: crates/ra-asm-emit/tests/csmith.rs crates/ra-asm-emit/Cargo.toml testdata/csmith/EMIT_PENDING.txt

### The manifest and the scoreboard

- The manifest of testdata and its test {c2}
  objective: A committed binary with no record of how it was made cannot be regenerated or trusted. testdata/MANIFEST.toml lists every binary, vector file and expected output with its SHA-256, the command that made it, the tools' versions and its evidence (`native`, `emulator`, `source`). A test hashes the tree and fails on an unlisted file or a wrong hash.
  acceptance: `cargo test -q -p ra-asm-testing --test manifest` prints `test result: ok` with at least 3 tests; the test asserts at least 150 entries.
  files: testdata/MANIFEST.toml crates/ra-asm-testing/src/manifest.rs crates/ra-asm-testing/src/lib.rs crates/ra-asm-testing/tests/manifest.rs

- `ras score` writes the scoreboard {c2}
  objective: Progress as a file: per program of a directory and in total, the functions at each level, the traps, the functions verified per level, gotos and casts per thousand lines of C, and the lengths of the pending lists and of the oracle's ignore list. Markdown for a person, `--json` for the test.
  acceptance: `cargo run -q -p ras -- score testdata/bin` prints a table with a row per program and a `total` row, in under 60 seconds.
  files: apps/ras/src/score.rs apps/ras/src/main.rs apps/ras/tests/score.rs

- The committed scoreboard never gets worse {c1}
  objective: docs/scoreboard.md is committed, and a test recomputes it: a number worse than the committed one fails with the program and the column, a better one fails with "run ras score --write", so every card's diff shows what it moved and a regression fails on the card that causes it.
  acceptance: `cargo test -q -p ras --test score committed` prints `test result: ok. 1 passed`.
  files: apps/ras/tests/score.rs docs/scoreboard.md
```

### A.2 Phase 2: the real program, proven

Ten of the sixteen `rhtest` programs (misc, sys, win, seh, eh, cpp, com, stl, stlio, engine) are ignored in the emit oracle as C, and three more as Rust only. The first section takes the ignore list apart by reason. The pending lists phase 1 wrote (32-bit forms, x87 forms, csmith programs, level violations, trees) are this phase's other backlog; their cards are written by the first card below, not by hand.

```markdown
## The real program, proven

### The backlog writes its own cards

- `ras cards --pending` writes a card per pending line {c2}
  objective: Every pending list of phase 1 (vector forms, csmith programs, level violations, trees) is a backlog whose cards are mechanical: the title is the line, the acceptance is the replay with that line removed, the files are the list and the source file the list's kind points at. The command prints them in the outline format `rc import` reads, grouped by list, ten lines a card at most.
  acceptance: `cargo test -q -p ras --test cards` prints `test result: ok` with at least 3 tests: a pending list of 25 forms gives 3 cards, each card has the four fields, each acceptance is a cargo command in backticks.
  files: apps/ras/src/cards.rs apps/ras/src/main.rs apps/ras/tests/cards.rs

- The uncovered-instruction census {c2}
  objective: Which traps matter is a count, not a guess: `ras smoke <directory> --census` lifts every image and prints the mnemonics the lift traps on, by number of functions affected, as a pending list `ras cards` reads. Run over testdata, System32 and the person's program, it orders the lift's remaining work.
  acceptance: `cargo test -q -p ras --test census` prints `test result: ok` with at least 2 tests: over testdata/bin the census is a sorted list whose total equals the sum of the functions' trap counts.
  files: apps/ras/src/smoke.rs apps/ras/tests/census.rs

### The old pipeline's programs: structures across the width

- The prototype tables carry structure layouts at both widths {c3}
  objective: (generates) rhtest's misc, sys and win are ignored for "the Win32 structure adapters": a 32-bit program hands the 64-bit host a WNDCLASSEXA laid out for 32-bit pointers. The regenerating test that reads the SDK headers with clang-cl also writes, for every structure a prototype's parameter points to, its fields with offset, size and kind at 32 and at 64 bits.
  acceptance: `cargo test -q -p ra-asm-protos --test layouts` prints `test result: ok` with at least 4 tests: WNDCLASSEXA is 48 bytes at 32 bits and 80 at 64, MSG's fields are found at both, a structure with no pointer has equal layouts and is marked so.
  files: crates/ra-asm-protos/src/layouts.rs crates/ra-asm-protos/src/lib.rs crates/ra-asm-protos/tests/layouts.rs crates/ra-asm-protos/tests/win32_layouts.rs

- The conformance table of structures, every row pending {c1}
  objective: Breadth as rows: testdata/conformance/win32_structs.toml names each structure the ignore reasons name (WNDCLASSA, WNDCLASSEXA, CREATESTRUCTA, MSG, OVERLAPPED, MEMORY_BASIC_INFORMATION, SLIST_HEADER, INIT_ONCE, WSADATA, CRITICAL_SECTION), its direction (in, out, both) and `pending`. The test converts a filled 32-bit structure to the host's and back by the adapter and compares; a pending row that passes fails the test.
  acceptance: `cargo test -q -p ra-asm-host --test conformance` prints `test result: ok`; the test asserts 10 rows, all pending.
  files: testdata/conformance/win32_structs.toml crates/ra-asm-host/tests/conformance.rs

- An adapter is generated from two layouts {c3}
  objective: The mechanism, with one user: from a structure's two layouts, the conversion of a guest structure to a host one and back (integers copied, pointers widened and narrowed with a check that the host's value fits in 32 bits, nested structures recursed), as a function of the interpreter's host. WNDCLASSEXA's row passes.
  acceptance: `cargo test -q -p ra-asm-host --test conformance` prints `test result: ok`; the test asserts 1 row `pass` and 9 pending.
  files: crates/ra-asm-host/src/adapt.rs crates/ra-asm-host/src/lib.rs testdata/conformance/win32_structs.toml

- The window structures pass: WNDCLASSA, CREATESTRUCTA, MSG {c2}
  objective: Rows, not mechanism: the three window structures with their callbacks and string pointers, each row flipped to `pass`.
  acceptance: `cargo test -q -p ra-asm-host --test conformance` prints `test result: ok`; the test asserts 4 rows `pass`.
  files: crates/ra-asm-host/src/adapt.rs testdata/conformance/win32_structs.toml

- The system structures pass: OVERLAPPED, MEMORY_BASIC_INFORMATION, SLIST_HEADER, INIT_ONCE, WSADATA, CRITICAL_SECTION {c2}
  objective: The remaining rows. The two that the host keeps pointers into (CRITICAL_SECTION, SLIST_HEADER) are not converted but kept as host objects behind the guest's address, which the row's direction says.
  acceptance: `cargo test -q -p ra-asm-host --test conformance` prints `test result: ok`; the test asserts 10 rows `pass`.
  files: crates/ra-asm-host/src/adapt.rs testdata/conformance/win32_structs.toml

- The port's import wrappers call generated adapters {c3}
  objective: The built port needs the same conversions as C: `ras emit` writes adapters.c from the layouts for the structures its imports take, and the wrapper of such an import converts before and after the call.
  acceptance: `cargo test -q -p ra-asm-emit --test snapshots adapters` prints `test result: ok` with at least 3 snapshots: the wrapper of RegisterClassExA, of GetMessageA and of an import with no structure.
  files: crates/ra-asm-emit/src/adapters.rs crates/ra-asm-emit/src/lib.rs crates/ra-asm-emit/tests/snapshots.rs testdata/cases/emit

- rhtest's sys and win port as C {c2}
  objective: The two programs the structures alone held back leave the ignore list. What still differs after the adapters is found with `ras bisect` and reported as pending lines, not fixed here.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle -- rhtest_sys_clang_runs_as_c rhtest_win_clang_runs_as_c` prints `test result: ok. 2 passed; 0 failed; 0 ignored`.
  files: crates/ra-asm-emit/tests/oracle.rs

- A host COM interface behind a 32-bit vtable {c3}
  objective: misc needs the shell's IMalloc and com needs objects of the 64-bit runtime called through x86 vtables. An interface pointer the host returns becomes a guest object whose vtable slots are thunks into the host's methods, built from the interface's prototype; IUnknown and IMalloc are the first two rows of testdata/conformance/win32_com.toml.
  acceptance: `cargo test -q -p ra-asm-host --test conformance interfaces` prints `test result: ok`; the test asserts 2 rows `pass`: a guest call of IMalloc::Alloc through the vtable returns memory below 4 GB, Release drops the host reference.
  files: crates/ra-asm-host/src/com.rs crates/ra-asm-host/src/lib.rs crates/ra-asm-host/tests/conformance.rs testdata/conformance/win32_com.toml

- rhtest's misc and com port as C {c2}
  objective: With the adapters and the interfaces, the two programs leave the ignore list; com's uncovered instruction at 0x401e2e joins the vectors and is lifted. misc's Rust port stays ignored for setjmp alone, and its reason says only that.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle -- rhtest_misc_clang_runs_as_c rhtest_com_clang_runs_as_c` prints `test result: ok. 2 passed; 0 failed; 0 ignored`.
  files: crates/ra-asm-emit/tests/oracle.rs crates/ra-asm-lift/src/x86.rs testdata/vectors/x86

### The old pipeline's programs: exceptions on x86

- The scope tables of `_except_handler4` are read {c2}
  objective: seh is ignored for structured exception handling. The 32-bit `__try` leaves a scope table in the image and a registration frame on the stack; the analysis finds the table from the prologue that pushes it and gives, per function, the protected ranges with their filter and handler addresses, as the x64 unwind data gives them today.
  acceptance: `cargo test -q -p ra-asm-analysis --test seh32` prints `test result: ok` with at least 3 tests over testdata/rhtest/seh-clang.exe: the functions with a scope table, their ranges, the nested one's parent.
  files: crates/ra-asm-analysis/src/seh32.rs crates/ra-asm-analysis/src/lib.rs crates/ra-asm-analysis/tests/seh32.rs

- A 32-bit `__try` runs in the interpreter {c3}
  objective: The lift maps a scope table's ranges onto the `ra_eh_*` protocol the x64 tables use (lib.rs says "x64 only so far"), and the host runs the filter and the handler when a fault or RaiseException reaches a protected range.
  acceptance: `cargo test -q -p ra-asm-host --test interp seh_clang` prints `test result: ok. 1 passed`: seh-clang.exe interpreted at the ABI level prints its expected text.
  files: crates/ra-asm-lift/src/eh.rs crates/ra-asm-host/src/host.rs crates/ra-asm-host/tests/interp.rs

- rhtest's seh ports as C {c2}
  objective: The C runtime's side of the same protocol for 32-bit programs, and the program leaves the ignore list.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle rhtest_seh_clang_runs_as_c` prints `test result: ok. 1 passed; 0 failed; 0 ignored`.
  files: runtime/rhipper.h runtime/driver.c crates/ra-asm-emit/tests/oracle.rs

- The x86 C++ frame handler's tables are read {c2}
  objective: eh is ignored for "the x86 frame handler's registration frames, and thrown objects with destructors". The FuncInfo of `__CxxFrameHandler3` on x86 (try blocks, unwind map, catch handlers) is read as the x64 one is; the state comes from the registration frame's try level instead of the instruction address.
  acceptance: `cargo test -q -p ra-asm-analysis --test eh32` prints `test result: ok` with at least 3 tests over testdata/rhtest/eh-clang.exe.
  files: crates/ra-asm-analysis/src/eh32.rs crates/ra-asm-analysis/src/lib.rs crates/ra-asm-analysis/tests/eh32.rs

- A 32-bit throw unwinds and destroys in the interpreter {c3}
  objective: The try level written to the registration frame is the state; a throw walks the unwind map, runs the destructors of the unwound objects and of the thrown one after its catch, and enters the handler.
  acceptance: `cargo test -q -p ra-asm-host --test interp eh_clang` prints `test result: ok. 1 passed`.
  files: crates/ra-asm-lift/src/eh.rs crates/ra-asm-host/src/host.rs crates/ra-asm-host/tests/interp.rs

- rhtest's eh ports as C {c2}
  objective: The runtime's side, and the program leaves the ignore list.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle rhtest_eh_clang_runs_as_c` prints `test result: ok. 1 passed; 0 failed; 0 ignored`.
  files: runtime/rhipper.h runtime/driver.c crates/ra-asm-emit/tests/oracle.rs

### The old pipeline's programs: the C++ runtime

- A port takes a second image: `ras emit --with <dll>` {c3}
  objective: stl and stlio are ignored because the 64-bit msvcp140 does not export the x86 mangled names, and behind the names its objects have other layouts. A boundary that carries C++ object layouts is not adapted: the dependency is ported with the program. An import one image of the port exports binds directly to the lifted function, and the two images share the guest memory.
  acceptance: `cargo test -q -p ras --test with_image` prints `test result: ok` with at least 2 tests: testdata/bin/libmain's program emitted with its DLL calls the DLL's lifted export with no host import left for it, and functions.json names both images.
  files: apps/ras/src/emit.rs apps/ras/src/project.rs apps/ras/tests/with_image.rs

- rhtest's stl and stlio port as C with the x86 C++ runtime {c3}
  objective: The two programs with SysWOW64's msvcp140.dll as their second image (read from the system like clang is, never committed). What the DLL's own lift traps on goes to the census.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle -- rhtest_stl_clang_runs_as_c rhtest_stlio_clang_runs_as_c` prints `test result: ok. 2 passed; 0 failed; 0 ignored`.
  files: crates/ra-asm-emit/tests/oracle.rs

- cpp and engine: the first differing function is a case {c2}
  objective: (generates) cpp "goes wrong at multi" and engine at "stream len", and nobody knows in which function. `ras bisect` over each names the function and the step, and writes the case; this card commits the two cases as pending and changes no code.
  acceptance: `cargo test -q -p ra-asm-lift --test cases` prints `test result: ok`; the test asserts two more cases, pending, each failing at the call the case names.
  files: testdata/cases/pending/cpp_multi.ll testdata/cases/pending/engine_stream_len.ll crates/ra-asm-lift/tests/cases.rs

- cpp and engine port as C {c3}
  objective: The two cases pass (the step each names is fixed, or the case shows a host boundary the second image solves), and the programs leave the ignore list.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle -- rhtest_cpp_clang_runs_as_c rhtest_engine_clang_runs_as_c` prints `test result: ok. 2 passed; 0 failed; 0 ignored`.
  files: crates/ra-asm-emit/tests/oracle.rs testdata/cases/pending

- A shared tail in Rust is a labeled block {c3}
  objective: Three Rust ports (core-static, float-ia32, crt-clang) are ignored for "a forward goto the Rust output cannot express". A forward jump to a tail two paths share is `'tail: { ... break 'tail; ... }` followed by the tail; the structuring marks such a goto and rust.rs prints it. Checked by snapshots and by the tree interpreter, which runs the marked tree against the functions' vectors.
  acceptance: `cargo test -q -p ra-asm-emit --test snapshots shared_tail` prints `test result: ok` with at least 3 tests: two snapshots, and a function with a shared tail whose tree replays its vectors.
  files: crates/ra-asm-emit/src/structure.rs crates/ra-asm-emit/src/rust.rs crates/ra-asm-emit/src/tree.rs crates/ra-asm-emit/tests/snapshots.rs

- The Rust ports leave the ignore list {c1}
  objective: The three programs the shared tail held (core-static, float-ia32, crt-clang) and the Rust twins of the ten programs above lose their `#[ignore]` where nothing else holds them; what still fails keeps its ignore with the new reason, which `ras cards --pending` reads.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle -- rhtest_core_static_runs_as_rust rhtest_float_ia32_runs_as_rust rhtest_crt_clang_runs_as_rust` prints `3 passed; 0 failed; 0 ignored`.
  files: crates/ra-asm-emit/tests/oracle.rs

### Evidence

- The ledger: evidence per function and step {c2}
  objective: What `ras verify` learns is kept: per function and per step, `tested` with the count of vectors, `traced` when the function was only seen entered, `none`. A text file in the project's gen directory, read and written by ra-asm-verify, merged across runs.
  acceptance: `cargo test -q -p ra-asm-verify --test ledger` prints `test result: ok` with at least 4 tests: write and read, a merge keeps the stronger evidence, a function whose bytes changed loses its lines.
  files: crates/ra-asm-verify/src/ledger.rs crates/ra-asm-verify/src/lib.rs crates/ra-asm-verify/tests/ledger.rs

- The ledger counts the blocks a function's vectors cover {c2}
  objective: Forty vectors through one path are one test. The replayer reports the blocks each vector ran; the ledger line becomes `tested 5 vectors, 11 of 14 blocks`, and the blocks never covered are listed for the generator below.
  acceptance: `cargo test -q -p ra-asm-verify --test ledger coverage` prints `test result: ok` with at least 2 tests over control64.exe.
  files: crates/ra-asm-verify/src/replay.rs crates/ra-asm-verify/src/ledger.rs crates/ra-asm-verify/tests/ledger.rs

- functions.json carries the evidence {c1}
  objective: The workbench shows the function table; a function's line gains `evidence` (per level) and `coverage`. The JSON's shape changes, so the report says so for ../rhipper-ide.
  acceptance: `cargo test -q -p ras --test functions evidence` prints `test result: ok. 1 passed`.
  files: crates/ra-asm-emit/src/functions.rs apps/ras/src/functions.rs apps/ras/tests/functions.rs

- Generated inputs for functions that touch only their arguments and frame {c3}
  objective: A function the recorded runs never entered has no vector. Where the ABI level's summary says the function reads and writes only its arguments, its frame and its result, inputs are generated (boundary values and random ones, seeded) and the machine level is the reference for the levels above. `ras verify --generate` adds them.
  acceptance: `cargo test -q -p ra-asm-verify --test generate` prints `test result: ok` with at least 3 tests: a pure function of structs64.exe that the run does not enter gets 16 vectors and replays at three levels; a function that reads a global gets none.
  files: crates/ra-asm-verify/src/generate.rs crates/ra-asm-verify/src/lib.rs crates/ra-asm-verify/tests/generate.rs

- Recorded vectors mutate toward uncovered blocks {c2}
  objective: For the functions that do touch memory, a recorded vector is the seed: one entry register or one byte of memory in is changed, the machine level run with the record's answers, and the result kept when it reaches a block no vector covered and no answered call diverged.
  acceptance: `cargo test -q -p ra-asm-verify --test generate mutate` prints `test result: ok` with at least 2 tests: the coverage of a switch function of dispatch64.exe grows from its recorded vectors.
  files: crates/ra-asm-verify/src/generate.rs crates/ra-asm-verify/tests/generate.rs

- Vectors replay against the compiled C {c3}
  objective: The last step, text to behaviour, is checked per function too: `ras verify --built` writes a harness beside the port that loads each vector, calls the compiled function with its callees linked to a mock that answers from the record, and prints the same line `ras verify` prints. A printer bug is then one function's line, not a program's output.
  acceptance: `cargo test -q -p ras --features oracle --test verify built` prints `test result: ok. 1 passed`: control64.exe's functions are verified through clang, and a printer fault injected by an environment variable fails exactly the functions that use the construct.
  files: apps/ras/src/verify.rs runtime/verify.c apps/ras/tests/verify.rs

### Assumptions

- A raise step lists what it assumed: the record, and the ABI step's {c3}
  objective: Raising assumes what the machine level does not promise. `Assumption { function, step, kind, site }` with the first kinds (the callee keeps the callee-saved registers, the return address is not written, the stack pointer is restored), returned by the ABI step beside its result instead of being implicit in it.
  acceptance: `cargo test -q -p ra-asm-lift --test assumptions abi` prints `test result: ok` with at least 4 tests: a function with two calls lists the callee-saved assumption per call, a leaf lists none, the list is stable across runs.
  files: crates/ra-asm-lift/src/raise/assume.rs crates/ra-asm-lift/src/raise/abi.rs crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-lift/tests/assumptions.rs

- The promote, stack and frame steps list theirs {c2}
  objective: The same for: a local whose address is not taken is not reached by another pointer, pushes and pops balance on every path, the frame pointer is a frame pointer.
  acceptance: `cargo test -q -p ra-asm-lift --test assumptions -- promote stack frame` prints `test result: ok` with at least 5 tests.
  files: crates/ra-asm-lift/src/raise/promote.rs crates/ra-asm-lift/src/raise/stack.rs crates/ra-asm-lift/src/raise/frame.rs crates/ra-asm-lift/tests/assumptions.rs

- Jump tables and indirect calls list theirs {c2}
  objective: The analysis assumes an index stays inside its table when no bound check dominates the jump, and devirtualization assumes the candidate set is complete. Both become assumptions with their site; a table whose bound check was found is recorded as discharged.
  acceptance: `cargo test -q -p ra-asm-lift --test assumptions -- tables calls` prints `test result: ok` with at least 3 tests over dispatch64.exe and classes64.exe.
  files: crates/ra-asm-analysis/src/jumptable.rs crates/ra-asm-lift/src/raise/assume.rs crates/ra-asm-lift/tests/assumptions.rs

- Callee-saved registers are discharged where the callee is lifted {c2}
  objective: Most assumptions can be proved from what is already computed: the promoted level's summary of a lifted callee says which registers it writes and restores. An assumption whose callee's summary proves it becomes `discharged`; calls to imports are discharged by the platform's ABI; what remains is open.
  acceptance: `cargo test -q -p ra-asm-lift --test assumptions discharge` prints `test result: ok` with at least 3 tests; over control32.exe at most the indirect calls stay open.
  files: crates/ra-asm-lift/src/raise/assume.rs crates/ra-asm-lift/src/raise/liveness.rs crates/ra-asm-lift/tests/assumptions.rs

- The checked build asserts the open assumptions {c3}
  objective: What is not proved is watched: `ras emit --checked` prints each open assumption as `ra_assume(<id>, <condition>)` at its site (the saved registers compared after the call, the index against the table's length, the stack pointer at the return), and the runtime's `ra_assume` prints the function, the kind and the site and stops.
  acceptance: `cargo test -q -p ra-asm-emit --test snapshots checked` prints `test result: ok` with at least 3 snapshots, one per kind of site.
  files: crates/ra-asm-emit/src/c.rs runtime/rhipper.h crates/ra-asm-emit/tests/snapshots.rs testdata/cases/emit

- The ledger holds the assumptions and their state {c2}
  objective: `ras verify` writes each function's assumptions as discharged, checked (the checked build ran the site in a recorded session and it held) or open, and `ras score` gains the three columns. "Proved under" becomes a list a person can read.
  acceptance: `cargo test -q -p ra-asm-verify --test ledger assumptions` prints `test result: ok` with at least 3 tests.
  files: crates/ra-asm-verify/src/ledger.rs apps/ras/src/score.rs crates/ra-asm-verify/tests/ledger.rs

### The Win32 boundary as a file

- A session is the boundary's events, and the fixtures' sessions are committed {c2}
  objective: (generates) Card 110's recording, named as what it is: every host call of a run with its arguments, result and the memory it wrote, in order. `ras run --record` over each Windows fixture writes testdata/sessions/<name>.session, listed in the manifest with evidence `native`.
  acceptance: `cargo test -q -p ra-asm-host --test sessions format` prints `test result: ok`; the test asserts at least 20 sessions read back equal.
  files: crates/ra-asm-host/src/session.rs crates/ra-asm-host/tests/sessions.rs testdata/sessions

- A session replays with no host {c3}
  objective: A Windows program's run becomes a test on any machine: a replaying host answers every call from the session and fails at the first call that differs in name or arguments, naming the event and the lifted function that made it. The interpreted runs of the Windows fixtures stop needing Windows.
  acceptance: `cargo test -q -p ra-asm-host --test sessions replay` prints `test result: ok`; the test asserts at least 20 programs replayed at the ABI level with no host call made, and that a changed argument is reported with its event number.
  files: crates/ra-asm-host/src/replay.rs crates/ra-asm-host/src/lib.rs crates/ra-asm-host/tests/sessions.rs

- A built port replays a session {c3}
  objective: The same for the compiled port: with RA_REPLAY set, the runtime's import wrappers answer from the session file instead of calling the host, so a window program's port is tested without a window, and deterministically.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle window64_replays_its_session` prints `test result: ok. 1 passed`.
  files: runtime/replay.c runtime/rhipper.h crates/ra-asm-emit/tests/oracle.rs

- Two sessions compare: the first event that differs {c2}
  objective: The per-program evidence is a comparison of the original's session with the port's. `ras diff --sessions a b` prints `equal, N events` or the first differing event with the calling function on each side.
  acceptance: `cargo test -q -p ras --test diff sessions` prints `test result: ok` with at least 2 tests.
  files: apps/ras/src/diff.rs apps/ras/tests/diff.rs

### Nothing stops the run: the residual engine

- A fixture whose call target no analysis finds {c2}
  objective: (generates) testdata/src/computed.c calls functions through addresses it computes at run time from a table of obfuscated offsets, so the static analysis has no reference to three of its functions. Its 32- and 64-bit builds and expected output are committed, and its row in the scoreboard shows the trap.
  acceptance: `cargo test -q -p ra-asm-analysis --test fixtures computed` prints `test result: ok. 1 passed`: the three functions are not among the functions found, by the addresses in the link map.
  files: testdata/src/computed.c testdata/bin/computed64.exe testdata/bin/computed32.exe testdata/expected/computed.txt

- ra-asm-residual lifts and interprets one function from memory {c3}
  objective: The engine: given a reader of memory, a state and an address, decode the function there, lift it at the machine level, interpret it, with a cache keyed by address and the hash of the bytes, and with calls to known addresses handed back to the caller. A Rust API first; no C entry yet.
  acceptance: `cargo test -q -p ra-asm-residual --test engine` prints `test result: ok` with at least 4 tests: a function of computed64.exe run from the image's bytes returns what its vector says, the second call hits the cache, changed bytes miss it.
  files: crates/ra-asm-residual/Cargo.toml crates/ra-asm-residual/src/lib.rs crates/ra-asm-residual/tests/engine.rs

- The interpreter's unknown call falls to the residual engine {c2}
  objective: The first user: where the interpreter's host today stops at a call to an address with no lifted function, it enters the engine with the Vm's memory. computed runs to its end in the interpreter and the run's facts name the three functions.
  acceptance: `cargo test -q -p ra-asm-host --test interp computed` prints `test result: ok` with at least 2 tests: the expected output, and three entries under `residual` in the facts.
  files: crates/ra-asm-host/src/host.rs crates/ra-asm-host/Cargo.toml crates/ra-asm-host/tests/interp.rs

- The port links the residual engine {c3}
  objective: The same for a built port: the engine as a static library with a C entry `ra_residual_call(state, address)`, linked by the CMake template when the project asks for it, entered by `ra_call` on an unknown target, reading the port's own memory, and writing what it entered to a facts file at exit.
  acceptance: `cargo test -q -p ras --features oracle --test pipeline computed_runs_with_residue` prints `test result: ok. 1 passed`: the port prints the expected output and `residual: 3 entered` on stderr.
  files: crates/ra-asm-residual/src/c.rs runtime/rhipper.h apps/ras/templates/CMakeLists.txt apps/ras/tests/pipeline.rs

- The second build has no residue {c2}
  objective: The loop closes: `ras pipeline` reads the facts file a run left, the analysis seeds the three functions, and the rebuilt port never enters the engine. This is the convergence every real program's port goes through.
  acceptance: `cargo test -q -p ras --features oracle --test pipeline computed_converges` prints `test result: ok. 1 passed`: the second run prints `residual: 0 entered`.
  files: apps/ras/src/pipeline.rs apps/ras/tests/pipeline.rs

### Packed programs: card 53 in four

- The UPX fixtures {c1}
  objective: (generates) hello32, hello64 and control32 packed by upx, committed with the manifest's command, and named by `protector_of`. Their expected output is their unpacked twins'.
  acceptance: `cargo test -q -p ra-asm-analysis --test fixtures upx` prints `test result: ok. 1 passed`: the three images are named UPX.
  files: testdata/bin/hello32-upx.exe testdata/bin/hello64-upx.exe testdata/bin/control32-upx.exe crates/ra-asm-analysis/tests/fixtures.rs

- The interpreter stops where control enters memory the run wrote {c2}
  objective: An unpacking stub ends by jumping into what it wrote. The host tracks the pages written since the start, and a call or jump to an unlifted address inside one stops the run with `Stop::Unpacked(address)` instead of entering the residual engine, when the run asked for it.
  acceptance: `cargo test -q -p ra-asm-host --test interp upx_stops_at_the_entry` prints `test result: ok. 1 passed`: hello32-upx stops at the address that is hello32's entry point.
  files: crates/ra-asm-host/src/host.rs crates/ra-asm-ir/src/interp/mod.rs crates/ra-asm-host/tests/interp.rs

- The stopped run's memory is an image {c3}
  objective: The unpacked program as an input: the sections from the Vm's memory, the entry from the stop, the imports rebuilt from the addresses the stub resolved into the import table, written as a PE the loader reads.
  acceptance: `cargo test -q -p ra-asm-host --test unpack` prints `test result: ok` with at least 3 tests: the image written for hello32-upx parses, has hello32's imports by name, and its code section equals hello32's.
  files: crates/ra-asm-host/src/unpack.rs crates/ra-asm-host/src/lib.rs crates/ra-asm-host/tests/unpack.rs

- The pipeline unpacks first {c2}
  objective: `ras pipeline` on an image the analysis names as packed runs the unpack step and continues on its output; the three fixtures port to the same output as their twins.
  acceptance: `cargo test -q -p ras --features oracle --test pipeline upx` prints `test result: ok` with 3 tests passed.
  files: apps/ras/src/pipeline.rs apps/ras/tests/pipeline.rs

### The program

- `ras pipeline --stats`: time and peak memory per step {c2}
  objective: Card 131's measuring half: before memory is engineered it is measured. Each step prints its wall time and the process's peak working set, and the totals go to the project's status file; over testdata the numbers are small and the test only checks they are there.
  acceptance: `cargo test -q -p ras --test stats` prints `test result: ok. 1 passed`: analyze and lift of control64.exe each report a time and a peak above zero.
  files: apps/ras/src/pipeline.rs apps/ras/src/project.rs apps/ras/tests/stats.rs

- The grade of any C directory {c2}
  objective: Gambit g7 asks for C "better to read" than the old pipeline's. `ras score --source <dir>` computes the lexical half of the grade (lines, gotos, casts, labels per thousand lines) over any C tree, so ../CrystalCore is a baseline number. It reads that directory and writes nothing there.
  acceptance: `cargo test -q -p ras --test score source` prints `test result: ok` with at least 2 tests over a C file of testdata/cases/emit with known counts.
  files: apps/ras/src/score.rs apps/ras/tests/score.rs

- The boundary census: what a program imports that the host cannot answer {c2}
  objective: The traps' census has a twin at the boundary. `ras smoke <image> --census` also lists the imports with no prototype, the structure parameters with no adapter row, and the COM interfaces with no row, each by the number of call sites, as pending lists `ras cards` reads. For a game-sized program this list, not anyone's guess, is the order of the remaining Win32 work (Direct3D and DirectSound interfaces first, by count).
  acceptance: `cargo test -q -p ras --test census boundary` prints `test result: ok` with at least 2 tests: over testdata/rhtest/win-clang.exe the list is empty once phase 2's rows pass, and an image importing a function the tables lack lists it with its call sites.
  files: apps/ras/src/smoke.rs apps/ras/tests/census.rs

- The real program's line {c1}
  objective: (needs RA_GAME, the person's program, never committed) An ignored test runs analyze, lift and score over the program and prints its scoreboard line and its census; the person runs it and pastes the line into docs/scoreboard.md under "the program". From here phase 2's measure is that line.
  acceptance: `cargo test -q -p ras --test score -- --ignored the_program` prints the line when RA_GAME is set, and fails with "RA_GAME is not set" when it is not.
  files: apps/ras/tests/score.rs

- The program's session: recorded once, replayed against every build {c2}
  objective: (needs RA_GAME) The interpreter records the program from its start to a point the person chooses (the title screen, by a call count); an ignored test replays that session against the current lift at the ABI level and prints `equal, N events` or the first difference with its function. With the residual engine, the session's facts feed the next analysis.
  acceptance: `cargo test -q -p ra-asm-host --test sessions -- --ignored the_program` prints `equal, N events` when RA_GAME and RA_GAME_SESSION are set.
  files: crates/ra-asm-host/tests/sessions.rs
```

### A.3 Phase 3: machines from descriptions

The phase has seven sections. The first two change no behaviour (the ABI and the state become data; the table mechanism is proved on a toy machine). The third builds the reference. Only then does MIPS begin, as rows in groups, each group checked by vectors before any program runs.

Rows whose reference this machine cannot execute (most of the R5900's multimedia instructions, the GTE) are checked against a second implementation written independently from the documentation by another card, and carry `evidence: manual` in the manifest until a community suite's vectors replace it (4.3, item 5).

```markdown
## Machines from descriptions: MIPS, the PlayStation and the EE

### The ABI and the state are data

- The answers of `Target`'s methods are a snapshot {c1}
  objective: Before the calling conventions move into data, what the code answers today is written down: for WIN64, X86, ARM64 and ARM32, every method of `Target` over its whole argument range (parameter slots 0 to 15, every register for callee_saved), as a committed text the test recomputes. The refactor below is then checked in a second.
  acceptance: `cargo test -q -p ra-asm-lift --test abi_tables snapshot` prints `test result: ok. 1 passed`; the snapshot has at least 300 lines.
  files: crates/ra-asm-lift/tests/abi_tables.rs testdata/cases/abi_tables.txt

- `Abi` is a value: the record and four constants {c2}
  objective: A calling convention as data in ra-asm-arch: integer and float parameter slots, result slots, the callee-saved set, stack alignment and home space, where the return address lives (the stack or a link register), the stack pointer, a global-pointer register if any. Constants for Win64, the x86 conventions, AAPCS64 and AAPCS32 with the values the snapshot shows.
  acceptance: `cargo test -q -p ra-asm-arch --test abi` prints `test result: ok` with at least 6 tests, one per constant and two for the record's invariants (no register both a parameter and callee-saved).
  files: crates/ra-asm-arch/src/abi.rs crates/ra-asm-arch/src/lib.rs crates/ra-asm-arch/tests/abi.rs

- `Target` answers from its `Abi` {c2}
  objective: The methods of `Target` in raise/mod.rs (sp, pushed_return, int_param_slots, float_params, param_slot, callee_saved, entry_misalignment and the rest) lose their `is_arm64()` and `is_arm32()` chains and read the constant. The snapshot does not change.
  acceptance: `cargo test -q -p ra-asm-lift --test abi_tables` prints `test result: ok`; the snapshot test passes unchanged and a second test asserts raise/mod.rs has no ISA test left in a method of `Target`.
  files: crates/ra-asm-lift/src/raise/mod.rs crates/ra-asm-lift/tests/abi_tables.rs

- The ISA tests of the raise are counted and may only fall {c1}
  objective: The chains outside `Target` (promote.rs's `rax` for a result is where card 81 stopped) are found by a test that counts `is_arm64`, `is_arm32`, `Isa::` and literal register names in src/raise and compares with a committed number: a rise fails, a fall asks for the number to be lowered. The cards below take it to zero.
  acceptance: `cargo test -q -p ra-asm-lift --test abi_tables ratchet` prints `test result: ok. 1 passed`; the report states the count.
  files: crates/ra-asm-lift/tests/abi_tables.rs testdata/cases/isa_ratchet.txt

- promote.rs and abi.rs read results and parameters from the `Abi` {c2}
  objective: The result register, the parameter registers and the caller-saved set in promote.rs and abi.rs come from the `Abi`. Checked by the function vectors of every fixture, which do not change.
  acceptance: `cargo test -q -p ra-asm-verify --test fixtures` prints `test result: ok`; the ratchet's committed count falls in this card's diff, and the landing's run fails if it was not lowered.
  files: crates/ra-asm-lift/src/raise/promote.rs crates/ra-asm-lift/src/raise/abi.rs testdata/cases/isa_ratchet.txt

- stack.rs, frame.rs and liveness.rs read the stack's rules from the `Abi`; the ratchet is zero {c2}
  objective: Whether a call pushes its return address, the alignment at entry, the frame register and the link register come from the `Abi`. After this card a new convention is a constant and no pass is edited for it.
  acceptance: `cargo test -q -p ra-asm-verify --test fixtures` prints `test result: ok`, and the ratchet's committed count is 0.
  files: crates/ra-asm-lift/src/raise/stack.rs crates/ra-asm-lift/src/raise/frame.rs crates/ra-asm-lift/src/raise/liveness.rs testdata/cases/isa_ratchet.txt

- `StateSpec`: the layout of the machine state is a value {c3}
  objective: `%State` is `{ [16 x i64], i64, [8 x i8], [16 x i128] }` for every machine, by `state_fields_for`. A `StateSpec` in ra-asm-arch (register groups with count and width, the program counter, flag bytes, extra fields) from which the IR type, the field offsets and the names derive; x86, ARM64 and ARM32 get specs that reproduce today's layouts bit for bit.
  acceptance: `cargo test -q -p ra-asm-lift --lib state::spec` prints `test result: ok` with at least 4 tests: the three machines' types and offsets equal the ones written out in the test, and a spec with 32 registers of 128 bits lays out without overlap.
  files: crates/ra-asm-arch/src/state.rs crates/ra-asm-arch/src/lib.rs crates/ra-asm-lift/src/state.rs

### The table mechanism

- A row, and a decoder built from rows {c3}
  objective: The new crate ra-asm-machine, on a toy machine of eight instructions defined in its tests: `Row { name, mask, bits, fields, flow }` for fixed 32-bit words, a table that builds a mask-and-match tree once, refuses two rows that accept the same word unless one is strictly more specific, and decodes a word to a row and its field values.
  acceptance: `cargo test -q -p ra-asm-machine --test decode` prints `test result: ok` with at least 6 tests: each toy row decodes, a specific row wins over a general one, an overlap is refused with both names, an unknown word is `None`.
  files: crates/ra-asm-machine/Cargo.toml crates/ra-asm-machine/src/lib.rs crates/ra-asm-machine/src/table.rs crates/ra-asm-machine/tests/decode.rs

- A decoded row is an `Instr` with its text {c2}
  objective: The neutral record from a row: mnemonic, operands from the fields (registers by the machine's names, immediates, relative targets resolved), the flow, the text in the form the machine's objdump prints, so the analysis and the disassembly need nothing per machine.
  acceptance: `cargo test -q -p ra-asm-machine --test instr` prints `test result: ok` with at least 5 tests on the toy machine: a register form, an immediate form, a branch with its target, a call, the text of each.
  files: crates/ra-asm-machine/src/instr.rs crates/ra-asm-machine/src/lib.rs crates/ra-asm-machine/tests/instr.rs

- A row's semantics build IR {c3}
  objective: A row carries a function over a builder (`r`, `set`, the arithmetic and comparisons at a width, `sext`, `zext`, `load`, `store`, `branch_if`, `jump`, `call`, `trap`). Run against the IR builder with a `StateSpec` it lifts the instruction; the toy machine's rows are lifted and interpreted.
  acceptance: `cargo test -q -p ra-asm-machine --test sem` prints `test result: ok` with at least 8 tests: one per toy row, the state after the interpreter equal to the state written in the test, and a write to the toy's zero register is discarded.
  files: crates/ra-asm-machine/src/sem.rs crates/ra-asm-machine/src/lib.rs crates/ra-asm-machine/tests/sem.rs

- A row's semantics say what it reads and writes {c2}
  objective: The same function run against a recording builder gives the registers read and written and whether memory is read or written, which fills `Instr`'s access fields. Nobody writes an access table by hand, and it cannot disagree with the lift.
  acceptance: `cargo test -q -p ra-asm-machine --test access` prints `test result: ok` with at least 4 tests on the toy machine.
  files: crates/ra-asm-machine/src/access.rs crates/ra-asm-machine/src/lib.rs crates/ra-asm-machine/tests/access.rs

- A function lifts from the table, delay slots included {c3}
  objective: The block lifter every described machine shares: over a function's blocks, lift each instruction by its row; a row whose flow is `Delayed` executes the next instruction before the transfer with the condition computed first, a "likely" row annuls the slot when not taken, a transfer inside a slot is a trap that names its address. The toy machine gains a delayed branch.
  acceptance: `cargo test -q -p ra-asm-machine --test lift` prints `test result: ok` with at least 6 tests: a loop, a delayed branch whose slot changes the compared register, an annulled slot, a slot that is a jump target, a branch in a slot trapping.
  files: crates/ra-asm-machine/src/lift.rs crates/ra-asm-machine/src/lib.rs crates/ra-asm-machine/tests/lift.rs

- A large row is one function, called where it is used {c2}
  objective: A row whose semantics run to hundreds of instructions (a GTE command, a packed multiply) would swell every function that uses it and hide the program in the output. A row marked `outlined` is built once as an IR function over the state fields it reads and writes, named after the row, and lifted as a call to it; the backends print it like any other function, so no backend knows a machine.
  acceptance: `cargo test -q -p ra-asm-machine --test outline` prints `test result: ok` with at least 3 tests on the toy machine: an outlined row used twice yields one function and two calls, the result equals the inlined form's, the access summary is the same.
  files: crates/ra-asm-machine/src/outline.rs crates/ra-asm-machine/src/lift.rs crates/ra-asm-machine/tests/outline.rs

- A row assembles, and enumerates its cases {c2}
  objective: Fields in, word out, for tests only; and from a row and a seed, instruction-vector cases: every register field at its boundary registers, immediates at their extremes and random, register values from a set of boundary and random numbers. The enumerator is what the reference runs.
  acceptance: `cargo test -q -p ra-asm-machine --test cases` prints `test result: ok` with at least 4 tests: every enumerated case of the toy machine decodes to the row it came from, and the same seed gives the same cases.
  files: crates/ra-asm-machine/src/asm.rs crates/ra-asm-machine/src/cases.rs crates/ra-asm-machine/src/lib.rs crates/ra-asm-machine/tests/cases.rs

- A table checks itself {c1}
  objective: `check(table)` is one call a machine's test makes: every row decodes its own encodings, no two rows accept one word, every row's semantics run against both builders without a panic, every row has cases. A new machine gets these tests by writing its rows.
  acceptance: `cargo test -q -p ra-asm-machine --test check` prints `test result: ok` with at least 3 tests: the toy table passes, a table with an unreachable row and one with a panicking row fail by name.
  files: crates/ra-asm-machine/src/check.rs crates/ra-asm-machine/src/lib.rs crates/ra-asm-machine/tests/check.rs

### The reference: a guest harness under qemu-user

- The harness and its MIPS trampoline {c3}
  objective: (generates; needs qemu-user-static in WSL) A freestanding C program with no libc, built by clang in WSL for a guest: it reads cases (code bytes, registers, a data page) from standard input, copies the code into an executable page followed by a return, loads the registers, runs, stores the registers and the data page, and writes the result. The per-machine part is one assembly file that loads and saves the registers; mipsel is the first. A script builds and runs it under `qemu-mipsel-static`.
  acceptance: `cargo test -q -p ra-asm-machine --test harness -- --ignored mipsel_runs_three_cases` prints `test result: ok. 1 passed`: `addu`, `lw` and a taken `beq` with its slot give the states written in the test.
  files: testdata/vectors/harness/guest.c testdata/vectors/harness/mipsel.S testdata/vectors/harness/run.sh crates/ra-asm-machine/tests/harness.rs

- A machine's vectors are generated by one function {c2}
  objective: `generate(table, guest, seed, out)` in ra-asm-machine's test support: enumerate the table's cases, run them through the harness in one batch per group, write testdata/vectors/<machine>/<group>.vec in the format of phase 1, with the fields a row declares undefined masked. A machine's regenerating test is three lines.
  acceptance: `cargo test -q -p ra-asm-machine --test harness -- --ignored toy_generates` prints `test result: ok. 1 passed`: with a fake guest that answers from the interpreter, the files are written and read back.
  files: crates/ra-asm-machine/src/generate.rs crates/ra-asm-machine/src/lib.rs crates/ra-asm-machine/tests/harness.rs

- A machine's vectors replay by one function {c1}
  objective: `replay(table, spec, directory)`: each committed case through decode, lift and the interpreter, with the pending list of phase 1. A machine's vector test is three lines and runs anywhere.
  acceptance: `cargo test -q -p ra-asm-machine --test replay` prints `test result: ok` with at least 3 tests on vectors of the toy machine committed under testdata/vectors/toy.
  files: crates/ra-asm-machine/src/replay.rs crates/ra-asm-machine/tests/replay.rs testdata/vectors/toy

### MIPS: the core, by groups of rows

- The hand-written decoder's check is hermetic {c2}
  objective: (generates; after the person lands branch card/80) crates/ra-asm-mips arrives with decode.rs, registers.rs, r5900.rs and an objdump oracle that needs WSL's llvm-objdump at test time. The listings of its fixtures are generated once and committed, and the test compares the decoder's text with them, so the reference decoder is checked in the fast tier on any host.
  acceptance: `cargo test -q -p ra-asm-mips --test objdump` prints `test result: ok` with no process started; the test asserts at least 2 000 instructions compared.
  files: crates/ra-asm-mips/tests/objdump.rs testdata/mips/listings

- The MIPS machine: the spec, the state and the first five rows {c2}
  objective: (generates) MIPS32 little-endian as a `MachineSpec` and a `StateSpec` (32 general registers of 64 bits with a fixed zero, hi, lo, the program counter), and the rows of `addu`, `addiu`, `or`, `ori`, `lui` in src/rows, with their vectors from the harness. The shape every later group copies.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors integer` prints `test result: ok. 1 passed`; the test asserts at least 200 cases over 5 rows and that `check(table)` passes.
  files: crates/ra-asm-mips/src/spec.rs crates/ra-asm-mips/src/rows/integer.rs crates/ra-asm-mips/tests/vectors.rs testdata/vectors/mipsel/integer.vec

- Rows: the rest of arithmetic, logic and comparison {c2}
  objective: (generates) `add addi sub subu and andi xor xori nor slt slti sltu sltiu`, the trapping overflow of `add`, `addi` and `sub` as a trap flow.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors integer` prints `test result: ok. 1 passed`; the test asserts 18 rows and at least 700 cases.
  files: crates/ra-asm-mips/src/rows/integer.rs testdata/vectors/mipsel/integer.vec

- Rows: shifts {c1}
  objective: (generates) `sll srl sra sllv srlv srav`, with `sll $0, $0, 0` printed as `nop`.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors shift` prints `test result: ok. 1 passed`; the test asserts 6 rows and at least 240 cases.
  files: crates/ra-asm-mips/src/rows/shift.rs testdata/vectors/mipsel/shift.vec

- Rows: aligned loads and stores {c2}
  objective: (generates) `lb lbu lh lhu lw sb sh sw` over the harness's data page, with the sign and zero extensions and an unaligned word access as a fault case.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors mem` prints `test result: ok. 1 passed`; the test asserts 8 rows and at least 320 cases.
  files: crates/ra-asm-mips/src/rows/mem.rs testdata/vectors/mipsel/mem.vec

- Rows: unaligned loads and stores {c2}
  objective: (generates) `lwl lwr swl swr` at each of the four alignments, in the byte order of the spec, since the big-endian machine reuses the rows.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors unaligned` prints `test result: ok. 1 passed`; the test asserts 4 rows and at least 160 cases covering every alignment.
  files: crates/ra-asm-mips/src/rows/mem.rs testdata/vectors/mipsel/unaligned.vec

- Rows: branches, jumps and links {c3}
  objective: (generates) `beq bne blez bgtz bltz bgez bltzal bgezal j jal jr jalr` as delayed flows, the link register written before the slot runs, and the likely forms `beql bnel blezl bgtzl bltzl bgezl` annulling. The harness's case holds the branch, a slot that changes a register, and two landing stubs.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors branch` prints `test result: ok. 1 passed`; the test asserts 18 rows, taken and not-taken cases for each conditional row, and a slot that writes the compared register.
  files: crates/ra-asm-mips/src/rows/branch.rs testdata/vectors/mipsel/branch.vec

- Rows: multiply, divide, hi and lo {c2}
  objective: (generates) `mult multu div divu mfhi mflo mthi mtlo`. Division by zero and the overflowing quotient are declared undefined in the rows, as the manual says, and masked in the vectors.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors muldiv` prints `test result: ok. 1 passed`; the test asserts 8 rows and at least 320 cases.
  files: crates/ra-asm-mips/src/rows/muldiv.rs testdata/vectors/mipsel/muldiv.vec

- Rows: syscall, break and the conditional traps {c1}
  objective: `syscall` as a call of the runtime's `ra_syscall`, `break` and `teq tne tge tgeu tlt tltu` with their immediate forms as trap flows with their code. Checked by lift tests, since the harness cannot run them.
  acceptance: `cargo test -q -p ra-asm-mips --test rows system` prints `test result: ok` with at least 4 tests.
  files: crates/ra-asm-mips/src/rows/system.rs crates/ra-asm-mips/tests/rows.rs

- Rows: the floating-point unit's registers and moves {c2}
  objective: (generates) Coprocessor 1 as a sub-machine in the spec (32 registers of 64 bits, the control register) and `mfc1 mtc1 cfc1 ctc1 lwc1 swc1 ldc1 sdc1 mov.s mov.d`.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors fpmove` prints `test result: ok. 1 passed`; the test asserts 10 rows and at least 300 cases.
  files: crates/ra-asm-mips/src/spec.rs crates/ra-asm-mips/src/rows/fp.rs testdata/vectors/mipsel/fpmove.vec

- Rows: floating-point arithmetic, conversions and compares {c3}
  objective: (generates) `add sub mul div sqrt abs neg` in single and double, `cvt.s.d cvt.d.s cvt.w.s cvt.w.d cvt.s.w cvt.d.w`, `trunc round ceil floor` to word, the sixteen `c.cond` compares and `bc1t bc1f` with their likely forms. NaNs, infinities, denormals and each rounding mode are among the cases.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors fp_ops` prints `test result: ok. 1 passed`; the test asserts at least 50 rows and 2 000 cases.
  files: crates/ra-asm-mips/src/rows/fp.rs crates/ra-asm-mips/src/rows/fpcmp.rs testdata/vectors/mipsel/fp_ops.vec

- The table's decoder and the hand-written one agree {c2}
  objective: Two decoders written independently are each other's oracle: over every word of the fixtures' code and a million seeded random words, the row's name, operands and flow equal decode.rs's, or the word is in a committed list of known differences with the reason. From here the analysis uses the table.
  acceptance: `cargo test -q -p ra-asm-mips --test agree` prints `test result: ok` with at least 2 tests; the test asserts a million words compared in under 30 seconds.
  files: crates/ra-asm-mips/tests/agree.rs crates/ra-asm-mips/src/lib.rs testdata/mips/DIFFERENCES.txt

### A MIPS program, from image to C

- The freestanding fixtures: a start, two syscalls, a small printf {c2}
  objective: (generates) A foreign Linux machine's fixtures should not drag in a C library. testdata/src/freestanding holds a start routine per machine, `write` and `exit` by syscall, and a printf of %d %u %x %s %c; with it hello.c and control.c are built by clang in WSL for mipsel, static, and their expected output is what `qemu-mipsel-static` prints (evidence `emulator`).
  acceptance: `cargo test -q -p ra-asm-testing --test manifest` prints `test result: ok`; the manifest lists testdata/bin/hello_mipsel and testdata/bin/control_mipsel with evidence `emulator` and their expected files equal testdata/expected/hello.txt and control.txt.
  files: testdata/src/freestanding testdata/bin/hello_mipsel testdata/bin/control_mipsel testdata/MANIFEST.toml

- A static MIPS ELF is an Image {c2}
  objective: elf.rs accepts the MIPS machine at 32 bits: the segments, the symbols, the entry, the byte order, and the global pointer's value from `.reginfo` or the `_gp` symbol, kept where the lift can read it.
  acceptance: `cargo test -q -p ra-asm-pe --test elf mipsel` prints `test result: ok` with at least 3 tests over hello_mipsel: the entry, the text segment's range, the value of gp.
  files: crates/ra-asm-pe/src/elf.rs crates/ra-asm-pe/src/image.rs crates/ra-asm-pe/tests/elf.rs

- ra-asm-mips implements `Arch` over its table {c2}
  objective: The trait the analysis walks: decode at an address through the table to the neutral `Instr`, with the flow of a delayed transfer marked as taking effect one instruction later, `jal` and `bal` as calls, `jr $ra` as a return.
  acceptance: `cargo test -q -p ra-asm-mips --test arch` prints `test result: ok` with at least 5 tests over bytes of hello_mipsel: a call, a return, a conditional branch with its two successors, a likely branch, a `j`.
  files: crates/ra-asm-mips/src/arch.rs crates/ra-asm-mips/src/lib.rs crates/ra-asm-mips/tests/arch.rs

- The analysis descends through delay slots {c3}
  objective: The builder ends a block after the slot of a transfer and not at the transfer, a slot that is itself a jump target starts a block that is entered twice, `jal` targets seed functions and a `j` out of the function is a tail call. The machines without slots take the path they took. Checked against the fixtures' own symbols.
  acceptance: `cargo test -q -p ra-asm-analysis --test fixtures mipsel_functions` prints `test result: ok` with at least 2 tests: the functions found in hello_mipsel and control_mipsel are exactly their function symbols.
  files: crates/ra-asm-analysis/src/lib.rs crates/ra-asm-analysis/src/builder.rs crates/ra-asm-analysis/Cargo.toml crates/ra-asm-analysis/tests/fixtures.rs

- Jump tables on MIPS {c2}
  objective: A `switch` compiles to a bound check, a scaled load from a table and `jr`: the table is found from the `lui`/`addiu` pair or the gp-relative load that forms its address, and its bound from the `sltiu` that guards it.
  acceptance: `cargo test -q -p ra-asm-analysis --test fixtures mipsel_tables` prints `test result: ok. 1 passed`: control_mipsel's switch has its cases as successors.
  files: crates/ra-asm-analysis/src/jumptable.rs crates/ra-asm-analysis/tests/fixtures.rs

- ra-asm-lift lifts a MIPS image through the table {c2}
  objective: The lift's entry dispatches on the image's machine: for MIPS the module's state comes from the machine's `StateSpec` and each function from ra-asm-machine's block lifter. No MIPS instruction is named in ra-asm-lift.
  acceptance: `cargo test -q -p ra-asm-lift --test levels mipsel` prints `test result: ok`: every function of hello_mipsel and control_mipsel passes the machine level's checker with no trap.
  files: crates/ra-asm-lift/src/lib.rs crates/ra-asm-lift/src/state.rs crates/ra-asm-lift/Cargo.toml crates/ra-asm-lift/tests/levels.rs

- The Linux syscalls of a freestanding program in the interpreter {c2}
  objective: `ra_syscall` reaches the host with the machine's syscall convention from a platform table (number and arguments in which registers, the result and the error flag in which); `write` and `exit` for o32 are the first two rows of testdata/conformance/linux_syscalls.toml. hello_mipsel runs at the machine level.
  acceptance: `cargo test -q -p ra-asm-host --test interp hello_mipsel` prints `test result: ok. 1 passed`: the output equals testdata/expected/hello.txt.
  files: crates/ra-asm-host/src/linux.rs crates/ra-asm-host/src/lib.rs crates/ra-asm-host/tests/interp.rs testdata/conformance/linux_syscalls.toml

- o32 is an `Abi` constant {c2}
  objective: Four argument registers with their home space on the stack, two result registers, the callee-saved set, the link register as the return address, the 8-byte stack alignment, `$gp` as the global pointer, the float arguments in `$f12` and `$f14`. A table test, and no pass is touched: this is the card that shows the first section worked.
  acceptance: `cargo test -q -p ra-asm-arch --test abi o32` prints `test result: ok` with at least 2 tests, and `git diff --stat` of the card names no file under crates/ra-asm-lift/src/raise.
  files: crates/ra-asm-arch/src/abi.rs crates/ra-asm-arch/tests/abi.rs

- The global pointer is a constant of the image, as an assumption {c2}
  objective: Static MIPS code sets `$gp` once and addresses its small data through it. The promote step replaces reads of `$gp` by the image's value and records the assumption "gp is not changed", which the checked build asserts at each function's entry.
  acceptance: `cargo test -q -p ra-asm-lift --test assumptions gp` prints `test result: ok` with at least 2 tests: a gp-relative load of control_mipsel becomes a load from a global, and the assumption is listed once per function that used it.
  files: crates/ra-asm-lift/src/raise/promote.rs crates/ra-asm-lift/src/raise/assume.rs crates/ra-asm-lift/tests/assumptions.rs

- The MIPS fixtures verify at three levels {c2}
  objective: (generates) The function vectors of hello_mipsel and control_mipsel are recorded and committed, and replay at the machine, promoted and ABI levels and through the structured tree. What differs is fixed here if it is in the `Abi` constant, and is a pending line otherwise.
  acceptance: `cargo test -q -p ra-asm-verify --test fixtures mipsel` prints `test result: ok. 1 passed`; the test asserts both programs, every function their runs enter, at three levels and the tree, with testdata/vectors/fn/MIPS_PENDING.txt naming what differs.
  files: testdata/vectors/fn/hello_mipsel.fv testdata/vectors/fn/control_mipsel.fv testdata/vectors/fn/MIPS_PENDING.txt crates/ra-asm-verify/tests/fixtures.rs

- MIPS through the C output {c2}
  objective: The port of a freestanding Linux program needs two syscalls from the runtime, on any host: `ra_syscall` in rhipper.h maps `write` and `exit` to the C library. The two fixtures build with the host's clang and print their expected output.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle mipsel` prints `test result: ok` with 2 tests passed: hello_mipsel_runs_as_c and control_mipsel_runs_as_c.
  files: runtime/rhipper.h crates/ra-asm-emit/tests/oracle.rs

- Sixteen csmith programs for mipsel {c2}
  objective: (generates) The csmith seeds of phase 1 built with the freestanding runtime for mipsel at -O2, their checksums from `qemu-mipsel-static`, run in the interpreter at three levels with a pending list. The first wide net over a described machine.
  acceptance: `cargo test -q -p ra-asm-host --test csmith mipsel` prints `test result: ok. 1 passed`; the test asserts 16 programs at three levels.
  files: testdata/csmith/build_guest.sh testdata/csmith/bin crates/ra-asm-host/tests/csmith.rs testdata/csmith/PENDING.txt

### Big-endian

- The cursor reads in either byte order {c1}
  objective: ra-asm-format's cursor is little-endian by name. It gains a byte order, little by default, that every multi-byte read follows, so a loader states its order once.
  acceptance: `cargo test -q -p ra-asm-format --lib order` prints `test result: ok` with at least 4 tests: 16, 32 and 64 bits in both orders, and a refusal that still names the offset and the field.
  files: crates/ra-asm-format/src/lib.rs

- The ELF loader reads a big-endian image {c2}
  objective: (generates) The order comes from the ELF's identification and is kept on the `Image`; every reader of elf.rs goes through the cursor's. The fixture hello_mips is the freestanding hello built in WSL for big-endian MIPS, committed by this card with its expected output from `qemu-mips-static`.
  acceptance: `cargo test -q -p ra-asm-pe --test elf mips_be` prints `test result: ok` with at least 2 tests over testdata/bin/hello_mips: the entry and the symbols equal `llvm-readobj`'s, written in the test.
  files: crates/ra-asm-pe/src/elf.rs crates/ra-asm-pe/src/image.rs crates/ra-asm-pe/tests/elf.rs testdata/bin/hello_mips

- The interpreter's memory and the module carry the byte order {c2}
  objective: An IR load is in the module's byte order, as in LLVM's data layout. The printer writes `E` or `e`, the parser reads it, and the interpreter's `read_word` and `write_word` follow it, while the machine state stays in host order.
  acceptance: `cargo test -q -p ra-asm-ir --lib interp::order` prints `test result: ok` with at least 4 tests: a 32-bit store and load, a byte read of a stored word, a 128-bit value, a little-endian module unchanged.
  files: crates/ra-asm-ir/src/interp/memory.rs crates/ra-asm-ir/src/printer.rs crates/ra-asm-ir/src/parse.rs crates/ra-asm-ir/src/module.rs

- Big-endian MIPS vectors {c2}
  objective: (generates) The same rows under the big-endian spec, with vectors from `qemu-mips-static` (the harness's big-endian trampoline is the little-endian one reassembled) for the groups byte order touches: loads, stores and the unaligned ones.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors big_endian` prints `test result: ok. 1 passed`; the test asserts 12 rows and at least 480 cases.
  files: crates/ra-asm-mips/src/spec.rs crates/ra-asm-mips/tests/vectors.rs testdata/vectors/mips testdata/vectors/harness/run.sh

- A big-endian program runs in the interpreter {c1}
  objective: hello_mips at the machine, promoted and ABI levels; the syscalls of the little-endian program serve it unchanged, since the host reads guest memory through the module's order.
  acceptance: `cargo test -q -p ra-asm-host --test interp hello_mips_be` prints `test result: ok. 1 passed`.
  files: crates/ra-asm-host/tests/interp.rs crates/ra-asm-host/src/linux.rs

- The C of a big-endian program swaps at its loads and stores {c2}
  objective: The faithful port keeps the guest's memory in the guest's order: the C runtime's load and store macros take the order from a definition the port's header sets, and swap on a host of the other order.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle hello_mips_be_runs_as_c` prints `test result: ok. 1 passed`.
  files: runtime/rhipper.h crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/tests/oracle.rs

### The PlayStation

- The PlayStation fixtures {c2}
  objective: (generates) Two programs built with PSn00bSDK in WSL and committed: `hello_psx` prints through the BIOS, `gte_psx` transforms a cube's vertices through the GTE and prints the screen coordinates and the flags. With no emulator on the machine their expected output is derived from the source and marked evidence `source`; the manifest says what would replace it.
  acceptance: `cargo test -q -p ra-asm-testing --test manifest` prints `test result: ok`; the manifest lists testdata/bin/hello_psx.exe and testdata/bin/gte_psx.exe with evidence `source`.
  files: testdata/src/psx/hello.c testdata/src/psx/gte.c testdata/bin/hello_psx.exe testdata/bin/gte_psx.exe

- A PSX-EXE is an Image, with the memory map {c2}
  objective: The header (entry, the text's address and size, the initial gp and stack), the text as one segment in KUSEG, KSEG0 and KSEG1 as mirrors of the same 2 MB, and the BIOS, scratchpad and hardware-register ranges named as regions with no bytes.
  acceptance: `cargo test -q -p ra-asm-pe --test psx` prints `test result: ok` with at least 4 tests over hello_psx.exe: the entry, the segment, an address read through each mirror, a refusal of a truncated file that names the field.
  files: crates/ra-asm-pe/src/psx.rs crates/ra-asm-pe/src/image.rs crates/ra-asm-pe/src/lib.rs crates/ra-asm-pe/tests/psx.rs

- The BIOS's A0, B0 and C0 tables as rows {c2}
  objective: A PlayStation program calls its BIOS by jumping to 0xA0, 0xB0 or 0xC0 with the function's number in `$t1`. The three tables are rows of a platform file (table, number, name, prototype) that ra-asm-protos embeds and looks up; the first platform described outside Rust source.
  acceptance: `cargo test -q -p ra-asm-protos --test psx` prints `test result: ok` with at least 3 tests: at least 150 rows, no number twice in a table, `A0:0x3F` is `printf` with a variadic prototype.
  files: platforms/psx/bios.toml crates/ra-asm-protos/src/psx.rs crates/ra-asm-protos/src/lib.rs crates/ra-asm-protos/tests/psx.rs

- A BIOS stub is an import {c2}
  objective: The analysis recognizes the stub (`li $t1, n`, a jump to one of the three addresses, in either order around the slot) and gives the function the import's name and prototype, as it does for a PE thunk.
  acceptance: `cargo test -q -p ra-asm-analysis --test fixtures psx_bios` prints `test result: ok` with at least 2 tests: hello_psx's stubs are imports, `printf` and `exit` among them, and a jump to 0xA0 without a constant `$t1` is left alone.
  files: crates/ra-asm-analysis/src/imports.rs crates/ra-asm-analysis/tests/fixtures.rs

- The BIOS's console in the interpreter {c2}
  objective: The host's side for the calls a console program needs: `putchar`, `puts`, `printf` with the BIOS's format subset, `exit`, the heap calls. hello_psx runs at three levels.
  acceptance: `cargo test -q -p ra-asm-host --test interp hello_psx` prints `test result: ok. 1 passed`.
  files: crates/ra-asm-host/src/psx.rs crates/ra-asm-host/src/lib.rs crates/ra-asm-host/tests/interp.rs

- The GTE's registers, as a sub-machine {c3}
  objective: Coprocessor 2: 32 data and 32 control registers in the spec, `mfc2 mtc2 cfc2 ctc2 lwc2 swc2`, and the registers whose reads and writes have effects: the screen-coordinate queue pushed by a write to SXYP, IRGB and ORGB, the leading-zero count, the sign extension of the 16-bit fields.
  acceptance: `cargo test -q -p ra-asm-mips --test rows gte_registers` prints `test result: ok` with at least 8 tests, one per register with an effect.
  files: crates/ra-asm-mips/src/gte/registers.rs crates/ra-asm-mips/src/spec.rs crates/ra-asm-mips/tests/rows.rs

- A second GTE, written from the documentation {c3}
  objective: No machine here runs GTE instructions, so the rows get an independent reference: a plain Rust model of the GTE's commands over the same registers, in the test tree, written from the public documentation and not from the rows. It generates testdata/vectors/psx/gte.vec, evidence `manual`.
  acceptance: `cargo test -q -p ra-asm-mips --test gte_model` prints `test result: ok` with at least 6 tests of documented examples (a perspective transform with a known result, the limiter flags of an overflow, the division's table).
  files: crates/ra-asm-mips/tests/gte_model.rs crates/ra-asm-mips/tests/gte_model/commands.rs testdata/vectors/psx/gte.vec

- GTE rows: the perspective transforms {c3}
  objective: `RTPS` and `RTPT` with the shift fraction and limiter fields of the command word, the unsigned Newton-Raphson division, every bit of the flag register.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors gte_rtp` prints `test result: ok. 1 passed`; the test asserts at least 400 cases with flag bits set among them.
  files: crates/ra-asm-mips/src/gte/transform.rs crates/ra-asm-mips/tests/vectors.rs

- GTE rows: the geometry commands {c3}
  objective: `NCLIP AVSZ3 AVSZ4 OP SQR GPF GPL MVMVA` with MVMVA's matrix, vector and translation selectors, including the documented misbehaviour of the far-colour selector.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors gte_geometry` prints `test result: ok. 1 passed`; the test asserts 8 rows and at least 600 cases.
  files: crates/ra-asm-mips/src/gte/geometry.rs crates/ra-asm-mips/tests/vectors.rs

- GTE rows: light and colour {c3}
  objective: `NCS NCT NCDS NCDT NCCS NCCT CC CDP DCPL DPCS DPCT INTPL`, sharing three helpers (the light matrix, the colour matrix, the depth cue) as the hardware does.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors gte_light` prints `test result: ok. 1 passed`; the test asserts 12 rows and at least 800 cases.
  files: crates/ra-asm-mips/src/gte/light.rs crates/ra-asm-mips/tests/vectors.rs

- The GTE program runs in the interpreter {c1}
  objective: gte_psx at three levels: the first program whose output depends on every group of GTE rows at once. A difference here that the vectors did not show is a missing case, and is added to the model's cases before it is fixed.
  acceptance: `cargo test -q -p ra-asm-host --test interp gte_psx` prints `test result: ok. 1 passed`.
  files: crates/ra-asm-host/tests/interp.rs

- The PlayStation programs' vectors {c1}
  objective: (generates) The function vectors of hello_psx and gte_psx committed and replayed at three levels and through the tree.
  acceptance: `cargo test -q -p ra-asm-verify --test fixtures psx` prints `test result: ok. 1 passed`; the test asserts both programs and every function their runs enter.
  files: testdata/vectors/fn/hello_psx.fv testdata/vectors/fn/gte_psx.fv crates/ra-asm-verify/tests/fixtures.rs

- The PlayStation programs as C {c2}
  objective: The port's runtime gains the BIOS console (runtime/psx.h: the calls of the interpreter's host, in C). The GTE needs nothing: its rows are outlined IR functions and print as C like the program's own. Both programs build with the host's clang and print their expected output.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle psx` prints `test result: ok` with 2 tests passed: hello_psx_runs_as_c and gte_psx_runs_as_c.
  files: runtime/psx.h runtime/driver.c crates/ra-asm-emit/tests/oracle.rs

### The Emotion Engine

- The R5900 machine: 128-bit registers, the second hi and lo, the shift amount {c2}
  objective: The spec and state of the EE's processor: 32 general registers of 128 bits of which the MIPS rows touch the low 64, `hi1` and `lo1`, `sa`, a floating-point unit of 32 single-precision registers with its accumulator. The core rows are instantiated for it unchanged.
  acceptance: `cargo test -q -p ra-asm-mips --test rows r5900_spec` prints `test result: ok` with at least 4 tests: `addu` writes the low 64 bits sign-extended and leaves the high 64, `check(table)` passes for the R5900 table.
  files: crates/ra-asm-mips/src/r5900/spec.rs crates/ra-asm-mips/src/lib.rs crates/ra-asm-mips/tests/rows.rs

- Rows: the 64-bit instructions of MIPS III {c2}
  objective: (generates) `daddu daddiu dsubu dadd daddi dsub dsll dsrl dsra dsll32 dsrl32 dsra32 dsllv dsrlv dsrav ld sd lwu ldl ldr sdl sdr`, with vectors from the harness under `qemu-mips64el-static`, which needs the 64-bit trampoline this card adds.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors mips3` prints `test result: ok. 1 passed`; the test asserts 22 rows and at least 880 cases.
  files: crates/ra-asm-mips/src/rows/mips3.rs testdata/vectors/harness/mips64el.S testdata/vectors/mips64el/mips3.vec

- Rows: what the R5900 adds to the core {c2}
  objective: The three-operand `mult` and `multu`, `mult1 multu1 div1 divu1 madd maddu madd1 maddu1 mfhi1 mflo1 mthi1 mtlo1`, `movz movn`, `mfsa mtsa mtsab mtsah`, `lq sq`. Their reference is a second model written from the instruction manual (evidence `manual`), as for the GTE.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors r5900_core` prints `test result: ok. 1 passed`; the test asserts 22 rows and at least 800 cases.
  files: crates/ra-asm-mips/src/r5900/core.rs crates/ra-asm-mips/tests/r5900_model.rs testdata/vectors/r5900/core.vec

- MMI rows: parallel add, subtract, compare, minimum and maximum {c2}
  objective: `paddb paddh paddw psubb psubh psubw` with their signed and unsigned saturating forms, `pcgtb pcgth pcgtw pceqb pceqh pceqw pmaxh pmaxw pminh pminw pabsh pabsw padsbh`. Each is one helper applied per lane, which is how the rows are written.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors mmi_add` prints `test result: ok. 1 passed`; the test asserts at least 31 rows and 1 200 cases with saturation at both ends among them.
  files: crates/ra-asm-mips/src/r5900/mmi_add.rs crates/ra-asm-mips/tests/r5900_model.rs testdata/vectors/r5900/mmi_add.vec

- MMI rows: shifts and logic {c2}
  objective: `psllh psrlh psrah psllw psrlw psraw psllvw psrlvw psravw pand por pxor pnor plzcw qfsrv`.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors mmi_bits` prints `test result: ok. 1 passed`; the test asserts 15 rows and at least 600 cases.
  files: crates/ra-asm-mips/src/r5900/mmi_bits.rs crates/ra-asm-mips/tests/r5900_model.rs testdata/vectors/r5900/mmi_bits.vec

- MMI rows: pack, extend, interleave, copy and exchange {c3}
  objective: `pextlb pextlh pextlw pextub pextuh pextuw pext5 ppacb ppach ppacw ppac5 pinth pinteh pcpyld pcpyud pcpyh pexeh pexew pexch pexcw prevh prot3w`: pure permutations of lanes, each row a table of which lane goes where.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors mmi_pack` prints `test result: ok. 1 passed`; the test asserts 22 rows and at least 900 cases.
  files: crates/ra-asm-mips/src/r5900/mmi_pack.rs crates/ra-asm-mips/tests/r5900_model.rs testdata/vectors/r5900/mmi_pack.vec

- MMI rows: multiply and divide {c3}
  objective: `pmultw pmultuw pmulth pmaddw pmadduw pmaddh pmsubw pmsubh phmadh phmsbh pdivw pdivuw pdivbw pmfhi pmflo pmthi pmtlo pmfhl pmthl` with the placement of the partial results in hi and lo that each form documents.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors mmi_mul` prints `test result: ok. 1 passed`; the test asserts at least 19 rows and 800 cases.
  files: crates/ra-asm-mips/src/r5900/mmi_mul.rs crates/ra-asm-mips/tests/r5900_model.rs testdata/vectors/r5900/mmi_mul.vec

- Rows: the EE's floating point, which is not IEEE {c3}
  objective: The EE's unit has no denormals, infinities or NaNs: results clamp to the largest finite value and the flags say so; `madd.s msub.s adda.s suba.s mula.s rsqrt.s max.s min.s` exist beside the usual set. Written as their own rows over the single-precision registers, with the clamps explicit, and never by reusing the host's float semantics unchecked.
  acceptance: `cargo test -q -p ra-asm-mips --test vectors r5900_fp` prints `test result: ok. 1 passed`; the test asserts at least 30 rows and 1 500 cases with an overflow, an underflow and a division by zero for each arithmetic row.
  files: crates/ra-asm-mips/src/r5900/fp.rs crates/ra-asm-mips/tests/r5900_model.rs testdata/vectors/r5900/fp.vec

- An EE ELF is an Image, with the memory map {c2}
  objective: The ELF of the EE's machine variant at 32-bit addresses: its segments in the 32 MB of main memory with the kernel-segment mirrors, the scratchpad, and the hardware-register ranges named. A fixture `hello_ee.elf` built with ps2sdk in WSL, printing through the EE's serial port, is committed by this card (evidence `source`).
  acceptance: `cargo test -q -p ra-asm-pe --test elf emotion_engine` prints `test result: ok` with at least 3 tests over testdata/bin/hello_ee.elf.
  files: crates/ra-asm-pe/src/elf.rs crates/ra-asm-pe/src/image.rs crates/ra-asm-pe/tests/elf.rs testdata/bin/hello_ee.elf

- The EE kernel's syscalls as rows {c2}
  objective: The kernel's calls as a platform file (number, name, prototype), with the negative numbers of the interrupt-safe variants, embedded and looked up by ra-asm-protos as the BIOS tables are.
  acceptance: `cargo test -q -p ra-asm-protos --test ps2` prints `test result: ok` with at least 3 tests: at least 100 rows, no number twice, `FlushCache` and `iSignalSema` present under their numbers.
  files: platforms/ps2/ee_syscalls.toml crates/ra-asm-protos/src/ps2.rs crates/ra-asm-protos/src/lib.rs crates/ra-asm-protos/tests/ps2.rs

- An EE syscall stub is an import {c1}
  objective: `li $v1, n; syscall; jr $ra` is recognized as the import the table names, through the mechanism the BIOS stubs introduced.
  acceptance: `cargo test -q -p ra-asm-analysis --test fixtures ee_syscalls` prints `test result: ok` with at least 2 tests over hello_ee.elf.
  files: crates/ra-asm-analysis/src/imports.rs crates/ra-asm-analysis/tests/fixtures.rs

- The EE's calling convention is an `Abi` constant {c1}
  objective: Eight argument registers of 64 bits, the 128-bit values passed by reference, the float arguments in `$f12` on, the callee-saved set. Again a table and no pass.
  acceptance: `cargo test -q -p ra-asm-arch --test abi eabi64` prints `test result: ok` with at least 2 tests.
  files: crates/ra-asm-arch/src/abi.rs crates/ra-asm-arch/tests/abi.rs

- The first device is two registers {c2}
  objective: The EE's serial port, a transmit register and a status register, as the first device description: a file of registers and the effect of a write, which the interpreter's host maps to standard output. The smallest instance of what phase 7 does for every device, and enough for hello_ee to run at three levels.
  acceptance: `cargo test -q -p ra-asm-host --test interp hello_ee` prints `test result: ok. 1 passed`: hello_ee.elf at the ABI level prints its expected text.
  files: platforms/ps2/sio.toml crates/ra-asm-host/src/ps2.rs crates/ra-asm-host/src/lib.rs crates/ra-asm-host/tests/interp.rs

- The EE program's vectors {c1}
  objective: (generates) hello_ee's function vectors committed and replayed at three levels and through the tree, 128-bit registers included in the vector format since phase 1.
  acceptance: `cargo test -q -p ra-asm-verify --test fixtures emotion_engine` prints `test result: ok. 1 passed`.
  files: testdata/vectors/fn/hello_ee.fv crates/ra-asm-verify/tests/fixtures.rs

- The EE program as C {c2}
  objective: The C of a program with 128-bit registers: `ra_u128` in the runtime as the compiler's `__int128` where it has one and a two-word struct with helper functions where it does not (MSVC), and the serial port as two lines of runtime/ps2.h. The multimedia rows are outlined IR functions and need nothing of their own.
  acceptance: `cargo test -q -p ra-asm-emit --features oracle --test oracle hello_ee_runs_as_c` prints `test result: ok. 1 passed`.
  files: runtime/rhipper.h runtime/ps2.h crates/ra-asm-emit/src/c.rs crates/ra-asm-emit/tests/oracle.rs

### The ARM lifts get their vectors

- The ARM64 forms the lift covers, as assembled cases {c2}
  objective: (generates) Milestone K was accepted on `hello`-sized programs, and arm64.rs (1 368 lines) has no instruction-level check. Its arms are listed as assembly templates with operand classes, assembled by `llvm-mc` in WSL into cases in the vector format's input half, committed.
  acceptance: `cargo test -q -p ra-asm-lift --test vectors_gen arm64_cases` prints `test result: ok. 1 passed`; the test asserts a case for every mnemonic arm64.rs has an arm for, by reading the arm list as the x86 oracle does.
  files: crates/ra-asm-lift/tests/vectors_gen.rs testdata/vectors/arm64/templates.txt testdata/vectors/arm64/cases

- ARM64 vectors from `qemu-aarch64-static` {c3}
  objective: (generates) The harness's AArch64 trampoline (the general registers, the flags, the vector registers), the cases run, the vectors committed, the forms that differ from the lift in a pending list for `ras cards`.
  acceptance: `cargo test -q -p ra-asm-lift --test vectors arm64` prints `test result: ok. 1 passed`; the test asserts at least 4 000 cases replayed.
  files: testdata/vectors/harness/aarch64.S testdata/vectors/arm64 crates/ra-asm-lift/tests/vectors.rs

- ARM32 and Thumb-2 vectors {c3}
  objective: (generates) The same for thumb.rs: templates in both instruction sets, a trampoline that enters each, conditional execution and IT blocks among the cases.
  acceptance: `cargo test -q -p ra-asm-lift --test vectors arm32` prints `test result: ok. 1 passed`; the test asserts at least 3 000 cases replayed.
  files: testdata/vectors/harness/arm.S testdata/vectors/arm32 crates/ra-asm-lift/tests/vectors_gen.rs crates/ra-asm-lift/tests/vectors.rs
```

## Appendix B. Phases 4 to 11, as milestones and sections

These are not cut into cards, on purpose. A card written today for phase 7 would name files that phase 3 has not created and would be wrong by the time it ran; the record of this roadmap's own regeneration says the same thing. Each phase is cut when the one it depends on is half accepted, by the regeneration card (140), with the rules of 3.7 and with `ras cards --pending` supplying every card whose shape is "make this pending row pass". What is fixed here is the order, the sections, what each section is for, and the measure of each phase in 4.1.

The headings are in the format of Appendix A. Whether `rc import` accepts an `objective:` line directly under a milestone or a section heading was not checked here (this document was written without touching `../rhipper-agent`): if it refuses them, strip those lines for the import and keep this appendix as the sections' notes.

```markdown
## Types and data

objective: Every value in the emitted source has a type that was inferred from evidence and that the evidence can be shown for, and every structure the program lays out in memory has a declaration; the measure is the share of loads and stores in the corpus written as a field or an element instead of pointer arithmetic, against the fixtures' own debug information.

### Ground truth: the fixtures' debug information is the answer key
objective: Every fixture is built once more with full debug information, and a reader in ra-asm-testing turns its PDB or DWARF into the expected type of each parameter, local and global, so that inference has a score per function before it has a single rule.

### The constraint graph
objective: `ra-asm-types` collects, per function at the ABI level, the constraints the code states (this value is dereferenced at offset 8 with width 4, these two values flow together, this one is passed to a parameter of known type), as plain data with the instruction that states each one.

### Solving, within a function
objective: A unification solver with subtyping over the lattice of widths, signedness, pointers and floats, which never picks when the evidence does not decide: an undecided value stays an integer of its width and is counted.

### Solving, across the program
objective: Function summaries carry type schemes, so that a callee's parameters and a caller's arguments constrain each other through the store's queries with early cutoff, and a change to one function reinfers only those whose summaries change.

### Structures from access patterns
objective: The offsets used through one pointer become the fields of a structure, arrays are told from structures by the stride of an index, and two structures are merged only when a value flows between them.

### The allocation site names the size
objective: A structure born at `malloc(48)`, `operator new`, a stack slot or a global has its size from there, which closes the open end every access-pattern method has.

### Classes: tables, constructors, hierarchy
objective: `rtti.rs` and the class evidence of milestone Q meet inference: the virtual table gives the methods, the constructor gives the layout and the base, and the emitted C++ declares the class with its real members where today it declares a table of pointers.

### Unions, variants and tagged data
objective: A location used at two incompatible types under a dominating test of a tag becomes a tagged union with the tag named; without such a test it becomes an explicit union and is counted against the measure.

### Types from the boundary
objective: The prototype tables of every platform (Win32, the C library, the BIOS and kernel rows of phase 3) seed inference with exact types, and those propagate inward: a handle that reaches `ReadFile` is a `HANDLE` wherever it has been.

### Globals and the data sections
objective: The data sections stop being byte arrays: each global has its extent from the accesses and relocations that reach it, its type from inference, and an initializer printed in that type.

### Strings, tables and constant data
objective: Strings in every encoding the corpus uses, jump tables, virtual tables, floating-point constants and lookup tables are recognized as what they are and printed so.

### Types are checked, not trusted
objective: A typed function is re-verified by its function vectors at the typed level, and a layout assumption (no access to this structure outside its fields) enters the assumption ledger and is asserted by the checked build.

### The person's types
objective: A type the person or a model names, corrects or imports from a header is a fact in the store with its author, outranks inference, and reinfers what depends on it; a correction that contradicts the evidence is refused with the instruction that contradicts it.

## The source

objective: The output reads as a programmer of the original would have written it and is proved, function by function, to be the same program; the measures are the readability score against the fixtures' own source (3.9), and the share of functions with evidence `proved` or `matched`.

### The idiom engine: a rewrite is a rule and a proof obligation
objective: `ra-asm-idiom` holds rewrites as data (a pattern over the typed level, a replacement, side conditions), each checked once for all inputs by the solver and again on every application by the function's vectors, so a rule cannot be added that changes behaviour.

### A solver in the verifier
objective: `ra-asm-verify` gains an encoding of two IR functions' equivalence into SMT (the person decides whether the solver is Z3 as a process or a pure Rust one; 4.3), with a time limit per query and the ledger recording `proved`, `refuted` with the counterexample as a new vector, or `timeout`.

### Undoing the compiler: arithmetic
objective: Division and remainder by constants from their multiply-and-shift forms, strength-reduced multiplications, sign and zero extension dances, branchless selects, bit tests, rotations and byte swaps, each as a rule with its proof.

### Undoing the compiler: loops
objective: Induction variables rejoined, unrolled and peeled bodies folded back, rotated loops restored to `for` and `while`, vectorized loops returned to their scalar source when the scalar form is proved equal.

### Undoing the compiler: calls
objective: Inlined library routines (`memcpy`, `strlen`, the string and math intrinsics) recognized by proof against the routine and replaced by the call; tail calls, shrink-wrapped prologues and outlined cold paths rejoined.

### 64-bit arithmetic on 32-bit machines, and its cousins
objective: The pairs of registers a 32-bit compiler used for a 64-bit value become one variable, the runtime helpers (`_alldiv`, `__divdi3`, soft float) become operators, and the same for 128-bit and fixed-point patterns the consoles use.

### Structuring without `goto`
objective: `structure.rs` is replaced by a structurer over the typed level with a proof that its tree runs as its graph (the tree interpreter of phase 1), short-circuit conditions rebuilt, `switch` with fallthrough, `break` and `continue` to labels where the language has them, and a counted `goto` only where the original source had one.

### Extraction by cost
objective: When the fixed schedule of rewrites becomes the source of order bugs, a function's alternatives are kept in an equality graph and the form with the lowest readability cost is extracted, with the cost model fitted against the ground-truth corpus's own source; the rules do not change.

### Expressions and variables
objective: Temporaries fold into expressions where a person would have folded them, variables are split by live range and merged by copy, and declarations sit at first use in the narrowest scope.

### Names
objective: A name comes from evidence in a stated order (symbols, debug records, exports, RTTI, strings logged beside the value, the API parameter it is passed to, the field it is stored in, a model's proposal) and carries where it came from; a model's name is a fact like the person's and never changes behaviour.

### Files and modules
objective: Functions are grouped into files by the object boundaries recovered from the link order, the static libraries' signatures, the string and RTTI evidence, and call clustering, with headers declaring what crosses a file and nothing else.

### Language idiom: C
objective: The C backend prints the typed tree as C a reviewer would accept: `for` loops, array indexing, field access, `const`, `static`, `bool`, `enum`s from the constants compared against, no casts that the types make redundant.

### Language idiom: C++
objective: Classes with their methods, constructors and destructors, virtual dispatch as calls, exceptions as `try` and `catch`, the standard containers of milestone Q recognized by layout and used by name.

### Language idiom: Rust
objective: The Rust backend's unsafe surface shrinks by proof: a pointer whose provenance and bounds the verifier establishes becomes a reference or a slice, and the count of `unsafe` blocks is a line of the scoreboard.

### Toolchain packs: the original compiler as an oracle
objective: A pack names a compiler by version and flags and how to run it (MSVC 6 through today's, the GCC of each console SDK, CodeWarrior, SN Systems), and `ras match` compiles an emitted function with it and compares the bytes with the original's, which is evidence `matched`: the strongest kind there is, since it says the source is not merely equivalent but the source.

### Matching search
objective: Where the first compile does not match, a bounded search over the equivalent forms the idiom engine knows (the order of declarations, `for` or `while`, the signedness of a counter) guided by the difference in the assembly, the permuter's work done with a prover behind it.

### The readability score
objective: `ras score --source` (phase 2) becomes a measure a card can move: tokens per function, `goto`s, casts, unnamed values, and the tree edit distance from the fixture's own source after normalization, so "more readable" is a number that only falls.

## The store and scale

objective: A program of two hundred thousand functions opens in seconds, an edit recomputes only what depends on it, and the whole corpus is processed in parallel within the machine's memory; the measures are cold and warm times on the real program and peak memory against a committed ceiling.

### The query store
objective: `ra-asm-db`, hand-written and small: a query is a function of keys to a value with its dependencies recorded as it runs, a red-green revalidation with early cutoff when a recomputed value is equal, and every stage of the pipeline moved behind queries one at a time with the old path as its differential oracle until it is deleted.

### Function units and summaries
objective: The unit of work, caching and proof is one function at one level with the summaries of its callees, so that the passes that are whole-module today (`infer`, `known`, `prototypes`) run bottom-up over the call graph's strongly connected components.

### Persistence
objective: The store is a directory under the project (content-addressed blobs and an index), written atomically and versioned by the hash of the pass that produced each value, so a new `ras` reuses everything a changed pass did not touch.

### Memory
objective: The image is mapped, not read; IR for a function lives in an arena that is dropped when its queries finish; and a ceiling on resident memory is enforced by evicting to the store, which is card 131's intent with a mechanism under it.

### Parallelism
objective: Queries run on a work-stealing pool bounded by the machine's cores and memory, with results identical to the serial run, which a test checks by running the fixtures both ways and comparing the stores byte for byte.

### Incremental sessions
objective: `ras serve` answers the workbench from the store: renaming a function, retyping a field or correcting a boundary re-emits the files it touches and re-verifies the functions it touches, and reports both lists.

### Programs of many images
objective: An executable with its libraries, a console disc with its overlays and modules, a firmware with its stages are one `Program` whose images share types, names and a call graph, loaded at the addresses and epochs the medium says.

### Sibling builds
objective: `ras relate` matches the functions of two builds of one program (another region, another version, another platform, a debug build with symbols) by the structure of their code, and carries names, types and proofs across; one labelled build labels the rest.

### The corpus at scale
objective: `ras smoke` grows from the system directory to a committed list of public programs per platform with a census of where each one stops, run nightly, so that the next most valuable card is always the top line of a table.

## Platform runtimes and hardware

objective: A port of a console program runs on a PC with its picture, sound and input, from a platform described as data; the measure is the homebrew corpus of each platform running to its expected frames and samples, and then one commercial program per platform reaching its title screen from a recorded session.

### The platform interface
objective: `ra-asm-platform` fixes the three events of the boundary (`call`, `access`, `deliver`) and derives from one description the interpreter's host, the port's runtime, the recorder, the replayer and the comparison point; cards 96 and the Win32 session work of phase 2 are its first two clients.

### Devices as descriptions
objective: A device is a register map with fields, the effect of each read and write as rows in the machine table's builder, and its timing in the platform's clock, checked by replaying committed register traces; the serial port of phase 3 is restated in it.

### The clock, interrupts and DMA
objective: Time is counted in the cycles the machine rows carry, events are scheduled against it, interrupts are delivered at the points the port can observe them, and DMA is a device like the others; determinism is the property tested, by running every fixture twice and comparing.

### Threads and the scheduler
objective: The guest's threads (Win32's, the EE kernel's, the PSP's) are host threads when the program's synchronization allows it and coroutines stepped by the recorded schedule when replaying, and a data race found by a replay that diverges is reported with both accesses.

### Graphics: the PlayStation's GPU
objective: The command stream to a software rasterizer that is exact against committed frames, then to the host's graphics API with the same frames as its oracle; the first device where "the same behaviour" needs a tolerance, which is stated per test and never global.

### Sound: the SPU
objective: Voices, ADPCM decoding, the envelope and the reverb as a device description producing samples that are compared bit for bit with a reference recording of the fixture.

### Input, storage and the disc
objective: Pads as `deliver` events from the host's devices or the session file, memory cards as files, the disc as the medium's file tree with its timing, and streamed audio and video through the decoder the program itself carries.

### The PlayStation 2: vector units
objective: The VU instruction set as rows (cards 84 and 151), microprograms found in the data they are uploaded from and lifted as functions of an epoch, and VU0's macro mode as part of the EE's table.

### The PlayStation 2: the IOP and its modules
objective: The second processor is the phase 3 MIPS core with its own image set (IRX modules, card 85); the two processors' ports talk through the SIF as a described device, and the common modules are ported once and shared by every program.

### The PlayStation 2: the GS and the rest
objective: The graphics synthesizer's registers and primitives to a rasterizer with committed frames, the GIF and VIF paths that feed it, SPU2, the IPU's decoder.

### Windows beyond the console: windows, GDI, Direct3D, DirectSound
objective: The real program's platform: the window and message layer through the adapters of phase 2, Direct3D 8 and 9 calls recorded as a session and replayed on the host's own Direct3D or on a translation layer, so the port is a native 64-bit program with no 32-bit dependency.

### High-level replacement, proved
objective: Where a described device or a ported library function has a host equivalent (a `memcpy`, a matrix multiply, a decoder), the replacement is a rule with the same proof obligation as an idiom; a replacement that cannot be proved is offered to the person as a choice and recorded as an assumption.

### The game, not the homebrew
objective: One commercial program per platform, supplied by the person from their own copy, carried from boot to its title screen and then through a recorded session to the session's end, with every stop on the way a row in a census and the top row the next card (card 100).

### The browser
objective: The port and its runtime compiled to WebAssembly with the platform layer on the web's own graphics, sound and input (card 101), which costs one more backend of the platform description and nothing in the engine.

## Dynamic code

objective: A program that writes, decrypts, loads or generates code at run time is ported as completely as one that does not; the measure is a corpus of such programs (packed, self-patching, with a recompiler, with a bytecode machine) whose ports run their sessions with the residual engine's entered count at zero for the first two tiers, confined to the generated code for the third, and with the fourth's scripts emitted as source that passes their own tests.

### Epochs
objective: An address space has versions: the bytes at an address at one time and at another are two epochs, discovered by the recorder (a write to a page that was executed), each analysed as an image of its own, with the transitions between them as edges of the program's graph.

### Tier one: code revealed once
objective: Packers, decryptors and loaders that write the program and then run it (phase 2's UPX work, generalized): the final epoch is the program, the earlier ones are ported as the loader they are, or dropped when the person asks and the proof holds that nothing else reads them.

### Tier two: a finite set of variants
objective: Code that patches itself among a finite set of forms (a call site rewritten after first use, an instruction's operand changed by a mode) is ported as each form, with the patching write turned into a switch of a variable and the equivalence checked by the session.

### Tier three: code generated at run time
objective: A dynamic recompiler or a JIT whose output depends on run-time input has no finite static port: the generator itself is ported, what it generates runs through the residual engine, every version that proves stable across sessions joins the next static build, and the promise is limited to exactly that.

### Tier four: a machine inside the program
objective: A program with a bytecode interpreter or a scripting virtual machine: the dispatch loop is recognized, the bytecode's rows are derived from its handlers into a machine description, and the scripts on the disc are lifted and ported as programs of that machine, which is the same engine pointed at a machine it has just learned; where the derivation fails, the port is a correct port of an interpreter and says so.

### The residual engine grows up
objective: `ra-asm-residual` from phase 2 (an interpreter over the machine level, linked into the port) gains a cache, a lift at run time for hot code, and a log of everything it ran which, fed back to `ras`, turns the next build's residue into static code.

### Coverage-directed discovery
objective: Code the static analysis and the sessions have not reached is listed, and for each entry `ras` proposes the input that would reach it (by solving the path condition where it can, by asking the person where it cannot), so "what have we not seen" is a shrinking list.

### Protection, within the line
objective: Obfuscated control flow, opaque predicates and virtualized functions are tier two and tier four problems and are treated as such, on what the person supplies and what that program does by itself when run: asm carries no keys, emulates no licence server, and bypasses no online check.

## Every machine, every container

objective: A new machine is a table and a new container is a parser, each landed by a short chain of small cards with vectors as their check; the measure is, per machine, the share of its rows with executable evidence and the fixture corpus ported, and the count of lines outside the machine's own crate that its arrival changed, which should be zero.

### The rest of MIPS: the PSP and the Nintendo 64
objective: Allegrex's additions and VFPU as rows with a PRX loader (cards 86 and 147), the VR4300 with the ROM's byte orders and its boot segment (cards 87 and 148), and the RSP's vector instruction set with microcode as epochs (card 152); the first proof that a second and third machine of a family cost only their differences.

### PowerPC: the GameCube, the Wii, the Xbox 360, the PlayStation 3
objective: One table for the family with the paired singles of Gekko and Broadway, VMX and VMX128, and the Cell's PPU; DOL, REL, XEX and SELF as containers (cards 89, 149, 90, 91), with XEX and SELF limited to images the person has already decrypted.

### SH: the Dreamcast, the Saturn, the 32X
objective: SH-4 and SH-2 as one table (cards 143, 153), with their delay slots through the lifter phase 3 built, and the Dreamcast's and Saturn's containers and boot files (card 93).

### 68000: the Mega Drive, the Amiga, the classic Macintosh, the arcade boards
objective: The first machine with variable-length instructions built from extension words, which the row format's second generation must describe; containers for ROM images, Hunk files and the Macintosh's resource forks.

### The 8-bit and 16-bit machines: 6502, 65816, Z80, the Game Boy's processor
objective: Machines where code and data share one bank-switched address space and the program counts its own cycles: banks are epochs, the mapper is a device, and the port's clock is exact because the table carries the cycles.

### ARM before Thumb-2: the Game Boy Advance, the DS, the 3DS
objective: ARMv4T through ARMv6 as a table of their own beside the hand-written lifts, which are then replaced by it once its vectors cover them; the two-processor DS as a program of two images.

### x86 beyond Windows: DOS, the original Xbox, Linux, macOS
objective: Real mode and segments for DOS programs with their extenders as loaders, the Xbox's kernel exports as a platform (card 92 gave the container), ELF and Mach-O programs with their C libraries' boundary.

### 64-bit ARM and x86 today: the phone, the Switch, the current consoles
objective: Mobile executables (Mach-O and ELF in their app packages), the Switch's NSO and NRO, and the present generation's formats, for images the person supplies decrypted; the machines are the ones asm already lifts, so the work is containers and platform boundaries.

### RISC-V and the microcontrollers
objective: RV32 and RV64 with their extensions, Cortex-M, AVR, MSP430, Xtensa: firmware as a flat image with a vector table, a memory map supplied or inferred from the vendor's description files, and devices as the peripherals of the chip.

### Firmware containers and the blob
objective: Images with no header: the machine is guessed from instruction statistics, the base from the pointers that land on function starts, and the layout from the strings and tables, each guess shown with its evidence and overridable.

### Managed and bytecode programs
objective: Programs for a virtual machine whose format keeps its types and names (.NET, the JVM, Dalvik, Flash, the scripting engines of games) are a different and easier problem that existing tools solve well; asm reads them as machines with rich metadata where a program mixes them with native code, and otherwise points at those tools.

### Importing descriptions
objective: Where a machine has a published formal description (SLEIGH, Sail, the vendors' machine-readable specifications), an importer generates rows from it and the rows are then checked by the same vectors as hand-written ones, so an import is a source of drafts and never of trust.

### Language runtimes
objective: Go, Rust, Swift, Objective-C, Delphi and the C++ runtimes of each compiler leave structure in a binary (type tables, method lists, string headers, calling conventions of their own): each is a pack of recognizers and `Abi` constants that turns that structure into types and names.

## Liberation

objective: The faithful port, which keeps the original's memory layout, addresses and pointer width, becomes a native program with none of them; the measure is the share of the program's memory accesses that no longer go through the guest address space, reaching all of them for a fixture and then for the real program, with every session still replaying.

### Provenance: every pointer has an origin
objective: For each pointer-typed value, the object it points into (a global, a stack slot, an allocation site, a mapped device) is established by the type inference of phase 4 and the sessions, and the pointers for which it is not are listed with the reason.

### Globals leave the address space
objective: A global whose every access is by name becomes a host variable of its type, placed by the host's linker, and the address it had is gone; the assumption that no computed pointer reaches it is discharged by proof or asserted by the checked build.

### The stack leaves the address space
objective: A function whose frame does not escape gets host locals and the host's stack, and the guest stack pointer disappears from its signature; the ones whose frames escape keep a guest frame and are counted.

### The heap leaves the address space
objective: Allocations whose pointers have full provenance come from the host's allocator at the host's pointer width, structures are re-laid out for the host, and the sizes passed to the allocator are recomputed from the types.

### Pointer width
objective: A 32-bit program's pointers become 64-bit where every structure holding them is liberated, and stay 32-bit indices into an arena where a layout is observable (written to a file, sent over a network, hashed), which is a boundary the sessions reveal.

### Byte order and alignment
objective: A big-endian program's data becomes host-order where it is typed and private, with the swaps moved to the boundary where files and packets are read and written, proved equal by the sessions that cross that boundary.

### Function pointers and tables
objective: Indirect calls through recovered function-pointer types become host function pointers, and the address-to-function table of the faithful port shrinks to the addresses that are stored as data or computed.

### The checked build is the safety net
objective: Every liberation is an assumption with an assertion in the checked build; a nightly run of all sessions under it is what lets a card liberate a class of objects without proving each, and any assertion that fires becomes a vector and a pending line.

### The mixed program
objective: Liberation is per object and per function, never all at once: liberated and faithful code call each other through generated shims that the store tracks, so the program builds and runs its sessions after every single card.

### Off the original's libraries
objective: Dependencies ported as second images in phase 2 (a C++ runtime under its old layout) are replaced by the host's own once the types that cross the boundary are liberated, and the port links against nothing from the original platform.

## The living model

objective: The program's model outlives the port: it is the thing people and models query, edit and extend, and the source is one rendering of it; the measure is what a person can do in a session without leaving it: ask why, change a fact, see what the change touched, and get a build that still passes.

### The model is an interface
objective: Everything in the store is reachable by a stable, documented query interface (functions, types, evidence, assumptions, epochs, sessions, the reason for each name), which `ras serve`, the workbench and an agent's tools all use, and which is versioned so that a project opened in five years still opens.

### Why: every fact answers for itself
objective: `ras why <thing>` prints the chain behind any line of output: the instruction it came from, the passes that shaped it, the evidence for each step, the assumptions it rests on, who named it; nothing in the output is unexplained.

### The person and the model as authors
objective: A human edit to the emitted source (a rename, a rewritten function, a restructured file) is read back as facts and as a new candidate for that function, verified against its vectors like any pass's output, and kept if it holds; the generated code and the hand-written code are the same kind of thing.

### Agents on the program
objective: `ras cards` cuts the work on a port (the functions still at the machine level, the open assumptions, the unnamed, the unmatched) into cards with a per-function acceptance, so the same agent that builds asm improves a port with it, in parallel, with the verifier as the reviewer.

### Changing the program
objective: A modification (a fix, a widescreen patch, a new platform's input) is made in the source and checked by the sessions it should not affect and the ones it should, with the difference in behaviour reported as the session events that changed.

### Back to the original
objective: A change made in the source is compiled by the toolchain pack and patched into the original binary where the person wants the original format, which the matching work of phase 5 makes exact for matched functions.

### Programs in relation
objective: The models of sibling builds, sequels, engines shared across games and libraries shared across everything are linked: a function understood once is understood everywhere it occurs, and a public index of library and runtime functions grows with every project that agrees to contribute to it.

### The archive
objective: A project directory is self-contained and self-describing (the inputs' hashes, the store, the sessions, the tool versions, the proofs), so that the understanding of a program is preserved in a form that needs neither the original hardware nor this year's tools to be read.
```

## Closing

The first card of this document is forty lines of Rust that write a failing test's output to a file. The last milestone is a preserved, proved, native, living model of any program ever shipped. What joins them is one habit, kept at every scale: nothing is claimed that a small, fast, local check cannot show, and the tool always knows, and says, which of its claims have such a check and which do not.
