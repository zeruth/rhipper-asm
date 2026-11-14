//! `ras score --source`: the lexical half of the grade over a C tree, with the counts of the
//! emit cases, whose text is fixed, and of a hand-written file whose gotos, casts and labels are
//! known, comments and literals among the decoys. And, ignored, the real program's line: the
//! scoreboard row of the program `RA_GAME` names, printed for `docs/scoreboard.md`.

use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ras(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ras")).args(args).output().unwrap()
}

/// `ras` with path arguments, which must succeed: the failure's stderr names the step.
fn ras_ok(step: &str, args: &[&OsStr]) -> Output {
    let out = Command::new(env!("CARGO_BIN_EXE_ras")).args(args).output().unwrap();
    assert!(out.status.success(), "{step} failed ({}):\n{}", out.status, String::from_utf8_lossy(&out.stderr));
    out
}

/// The columns of the scoreboard's row of a program: the header line pasted once over the rows.
const COLUMNS: &str = "| program | functions | traps | trapping functions | unresolved jumps | imports | structures | interfaces | lines | gotos ‰ | casts ‰ | labels ‰ |";

/// The scoreboard row of a program from its smoke report (the analysis, the lift at the ABI level
/// and the boundary) and the score of its C: the coverage, the length of each pending list of
/// the boundary, then the lexical half per thousand lines.
fn scoreboard_line(name: &str, report: &Value, score: &Value) -> String {
    let c = &report["coverage"];
    let pending = |key: &str| report["boundary"][key].as_array().map_or(0, Vec::len);
    let per_thousand = |key: &str| score["per_thousand"][key].as_f64().unwrap_or(0.0);
    format!(
        "| {name} | {} | {} | {} | {} | {} | {} | {} | {} | {:.1} | {:.1} | {:.1} |",
        c["functions"],
        c["traps"],
        c["trapping_functions"],
        c["unresolved_jumps"],
        pending("imports"),
        pending("structures"),
        pending("interfaces"),
        score["lines"],
        per_thousand("gotos"),
        per_thousand("casts"),
        per_thousand("labels"),
    )
}

/// The real program's line. `RA_GAME` names the person's program, which is never committed: the
/// test runs `ras smoke --census` over it (the analysis, the lift at the ABI level, the
/// translation and the boundary), emits its C and scores the tree, and prints the scoreboard row
/// to paste into `docs/scoreboard.md` under "the program", then the census. From here the
/// program's measure is that line.
#[test]
#[ignore = "needs RA_GAME, the person's program; run with `cargo test -q -p ras --test score -- --ignored the_program`"]
fn the_program() {
    let game = std::env::var_os("RA_GAME").unwrap_or_else(|| panic!("RA_GAME is not set: it names the program's path"));
    let game = PathBuf::from(game);
    assert!(game.is_file(), "RA_GAME names no file: {}", game.display());
    let dir = ra_asm_testing::Scratch::new("ras-score-tests", "the-program");
    let report_path = dir.join("smoke.json");
    // A game-sized program takes minutes through the lift: no time limit of the smoke's own.
    let smoke = ras_ok(
        "ras smoke",
        &[
            OsStr::new("smoke"),
            OsStr::new("--quiet"),
            OsStr::new("--census"),
            OsStr::new("--timeout"),
            OsStr::new("86400"),
            OsStr::new("--out"),
            report_path.as_os_str(),
            game.as_os_str(),
        ],
    );
    let census = String::from_utf8_lossy(&smoke.stdout).into_owned();
    let report: Value = serde_json::from_str(&std::fs::read_to_string(&report_path).unwrap()).unwrap();
    assert_eq!(report["status"]["ok"], 1, "the program did not go through the smoke: {:#}\n{:#}", report["status"], report["errors"]);

    let c = dir.join("c");
    ras_ok("ras emit", &[OsStr::new("emit"), game.as_os_str(), OsStr::new("--out"), c.as_os_str(), OsStr::new("--quiet")]);
    let score = ras_ok("ras score", &[OsStr::new("score"), OsStr::new("--json"), OsStr::new("--source"), c.as_os_str()]);
    let score: Value = serde_json::from_slice(&score.stdout).unwrap();

    let line = scoreboard_line("the program", &report, &score);
    // Written past the harness's capture, so a passing run shows the line without `--nocapture`.
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{COLUMNS}\n{line}\n\n{census}");
}

/// `ras score --source <source> --json`, parsed.
fn score(source: &Path) -> Value {
    let out = ras(&["score", "--json", "--source", source.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

/// The names in `dir`, sorted: what a run must leave as it found it.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn source_one_emit_case_has_its_known_counts() {
    // `checked_switch.c`: 14 lines, a `switch` whose `case` and `default` lines are no labels,
    // and no cast.
    let score = score(&root().join("testdata/cases/emit/checked_switch.c"));
    assert_eq!(score["files"], 1, "{score}");
    assert_eq!(score["lines"], 14, "{score}");
    assert_eq!(score["gotos"], 0, "{score}");
    assert_eq!(score["casts"], 0, "{score}");
    assert_eq!(score["labels"], 0, "{score}");
}

#[test]
fn source_emit_directory_sums_its_files_and_is_left_as_found() {
    let dir = root().join("testdata/cases/emit");
    let before = listing(&dir);
    let score = score(&dir);
    assert_eq!(score["files"], 4, "{score}");
    assert_eq!(score["lines"], 36, "{score}");
    // `(rsp - n)` in `checked_return.c` is an expression, `ra_assume(...)` a call: no cast.
    assert_eq!(score["casts"], 0, "{score}");
    assert_eq!(score["gotos"], 0, "{score}");
    assert_eq!(score["labels"], 0, "{score}");
    assert_eq!(score["per_thousand"]["gotos"], 0.0, "{score}");
    assert_eq!(listing(&dir), before);

    let table = ras(&["score", "--source", dir.to_str().unwrap()]);
    assert!(table.status.success(), "{}", String::from_utf8_lossy(&table.stderr));
    let table = String::from_utf8_lossy(&table.stdout);
    assert!(table.contains("lines    36"), "{table}");
    assert!(table.contains("gotos    0        0.0 per thousand lines"), "{table}");
}

#[test]
fn source_counts_gotos_casts_and_labels_past_the_decoys() {
    let dir = ra_asm_testing::Scratch::new("ras-score-tests", "counts");
    std::fs::copy(root().join("testdata/cases/emit/checked_switch.c"), dir.join("checked_switch.c")).unwrap();
    // 23 lines; 2 gotos; 2 labels (`again`, `done`); 4 casts: `(int32_t)n`, `(int)*p`,
    // `(uintptr_t)` (the `(void *)` after it is the missed second of a chain) and
    // `(int32_t)'\''`. `sizeof(int32_t)` and `(total)` are no casts; the comments and the
    // string hold every word but count for nothing.
    let hand = "\
// goto in a comment, (int) in a comment, label: in a comment
#include <stdint.h>
/* a block comment with goto and
   label: and (char *)x */
static const char *text = \"goto label: (int)x\";
int f(int32_t n, char *p) {
    int32_t total = (int32_t)n + (int)*p;
    uintptr_t at = (uintptr_t)(void *)p;
    if (n < 0) goto done;
again:
    n = n - 1;
    if (n > 0) goto again;
    total = total * (n + 1);
    total = sizeof(int32_t) + (total);
    switch (total) {
    case 0:
        return 0;
    default:
        break;
    }
done:
    return total ? total : (int32_t)'\\'';
}
";
    std::fs::write(dir.join("hand.c"), hand).unwrap();
    // A file of another language is not read.
    std::fs::write(dir.join("notes.txt"), "goto goto goto\nlabel:\n(int)x\n").unwrap();

    let score = score(dir.path());
    assert_eq!(score["files"], 2, "{score}");
    assert_eq!(score["lines"], 37, "{score}");
    assert_eq!(score["gotos"], 2, "{score}");
    assert_eq!(score["labels"], 2, "{score}");
    assert_eq!(score["casts"], 4, "{score}");
    let per_thousand = score["per_thousand"]["gotos"].as_f64().unwrap();
    assert!((per_thousand - 2000.0 / 37.0).abs() < 1e-9, "{score}");
}
