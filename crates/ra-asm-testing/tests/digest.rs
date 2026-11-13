//! The one-line comparison of the oracles: equal across line endings, the right line named, the
//! line short, both sides and the digest written.

use ra_asm_testing::Scratch;
use ra_asm_testing::digest::{LIMIT, compare_in, digest_line, first_difference};

/// `count` numbered lines, each ending in a newline, with `changed` (from 1) replaced.
fn lines(count: usize, changed: Option<(usize, &str)>) -> String {
    (1..=count)
        .map(|n| match changed {
            Some((line, text)) if line == n => format!("{text}\n"),
            _ => format!("line {n} of the output\n"),
        })
        .collect()
}

#[test]
fn a_crlf_text_equals_its_lf_twin() {
    let lf = lines(50, None);
    let crlf = lf.replace('\n', "\r\n");
    assert_eq!(first_difference(&lf, &crlf), None);
    let scratch = Scratch::new("ra-asm-testing-digest", "crlf");
    assert_eq!(compare_in(&scratch, "crlf", &crlf, &lf), Ok(()));
    assert!(!scratch.join("digest.txt").exists());
}

#[test]
fn a_difference_at_line_400_is_line_400() {
    let expected = lines(500, None);
    let actual = lines(500, Some((400, "something else")));
    let difference = first_difference(&expected, &actual).expect("the texts differ");
    assert_eq!(difference.line, 400);
    assert_eq!(difference.actual.as_deref(), Some("something else"));
    assert_eq!(difference.context, ["line 397 of the output", "line 398 of the output", "line 399 of the output"]);
    let scratch = Scratch::new("ra-asm-testing-digest", "line400");
    let message = compare_in(&scratch, "line400", &expected, &actual).unwrap_err();
    assert!(message.contains("line 400:"), "{message}");
}

#[test]
fn the_message_is_one_short_line() {
    let long = "x".repeat(1000);
    let longer = format!("{long}y");
    let expected = lines(20, Some((10, long.as_str())));
    let actual = lines(20, Some((10, longer.as_str())));
    let scratch = Scratch::new("ra-asm-testing-digest", "short");
    let message = compare_in(&scratch, "a::very_long::test::name", &expected, &actual).unwrap_err();
    assert!(message.chars().count() < LIMIT, "{} characters: {message}", message.chars().count());
    assert!(!message.contains('\n') && !message.contains('\r'), "{message}");
    assert!(message.contains("line 10:"), "{message}");
}

#[test]
fn a_missing_last_line_is_a_difference() {
    let expected = lines(3, None);
    let actual = expected.trim_end().to_owned();
    let difference = first_difference(&expected, &actual).expect("the texts differ");
    assert_eq!(difference.line, 4);
    assert_eq!(difference.expected.as_deref(), Some(""));
    assert_eq!(difference.actual, None);
    assert!(digest_line("t", &difference).contains("<end>"));
}

#[test]
fn both_sides_and_the_digest_line_are_written() {
    let scratch = Scratch::new("ra-asm-testing-digest", "written");
    let expected = lines(10, None);
    let actual = lines(10, Some((7, "seven")));
    let first = compare_in(&scratch, "oracle::hello", &expected, &actual).unwrap_err();
    let second = compare_in(&scratch, "oracle::world", &expected, &actual).unwrap_err();
    let dir = scratch.join("oracle__hello");
    assert_eq!(std::fs::read_to_string(dir.join("expected.txt")).unwrap(), expected);
    assert_eq!(std::fs::read_to_string(dir.join("actual.txt")).unwrap(), actual);
    let digest = std::fs::read_to_string(scratch.join("digest.txt")).unwrap();
    assert_eq!(digest, format!("{first}\n{second}\n"));
    // A later equal comparison takes away what the failure left.
    assert_eq!(compare_in(&scratch, "oracle::hello", &expected, &expected), Ok(()));
    assert!(!dir.exists());
}
