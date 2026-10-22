//! A small unified diff of two texts, for `kosmetic fmt --diff`.

use std::fmt::Write as _;

/// Lines of context around every change.
const CONTEXT: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Keep,
    Remove,
    Add,
}

/// The longest common subsequence of two lists of lines, as a list of
/// operations. Lines that both texts share at the start and the end are
/// skipped before the quadratic part, which keeps typical edits cheap.
fn operations<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<(Op, &'a str)> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (old_mid, new_mid) = (
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );

    // lcs[i][j]: length of the common subsequence of old_mid[i..] and new_mid[j..].
    let width = new_mid.len() + 1;
    let mut lcs = vec![0u32; (old_mid.len() + 1) * width];
    for i in (0..old_mid.len()).rev() {
        for j in (0..new_mid.len()).rev() {
            lcs[i * width + j] = if old_mid[i] == new_mid[j] {
                lcs[(i + 1) * width + j + 1] + 1
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }

    let mut ops: Vec<_> = old[..prefix].iter().map(|line| (Op::Keep, *line)).collect();
    let (mut i, mut j) = (0, 0);
    while i < old_mid.len() || j < new_mid.len() {
        if i < old_mid.len() && j < new_mid.len() && old_mid[i] == new_mid[j] {
            ops.push((Op::Keep, old_mid[i]));
            i += 1;
            j += 1;
        } else if i < old_mid.len()
            && (j == new_mid.len() || lcs[(i + 1) * width + j] >= lcs[i * width + j + 1])
        {
            ops.push((Op::Remove, old_mid[i]));
            i += 1;
        } else {
            ops.push((Op::Add, new_mid[j]));
            j += 1;
        }
    }
    ops.extend(
        old[old.len() - suffix..]
            .iter()
            .map(|line| (Op::Keep, *line)),
    );
    ops
}

/// A unified diff from `old` to `new`, empty if they are equal.
pub(crate) fn unified(name: &str, old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let old_lines: Vec<_> = old.split_inclusive('\n').collect();
    let new_lines: Vec<_> = new.split_inclusive('\n').collect();
    let ops = operations(&old_lines, &new_lines);

    // Line numbers (0-based) in both texts at the start of every operation.
    let mut positions = Vec::with_capacity(ops.len());
    let (mut old_at, mut new_at) = (0, 0);
    for (op, _) in &ops {
        positions.push((old_at, new_at));
        old_at += usize::from(*op != Op::Add);
        new_at += usize::from(*op != Op::Remove);
    }

    let changed: Vec<_> = (0..ops.len()).filter(|&i| ops[i].0 != Op::Keep).collect();
    let mut out = format!("--- {name}\n+++ {name} (formatted)\n");
    let mut k = 0;
    while k < changed.len() {
        let start = changed[k].saturating_sub(CONTEXT);
        let mut end = changed[k];
        while k + 1 < changed.len() && changed[k + 1] <= end + 2 * CONTEXT + 1 {
            k += 1;
            end = changed[k];
        }
        k += 1;
        let end = (end + CONTEXT + 1).min(ops.len());
        let hunk = &ops[start..end];
        let old_count = hunk.iter().filter(|(op, _)| *op != Op::Add).count();
        let new_count = hunk.iter().filter(|(op, _)| *op != Op::Remove).count();
        let (old_start, new_start) = positions[start];
        let _ = writeln!(
            out,
            "@@ -{},{old_count} +{},{new_count} @@",
            old_start + usize::from(old_count > 0),
            new_start + usize::from(new_count > 0),
        );
        for (op, line) in hunk {
            let sign = match op {
                Op::Keep => ' ',
                Op::Remove => '-',
                Op::Add => '+',
            };
            out.push(sign);
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::unified;

    #[test]
    fn equal_texts_have_no_diff() {
        assert_eq!(unified("a.toml", "a = 1\n", "a = 1\n"), "");
    }

    #[test]
    fn shows_removed_and_added_lines_with_context() {
        let old = "a=1\nb = 2\nc = 3\n";
        let new = "a = 1\nb = 2\nc = 3\n";
        assert_eq!(
            unified("a.toml", old, new),
            "--- a.toml\n+++ a.toml (formatted)\n@@ -1,3 +1,3 @@\n-a=1\n+a = 1\n b = 2\n c = 3\n"
        );
    }

    #[test]
    fn distant_changes_make_separate_hunks() {
        let old = (1..=20)
            .map(|n| format!("k{n} = {n}\n"))
            .collect::<Vec<_>>()
            .concat();
        let new = old
            .replace("k2 = 2", "k2 = 22")
            .replace("k19 = 19", "k19 = 99");
        let diff = unified("f", &old, &new);
        assert_eq!(diff.matches("@@ -").count(), 2, "{diff}");
        assert!(
            diff.contains(
                "@@ -1,5 +1,5 @@\n k1 = 1\n-k2 = 2\n+k2 = 22\n k3 = 3\n k4 = 4\n k5 = 5\n"
            )
        );
        assert!(diff.contains("-k19 = 19\n+k19 = 99\n k20 = 20\n"));
    }

    #[test]
    fn nearby_changes_share_a_hunk() {
        let old = "a\nb\nc\nd\ne\nf\ng\n";
        let new = "A\nb\nc\nd\ne\nf\nG\n";
        assert_eq!(unified("f", old, new).matches("@@ -").count(), 1);
    }

    #[test]
    fn inserted_and_deleted_blank_lines() {
        assert_eq!(
            unified("f", "a = 1\n\n\nb = 2\n", "a = 1\n\nb = 2\n"),
            "--- f\n+++ f (formatted)\n@@ -1,4 +1,3 @@\n a = 1\n \n-\n b = 2\n"
        );
    }

    #[test]
    fn a_missing_final_newline_is_marked() {
        assert_eq!(
            unified("f", "a = 1", "a = 1\n"),
            "--- f\n+++ f (formatted)\n@@ -1,1 +1,1 @@\n-a = 1\n\\ No newline at end of file\n+a = 1\n"
        );
    }

    #[test]
    fn works_on_empty_sides() {
        assert_eq!(
            unified("f", "", "a = 1\n"),
            "--- f\n+++ f (formatted)\n@@ -0,0 +1,1 @@\n+a = 1\n"
        );
    }
}
