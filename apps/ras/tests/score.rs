//! `ras score <dir>`: the board over a scratch `testdata` tree of two fixtures, a link map, a
//! pending list and an oracle, as JSON and as a table; and the committed board,
//! `docs/scoreboard.md`, recomputed over `testdata/bin`, which no change makes worse and which,
//! when it moves, is kept whole under `out/`.
//! `ras score --source`: the lexical half of
//! the grade over a C tree, with the counts of the emit cases, whose text is fixed, and of a
//! hand-written file whose gotos, casts and labels are known, comments and literals among the
//! decoys. And, ignored, the real program's line: the scoreboard row of the program `RA_GAME`
//! names, printed for `docs/scoreboard.md`.

use std::collections::{BTreeMap, BTreeSet};
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

/// A scratch `testdata` tree: `bin` with `hello64.exe`, `control64.exe` and the link map of the
/// second, which is no program; a pending list of two entries under a comment; and, beside the
/// tree, an emit oracle that ignores one test and keeps one debugging aid.
fn board_tree(name: &str) -> ra_asm_testing::Scratch {
    let dir = ra_asm_testing::Scratch::new("ras-score-tests", name);
    let bin = dir.join("testdata/bin");
    std::fs::create_dir_all(&bin).unwrap();
    for file in ["hello64.exe", "control64.exe", "control64.map"] {
        std::fs::copy(root().join("testdata/bin").join(file), bin.join(file)).unwrap();
    }
    let list = dir.join("testdata/vectors/x86");
    std::fs::create_dir_all(&list).unwrap();
    std::fs::write(list.join("PENDING.txt"), "# the forms that differ\nstack Pushad: case 64\nstack Popad: case 80\n").unwrap();
    let oracle = dir.join("crates/ra-asm-emit/tests");
    std::fs::create_dir_all(&oracle).unwrap();
    let text =
        "#[test]\n#[ignore = \"needs a card\"]\nfn waits() {}\n\n#[test]\n#[ignore = \"a debugging aid: run by name\"]\nfn traced() {}\n";
    std::fs::write(oracle.join("oracle.rs"), text).unwrap();
    dir
}

#[test]
fn board_has_a_row_per_program_a_total_and_the_lists() {
    let dir = board_tree("board-json");
    let bin = dir.join("testdata/bin");
    let out = ras(&["score", "--json", bin.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let board: Value = serde_json::from_slice(&out.stdout).unwrap();
    let programs = board["programs"].as_array().unwrap();
    let names: Vec<&str> = programs.iter().filter_map(|p| p["program"].as_str()).collect();
    assert_eq!(names, ["control64.exe", "hello64.exe"], "{board:#}");
    for p in programs {
        assert_eq!(p["status"], "ok", "{p:#}");
        assert!(p["functions"]["machine"].as_u64().unwrap() > 0, "{p:#}");
        assert!(p["functions"]["promoted"].as_u64().unwrap() > 0, "{p:#}");
        assert!(p["functions"]["abi"].as_u64().unwrap() <= p["functions"]["machine"].as_u64().unwrap(), "{p:#}");
        assert!(p["lines"].as_u64().unwrap() > 0, "{p:#}");
        assert!(p["per_thousand"]["gotos"].is_f64(), "{p:#}");
        assert!(p["verified"]["promoted"].as_u64().unwrap() <= p["recorded"].as_u64().unwrap(), "{p:#}");
    }
    let total = &board["total"];
    assert_eq!(total["program"], "total");
    assert_eq!(total["status"], "2/2 ok");
    let sum = |key: &str, level: &str| programs.iter().map(|p| p[key][level].as_u64().unwrap()).sum::<u64>();
    assert_eq!(total["functions"]["machine"].as_u64().unwrap(), sum("functions", "machine"), "{board:#}");
    assert_eq!(total["verified"]["machine"].as_u64().unwrap(), sum("verified", "machine"), "{board:#}");
    if cfg!(windows) {
        // `control64.exe` runs to its end through the interpreter, and its vectors replay.
        let control = &programs[0];
        assert!(control["verified"]["machine"].as_u64().unwrap() >= 1, "{control:#}");
        assert!(control["verified"]["promoted"].as_u64().unwrap() >= 1, "{control:#}");
    }
    assert_eq!(board["pending"], serde_json::json!([{"list": "testdata/vectors/x86/PENDING.txt", "entries": 2}]), "{board:#}");
    assert_eq!(board["ignored"], serde_json::json!([{"oracle": "crates/ra-asm-emit/tests/oracle.rs", "tests": 1}]), "{board:#}");
}

#[test]
fn board_table_has_a_row_per_program_and_a_total_row() {
    let dir = board_tree("board-table");
    let bin = dir.join("testdata/bin");
    let out = ras(&["score", bin.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let table = String::from_utf8_lossy(&out.stdout);
    assert!(table.starts_with("| program | status | machine |"), "{table}");
    for row in ["| control64.exe | ok |", "| hello64.exe | ok |", "| total | 2/2 ok |"] {
        assert_eq!(table.lines().filter(|l| l.starts_with(row)).count(), 1, "{row}:\n{table}");
    }
    assert!(!table.contains("control64.map"), "{table}");
    assert!(table.contains("| testdata/vectors/x86/PENDING.txt | 2 |"), "{table}");
    assert!(table.contains("| crates/ra-asm-emit/tests/oracle.rs | 1 |"), "{table}");
}

/// The way a column of the board gets better; a column of neither way, such as the lines of the
/// C, only moves.
#[derive(Clone, Copy)]
enum Better {
    Higher,
    Lower,
    Neither,
}

fn better(column: &str) -> Better {
    match column {
        "status" | "machine" | "promoted" | "abi" | "verified machine" | "verified promoted" => Better::Higher,
        "traps" | "trapping" | "open" | "gotos ‰" | "casts ‰" | "entries" | "ignored tests" => Better::Lower,
        _ => Better::Neither,
    }
}

/// The cells of a board's tables by the table's first heading, the row's first cell and the
/// column: what stands outside a table is no figure.
type Cells = BTreeMap<(String, String, String), String>;

fn cells(board: &str) -> Cells {
    let mut out = Cells::new();
    let mut header: Option<Vec<String>> = None;
    for line in board.lines().map(str::trim) {
        if !line.starts_with('|') {
            header = None;
            continue;
        }
        let row: Vec<String> = line.trim_matches('|').split('|').map(|cell| cell.trim().to_owned()).collect();
        if row.iter().all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-')) {
            continue;
        }
        match &header {
            None => header = Some(row),
            Some(columns) => {
                for (column, cell) in columns.iter().zip(&row).skip(1) {
                    out.insert((columns[0].clone(), row[0].clone(), column.clone()), cell.clone());
                }
            }
        }
    }
    out
}

/// A cell as a figure: a status counts the programs that are `ok` (the total's `47/48 ok` its
/// 47), and a `-` is no figure.
fn figure(column: &str, cell: &str) -> Option<f64> {
    if column == "status" {
        return match cell.split_once('/') {
            Some((ok, _)) => ok.parse().ok(),
            None => Some(f64::from(u8::from(cell == "ok"))),
        };
    }
    cell.parse().ok()
}

/// Whether the cell went from `was` to `now` the worse way: a figure that became none is worse,
/// one that appeared better.
fn is_worse(column: &str, was: &str, now: &str) -> bool {
    match (figure(column, was), figure(column, now)) {
        (Some(was), Some(now)) => match better(column) {
            Better::Higher => now < was,
            Better::Lower => now > was,
            Better::Neither => false,
        },
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// Whether a cell is held to the committed one. A table's total sums its rows, which are held one
/// by one, so a row new to the board moves the total without making it worse; and a program that
/// ran out of time stopped where the machine's load left it: of its row only the status is held.
fn held(was: &Cells, now: &Cells, (table, row, column): &(String, String, String)) -> bool {
    if row == "total" {
        return false;
    }
    if table != "program" {
        return true;
    }
    let timed_out = |cells: &Cells| cells.get(&(table.clone(), row.clone(), "status".to_owned())).is_some_and(|s| s == "timeout");
    column == "status" || !(timed_out(was) || timed_out(now))
}

/// The lines of `board` that head a table or hold one of `rows`, by table and row: the failure
/// shows what moved and not the whole board.
fn excerpt(board: &str, rows: &BTreeSet<(String, String)>) -> String {
    let mut out = String::new();
    let mut table: Option<String> = None;
    for line in board.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            table = None;
            continue;
        }
        let first = trimmed.trim_matches('|').split('|').next().unwrap_or_default().trim().to_owned();
        let shown = match &table {
            None => {
                table = Some(first);
                true
            }
            Some(_) if first.chars().all(|c| c == '-') => true,
            Some(table) => rows.contains(&(table.clone(), first)),
        };
        if shown {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Where a board that moved is kept, relative to the worktree: the failure's own output shows only
/// the rows that moved, and names this file, which holds the whole board.
const FRESH: &str = "out/scoreboard.fresh.md";

/// Writes `board` under `worktree` as `ras score --write` writes it, below the head of the
/// committed board, and returns the file.
fn save_fresh(worktree: &Path, committed: &str, board: &str) -> PathBuf {
    let head = committed.find("\n|").map_or("", |at| &committed[..=at]);
    let fresh = worktree.join(FRESH);
    std::fs::create_dir_all(fresh.parent().unwrap()).unwrap();
    std::fs::write(&fresh, format!("{head}{board}")).unwrap();
    fresh
}

#[test]
fn fresh_board_lands_under_out() {
    // A scratch worktree stands for the repository, so the run leaves the real `out/` as found.
    let dir = board_tree("fresh-board");
    let out = ras(&["score", dir.join("testdata/bin").to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let board = String::from_utf8_lossy(&out.stdout).into_owned();
    let committed = "# The scoreboard\n\nthe head of the board\n\n| program | status |\n|---|---|\n| gone.exe | ok |\n";
    let fresh = save_fresh(dir.path(), committed, &board);
    assert_eq!(fresh, dir.path().join("out/scoreboard.fresh.md"));
    let text = std::fs::read_to_string(&fresh).unwrap();
    assert!(text.starts_with("# The scoreboard\n\nthe head of the board\n\n|"), "{text}");
    assert!(!text.contains("gone.exe"), "{text}");
    let rows: BTreeSet<&str> = text.lines().collect();
    let board_rows: Vec<&str> = board.lines().filter(|l| l.trim_start().starts_with('|')).collect();
    assert!(board_rows.iter().any(|l| l.starts_with("| total |")), "{board}");
    for row in board_rows {
        assert!(rows.contains(row), "{row} is not in {}:\n{text}", fresh.display());
    }
}

/// The committed board: `docs/scoreboard.md` recomputed over `testdata/bin`. A figure worse than
/// the committed one fails with its program and column; any other move fails with the call that
/// writes the board again, so the diff of the change that moved it shows the move.
#[test]
fn committed_board_is_never_worse() {
    // the board is measured on Windows: a function is verified by a recorded run, which takes
    // the Windows DLLs, so off Windows every program reads 1 verified (release CI on Linux)
    if !cfg!(windows) {
        eprintln!("SKIPPED: committed_board_is_never_worse: docs/scoreboard.md is measured on Windows");
        return;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap();
    let committed = std::fs::read_to_string(repo.join("docs/scoreboard.md")).unwrap_or_default();
    let out = ras(&["score", repo.join("testdata/bin").to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let board = String::from_utf8_lossy(&out.stdout).into_owned();

    let (was, now) = (cells(&committed), cells(&board));
    let mut worse = Vec::new();
    let mut moved = Vec::new();
    let mut rows = BTreeSet::new();
    for (key, before) in was.iter().filter(|(key, _)| held(&was, &now, key)) {
        let (table, row, column) = key;
        match now.get(key) {
            // A program gone from the board took its figures with it; a list gone is emptied.
            None if table == "program" => worse.push(format!("{row}: {column} {before} -> gone")),
            None => moved.push(format!("{row}: {column} {before} -> gone")),
            Some(after) if after == before => continue,
            Some(after) if is_worse(column, before, after) => worse.push(format!("{row}: {column} {before} -> {after}")),
            Some(after) => moved.push(format!("{row}: {column} {before} -> {after}")),
        }
        rows.insert((table.clone(), row.clone()));
    }
    let known: BTreeSet<(&String, &String)> = was.keys().map(|(table, row, _)| (table, row)).collect();
    for ((table, row, column), after) in now.iter().filter(|(key, _)| !was.contains_key(*key) && held(&was, &now, key)) {
        // A row new to the board is one move, its figures stand in the rows below.
        if !known.contains(&(table, row)) {
            if rows.insert((table.clone(), row.clone())) {
                moved.push(format!("{row}: new"));
            }
            continue;
        }
        moved.push(format!("{row}: {column} new, {after}"));
        rows.insert((table.clone(), row.clone()));
    }
    if worse.is_empty() && moved.is_empty() {
        return;
    }
    let shown = excerpt(&board, &rows);
    save_fresh(repo, &committed, &board);
    let fresh = FRESH;
    assert!(worse.is_empty(), "worse than docs/scoreboard.md:\n  {}\n\nthe rows now:\n{shown}\nthe board now: {fresh}", worse.join("\n  "));
    assert!(
        moved.is_empty(),
        "the board moved from docs/scoreboard.md: run `ras score --write testdata/bin` and commit it:\n  {}\n\nthe rows now:\n{shown}\nthe board now: {fresh}",
        moved.join("\n  ")
    );
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
    // The checked build's cases, apart from the rest of the snapshots, which grow.
    let scratch = ra_asm_testing::Scratch::new("ras-score-tests", "checked");
    for case in ["checked_call.c", "checked_return.c", "checked_switch.c", "checked_table.c"] {
        std::fs::copy(root().join("testdata/cases/emit").join(case), scratch.join(case)).unwrap();
    }
    let dir = scratch.path();
    let before = listing(dir);
    let score = score(dir);
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
