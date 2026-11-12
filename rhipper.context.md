# Rhipper Asm

The disassembly and lifting core of Rhipper, in pure Rust: a Windows PE image in, an LLVM-IR-shaped IR out, and C, C++ or Rust source out of that IR that builds and runs. It replaces Ghidra and the Python pipeline in the Rhipper Workbench (`../rhipper-ide`, an IntelliJ fork; its plugin is `plugins/rhipper`, its plan for this engine `docs/rhipper/rhipper-asm.md`). The roadmap is `rhipper.md`; a task is a card on it; you are the worker of one card.

## Layout

- `crates/ra-asm-format` — the little-endian cursor; every refusal names the offset and the field
- `crates/ra-asm-pe` — the PE loader: headers, sections, imports, exports, relocations, the exception table, TLS, the debug directory; `Image` with RVA/VA/file-offset conversions and bounded reads
- `crates/ra-asm-arch` — the neutral `Instr` record and the `Arch` trait a second architecture implements
- `crates/ra-asm-x86` — the x86/x64 decoder over iced-x86 (`decode.rs`), the register table (`registers.rs`)
- `crates/ra-asm-analysis` — functions (`lib.rs` fixpoint of seeds and descent, `builder.rs` blocks), jump tables, no-return, RTTI classes (`rtti.rs`), calls through a register that holds an import (`imports.rs`)
- `crates/ra-asm-ir` — the IR: types, instructions, modules, the `.ll` printer, dominators
- `crates/ra-asm-lift` — the lift: `x86.rs` (one match arm per mnemonic), `x87.rs`, `state.rs` (the `%State` machine model and the runtime globals), `data.rs` (data sections as globals), `raise/` (promote, abi, stack, frame, conditions, simplify, dce, widths, known, stubs, prototypes = the import prototype table)
- `crates/ra-asm-emit` — `structure.rs` (language-neutral structuring), `c.rs`, `rust.rs`, `functions.rs` (the function table)
- `apps/ras` — the CLI and the engine of the workbench: `info|disasm|functions|cfg|lift|signatures|emit|pipeline|serve`; `serve.rs` is JSON-RPC over stdio; `templates/CMakeLists.txt`
- `runtime/` — `rhipper.h` and `driver.c` (the C runtime and the host driver of a port), `rhipper.rs` (the Rust runtime); embedded in `ras` and written under a project's `gen`
- `testdata/` — `src/*.c|cpp` fixtures, `build.ps1` builds them with clang+lld into `bin/` (64 and 32), `expected/*.txt` their native output; `rhtest/` the old pipeline's MSVC-built test programs (`src/`, `<name>-<variant>.exe|.map`, `<name>.txt`)
- `rhipper.md` is tracked here (the roadmap is part of the record); `var/` and `out/` are not. `out/` is the scratch directory for ports and traces.

## Build and checks

- `cargo test -q` is the check of a card unless its acceptance names another cargo command in backticks. Run it before reporting. The run's environment carries `CARGO_BUILD_JOBS=6` (from `rhipper.toml`): the machine has little RAM and a full-parallel build swaps, so never raise it.
- `rhipper.toml` also allows the worker `clang`, `clang++`, `clang-cl`, `lld-link`, the `llvm-*` tools, `cmake`, `ninja` and `target/debug/ras.exe`, so a port can be built and traced by hand as described below; a command the allowlist refuses is refused for the whole run, so stay with these.
- The tests need LLVM on the PATH: `clang`, `clang++`, `lld`, `llvm-objdump`, `llvm-readobj`, `llvm-symbolizer`; `cargo` for the Rust oracle. Tests that need a tool skip with a message when it is absent; a skip is not a pass.
- `cargo clippy --all-targets` warns with the pedantic set (the allows are in `Cargo.toml`); a change adds no warning. `cargo fmt` (width 140, `use_small_heuristics = "Max"`) on what you touched. Edition 2024, let chains allowed.
- `cargo test` does not always relink `apps/ras`'s binary for the integration tests: run `cargo build -q` first when a `ras` test looks stale.
- The oracles: `crates/ra-asm-lift/tests/oracle.rs` lifts every fixture at the machine, promoted and ABI (standalone) levels, compiles the `.ll` with clang beside `runtime/driver.c`, runs it and compares with `testdata/expected`; `crates/ra-asm-emit/tests/oracle.rs` does the same for the C, C++ and Rust outputs; `apps/ras/tests/pipeline.rs` runs the whole pipeline. A fixture program joins all of them.
- A hand port for debugging: `target/debug/ras.exe emit <exe> --out out/<name> --quiet`, then clang on `out/<name>/*.c` plus `runtime/driver.c` with `-I out/<name> -I runtime -fuse-ld=lld -lkernel32` (`-Wl,/largeaddressaware:no` for a 32-bit program; pass `-Wl,...` through a PowerShell array, the shell mangles it inline). `-DRA_TRACE` prints every lifted function entered on stderr; a crash prints `ra: exception <code> at <host address> (module+<offset>)`, and `llvm-symbolizer --obj=<port.exe> 0x140000000+<offset>` on a `-g -gcodeview` build names the function and its line. There is no debugger on this machine.
- The console and foreign-architecture toolchains live in WSL Ubuntu 24.04 (since 2026-11-12), not on Windows: the Windows LLVM builds only ARM and x86. Call them as `wsl -d Ubuntu-24.04 -- bash -lc "<command>"` (the login shell reads `/etc/profile.d/consoles.sh`, which puts every SDK on the path and sets its variables); the repository is at `/mnt/c/Users/tyler/Documents/code/rhipper-asm` there, and a worktree at the same path under `var/worktrees/<id>`. What is there:
  - `clang`, `lld`, `llvm-objdump` with every target (MIPS, PowerPC, SystemZ, RISC-V and the rest): the fixtures of a foreign architecture are built and listed here and committed beside their source.
  - PlayStation: PSn00bSDK with `mipsel-none-elf-gcc` (`/opt/psn00bsdk`). PlayStation 2: ps2dev with `mips64r5900el-ps2-elf-gcc` and ps2sdk (`PS2DEV`, `PS2SDK`). PSP: pspdev with `psp-gcc` (`PSPDEV`). Nintendo 64: libdragon with `mips64-elf-gcc` (`N64_INST=/opt/libdragon`). Dreamcast and Saturn: KallistiOS with `sh-elf-gcc` (`KOS_BASE=/opt/toolchains/dc/kos`), when its build finished.
  - No emulator is installed: a fixture's expected output comes from its source and the SDK's documented behaviour, or the card says it needs an emulator.
  - `upx` 5.3.0 is on Windows (winget): `C:\Users\tyler\AppData\Local\Microsoft\WinGet\Packages\UPX.UPX_Microsoft.Winget.Source_8wekyb3d8bbwe\upx-5.3.0-win64\upx.exe`.
- `ras emit --quiet` hides the lift's warnings; without it every unsupported instruction prints with its address. The grade of a function is its trap count; a lift that guesses is a bug.

## Conventions

- Match the surrounding code: its comment density, naming and idiom. Doc comments say what and why, not how; a comment inside a function names the reason, not the step. No `TODO`s: an open end becomes a card or a line in the card's report.
- The lift is exact or it stops: an instruction the lift does not cover becomes a trap that names its address, never an approximation. Readability work (widths, names, idioms) keeps every oracle green; a new behaviour comes with a fixture that exercises it and its expected output from the native run.
- No compatibility with the old pipeline's files, names or conventions (Ghidra's corpus, the fidelity scores, `FUN_`/`GEN_` names): the engine writes its own shapes and the workbench reads those.
- The 32-bit port of a program is a 64-bit executable linked without large-address awareness that keeps the program's 32-bit pointers (roadmap decision 8); the machine and promoted levels map the image at its base and cannot run a 32-bit program that uses the C runtime.
- Names: an unnamed function is `F_<address>` in the IR, the C and the table; an address-taken x86 function has a universal entry `U_<address>`; runtime symbols are `ra_*`.
- `../rhipper-ide` is the consumer: when the JSON of `ras serve`, `functions.json` or `rhipper.json` changes, say so in the report so the workbench and its `docs/rhipper/rhipper-asm.md` follow. Never touch `../rhipper-agent`.
- Read the files the card names first, by the regions the outline points at; search with Grep and Glob before exploring; do not read `rhipper.md` itself, the card's text is in your prompt.
- Never write a file with a shell heredoc or `Set-Content`; use the Write and Edit tools. Never run Python with the script on stdin. `Remove-Item` on repo files is blocked; leave a stray file and say so.
- Report what changed and what was verified in a few lines; the check decides whether the card is done, not the report. The person commits.
