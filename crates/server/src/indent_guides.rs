//! Per-line indent-guide depth (CPE-1051, epic CPE-724): given a source file's text, produce a flat
//! `Vec<u16>` — one entry per line — giving the indent-guide **depth** to draw for that line, so a future
//! preview can render continuous vertical indent guides down a code pane.
//!
//! Deliberately **pure + dependency-free**: column math over leading whitespace (tabs expand to
//! `tab_width`), no language awareness needed since indentation is language-agnostic. A blank or
//! whitespace-only line bridges to the min of its nearest non-blank neighbours' depths, so a guide
//! doesn't visually break across a blank line inside a block. Std only, O(n).

/// Indent-guide depth for each line of `source` (index `i` == line `i + 1`). `tab_width` is the number of
/// columns a tab expands to (treated as `1` if given as `0`, to avoid division by zero). Empty input
/// yields an empty vec.
pub fn indent_levels(source: &str, tab_width: usize) -> Vec<u16> {
    let tab_width = if tab_width == 0 { 1 } else { tab_width };
    let lines: Vec<&str> = source.lines().collect();
    let n = lines.len();
    if n == 0 {
        return Vec::new();
    }

    // Pass 1: per-line raw depth for non-blank lines (`None` for blank/whitespace-only lines).
    let raw: Vec<Option<u16>> = lines.iter().map(|line| line_depth(line, tab_width)).collect();

    // Pass 2a: nearest non-blank depth at-or-before each index (running forward).
    let mut prev: Vec<Option<u16>> = vec![None; n];
    let mut last = None;
    for i in 0..n {
        if let Some(d) = raw[i] {
            last = Some(d);
        }
        prev[i] = last;
    }

    // Pass 2b: nearest non-blank depth at-or-after each index (running backward).
    let mut next: Vec<Option<u16>> = vec![None; n];
    let mut upcoming = None;
    for i in (0..n).rev() {
        if let Some(d) = raw[i] {
            upcoming = Some(d);
        }
        next[i] = upcoming;
    }

    // Pass 3: fill blanks with the min of the two neighbours (or whichever single one exists, or 0).
    (0..n)
        .map(|i| match raw[i] {
            Some(d) => d,
            None => match (prev[i], next[i]) {
                (Some(a), Some(b)) => a.min(b),
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => 0,
            },
        })
        .collect()
}

/// The indent depth of a single non-blank line, or `None` if the line is blank/whitespace-only.
fn line_depth(line: &str, tab_width: usize) -> Option<u16> {
    if line.trim().is_empty() {
        return None;
    }
    let mut col: usize = 0;
    for c in line.chars() {
        if c == '\t' {
            col = (col / tab_width + 1) * tab_width;
        } else if c.is_whitespace() {
            col += 1;
        } else {
            break;
        }
    }
    Some((col / tab_width) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_empty_vec() {
        assert_eq!(indent_levels("", 4), Vec::<u16>::new());
    }

    #[test]
    fn depth_increases_with_nesting() {
        let src = "fn a() {\n    fn b() {\n        fn c() {}\n    }\n}\n";
        assert_eq!(indent_levels(src, 4), vec![0, 1, 2, 1, 0]);
    }

    #[test]
    fn tabs_and_equivalent_spaces_yield_same_depths() {
        let tabbed = "a\n\tb\n\t\tc\n";
        let spaced = "a\n    b\n        c\n";
        assert_eq!(indent_levels(tabbed, 4), indent_levels(spaced, 4));
        assert_eq!(indent_levels(tabbed, 4), vec![0, 1, 2]);
    }

    #[test]
    fn mixed_tabs_and_spaces_use_column_math() {
        // A single space then a tab: col 1 -> next multiple of 4 -> col 4 -> depth 1.
        let src = " \tindented\n";
        assert_eq!(indent_levels(src, 4), vec![1]);
    }

    #[test]
    fn blank_line_between_two_depth_two_lines_bridges_to_two() {
        let src = "        a\n\n        b\n"; // 8 spaces = depth 2 at tab_width 4
        assert_eq!(indent_levels(src, 4), vec![2, 2, 2]);
    }

    #[test]
    fn whitespace_only_line_counts_as_blank_for_bridging() {
        let src = "        a\n    \n        b\n"; // middle line is spaces-only, not truly empty
        assert_eq!(indent_levels(src, 4), vec![2, 2, 2]);
    }

    #[test]
    fn leading_blank_lines_take_the_single_following_neighbour() {
        let src = "\n\n    a\n";
        assert_eq!(indent_levels(src, 4), vec![1, 1, 1]);
    }

    #[test]
    fn trailing_blank_lines_take_the_single_preceding_neighbour() {
        let src = "    a\n\n\n";
        assert_eq!(indent_levels(src, 4), vec![1, 1, 1]);
    }

    #[test]
    fn all_blank_input_is_all_zero() {
        let src = "\n\n\n";
        assert_eq!(indent_levels(src, 4), vec![0, 0, 0]);
    }

    #[test]
    fn multiple_blank_lines_bridge_to_the_min_of_neighbours() {
        // depth 1 then a blank run then depth 3 -> bridged blanks take min(1, 3) = 1.
        let src = "    a\n\n\n            b\n";
        assert_eq!(indent_levels(src, 4), vec![1, 1, 1, 3]);
    }

    #[test]
    fn tab_width_zero_does_not_panic_and_treats_as_one() {
        let src = "a\n b\n  c\n";
        assert_eq!(indent_levels(src, 0), vec![0, 1, 2]);
    }

    #[test]
    fn no_trailing_newline_is_still_handled() {
        let src = "a\n    b";
        assert_eq!(indent_levels(src, 4), vec![0, 1]);
    }
}
