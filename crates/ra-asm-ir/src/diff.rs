//! The line diff of two texts of IR: what two builds of a function became once raised, shown as a
//! unified diff. The lines are compared as given; the caller takes out what differs between builds
//! without a change, such as the addresses in the names.

use std::fmt::Write;

/// A line of a diff: in both texts, only in the old, or only in the new.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line<'a> {
    Same(&'a str),
    Removed(&'a str),
    Added(&'a str),
}

/// The bound of the table of [`lines`], in cells of four bytes.
const MAX_CELLS: usize = 1 << 22;

/// The longest common subsequence of `old` and `new`, as the edit that turns one into the other.
/// The common head and tail are taken first, so a small change in a large function costs the
/// table of the lines between them only.
pub fn lines<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Line<'a>> {
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old[head..].iter().rev().zip(new[head..].iter().rev()).take_while(|(a, b)| a == b).count();
    let (a, b) = (&old[head..old.len() - tail], &new[head..new.len() - tail]);
    let mut out: Vec<Line<'a>> = old[..head].iter().map(|&l| Line::Same(l)).collect();
    // lcs[i][j]: the common subsequence of a[i..] and b[j..]. A middle too large for the table
    // reads as replaced whole.
    let width = b.len() + 1;
    let cells = (a.len() + 1) * width;
    let mut lcs = if cells <= MAX_CELLS { vec![0u32; cells] } else { Vec::new() };
    if lcs.is_empty() {
        out.extend(a.iter().map(|&l| Line::Removed(l)));
        out.extend(b.iter().map(|&l| Line::Added(l)));
        out.extend(old[old.len() - tail..].iter().map(|&l| Line::Same(l)));
        return out;
    }
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i * width + j] = if a[i] == b[j] { lcs[(i + 1) * width + j + 1] + 1 } else { lcs[(i + 1) * width + j].max(lcs[i * width + j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            out.push(Line::Same(a[i]));
            i += 1;
            j += 1;
        } else if lcs[(i + 1) * width + j] >= lcs[i * width + j + 1] {
            out.push(Line::Removed(a[i]));
            i += 1;
        } else {
            out.push(Line::Added(b[j]));
            j += 1;
        }
    }
    out.extend(a[i..].iter().map(|&l| Line::Removed(l)));
    out.extend(b[j..].iter().map(|&l| Line::Added(l)));
    out.extend(old[old.len() - tail..].iter().map(|&l| Line::Same(l)));
    out
}

/// The unified diff of `old` and `new` with `context` lines around each change, its hunks headed
/// `@@ -start,count +start,count @@`; empty when the two are the same.
pub fn unified(old: &str, new: &str, context: usize) -> String {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let diff = lines(&a, &b);
    let changes: Vec<usize> = diff.iter().enumerate().filter(|(_, l)| !matches!(l, Line::Same(_))).map(|(n, _)| n).collect();
    let mut out = String::new();
    let mut k = 0;
    while k < changes.len() {
        // A hunk runs while the next change is within twice the context of the last.
        let mut last = k;
        while last + 1 < changes.len() && changes[last + 1] - changes[last] <= 2 * context + 1 {
            last += 1;
        }
        let from = changes[k].saturating_sub(context);
        let to = (changes[last] + context + 1).min(diff.len());
        let (mut old_line, mut new_line) = (1, 1);
        for l in &diff[..from] {
            match l {
                Line::Same(_) => (old_line, new_line) = (old_line + 1, new_line + 1),
                Line::Removed(_) => old_line += 1,
                Line::Added(_) => new_line += 1,
            }
        }
        let hunk = &diff[from..to];
        let old_count = hunk.iter().filter(|l| !matches!(l, Line::Added(_))).count();
        let new_count = hunk.iter().filter(|l| !matches!(l, Line::Removed(_))).count();
        let _ = writeln!(out, "@@ -{old_line},{old_count} +{new_line},{new_count} @@");
        for l in hunk {
            let _ = match l {
                Line::Same(t) => writeln!(out, " {t}"),
                Line::Removed(t) => writeln!(out, "-{t}"),
                Line::Added(t) => writeln!(out, "+{t}"),
            };
        }
        k = last + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_line_is_one_removed_and_one_added_in_its_context() {
        let old = "define i32 @f() {\n  %a = add i32 1, 2\n  ret i32 %a\n}\n";
        let new = "define i32 @f() {\n  %a = add i32 1, 3\n  ret i32 %a\n}\n";
        assert_eq!(unified(old, new, 1), "@@ -1,3 +1,3 @@\n define i32 @f() {\n-  %a = add i32 1, 2\n+  %a = add i32 1, 3\n   ret i32 %a\n");
        assert_eq!(unified(old, old, 3), "");
    }

    #[test]
    fn the_lines_keep_the_longest_common_run() {
        let diff = lines(&["a", "b", "c", "d"], &["a", "c", "d", "e"]);
        assert_eq!(diff, [Line::Same("a"), Line::Removed("b"), Line::Same("c"), Line::Same("d"), Line::Added("e")]);
    }
}
