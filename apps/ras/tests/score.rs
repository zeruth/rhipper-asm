//! `ras score --source`: the lexical half of the grade over a C tree, with the counts of the
//! emit cases, whose text is fixed, and of a hand-written file whose gotos, casts and labels are
//! known, comments and literals among the decoys.

use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ras(args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_ras")).args(args).output().unwrap()
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
