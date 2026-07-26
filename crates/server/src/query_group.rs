//! Boolean query grouping — `OR` / `NOT` / parentheses over opaque leaf tokens (CPE-1062, epic CPE-703).
//!
//! [`index_query`](crate::index_query) is an AND-only grammar: every term must match. This module adds the
//! boolean *structure* around that — `OR`, `NOT` (or its `-token` shorthand), and parenthesised grouping —
//! without knowing anything about what a leaf token *means*. A leaf is just an opaque string; [`eval`] asks
//! a caller-supplied predicate whether a given leaf matches the item under test. A later ticket wires a real
//! leaf (e.g. `size:>1mb`) into that predicate — this module is deliberately standalone, std-only, with no
//! dependency on `index_query.rs` or any sibling filter module, so it is fully testable with a stub matcher.
//!
//! ## Grammar (precedence low → high)
//! `OR`  <  `AND` (implicit by juxtaposition, or the explicit word `AND`)  <  `NOT` / `-`  <  `( … )`.
//!
//! - Whitespace separates tokens. Two leaves side by side are ANDed (`a b` == `a AND b`).
//! - `OR` (case-insensitive) separates the lowest-precedence alternatives: `a OR b c` parses as
//!   `a OR (b AND c)` — `AND`/juxtaposition binds tighter than `OR`.
//! - `NOT` (case-insensitive) or a leading `-` with no space (`-token`) negates the *next* atom and binds
//!   tighter than juxtaposition: `NOT a b` parses as `(NOT a) AND b`, not `NOT (a AND b)`.
//! - `( … )` groups a sub-expression at any nesting depth, overriding all of the above.
//! - **Unbalanced parens are tolerated, never a parse error/panic**: a `(` with no matching `)` before the
//!   end of input auto-closes at EOF (everything after it up to EOF is taken as its contents); a stray `)`
//!   with no matching `(` is simply skipped as a no-op, and parsing continues (any expression before and
//!   after a skipped `)` is ANDed together).
//! - **An empty query matches everything**: it parses to `Node::And(vec![])`, and [`eval`] on an empty
//!   `And`/`Not`-free-of-content node is vacuously true (`Iterator::all` over zero items is `true`).
//! - **Nesting is depth-bounded, so adversarial input can never blow the call stack.** Both `(` groups and
//!   stacked `NOT`/`-` prefixes recurse; past [`MAX_DEPTH`] levels the parser stops recursing and instead
//!   folds the rest of that nesting into ordinary, non-recursive content (see `MAX_DEPTH`'s doc). This
//!   keeps every real query's shape identical while guaranteeing `parse` always returns — no panic, and no
//!   stack overflow (which, unlike a panic, is an uncatchable process abort) even on thousands of nested
//!   parens or `NOT`s. Since `parse` only ever produces a depth-bounded tree, [`eval`]'s recursion over it
//!   is bounded too; `eval` also carries its own depth cap as a second, independent guard against a
//!   pathological `Node` built by hand (outside of `parse`) rather than relying solely on that guarantee.

/// Maximum nesting depth `parse` will recurse into — for parenthesised groups and for stacked `NOT`/`-`
/// prefixes alike — and the matching depth cap `eval` enforces on its own recursion. Far beyond anything a
/// real, hand-typed (or even hand-nested test) query would ever need; it exists purely to bound the call
/// stack against adversarial/pasted input (e.g. `"(".repeat(10_000)`), which would otherwise recurse the
/// parser (or, for a manually-built deep `Node`, `eval`) until the stack overflows — an uncatchable process
/// abort, not a `panic!` that could be caught. Once a nesting chain hits this cap, `parse` stops recursing
/// and instead treats the excess `(`/`NOT` as ordinary, non-recursive content (see [`parse_atom`] and
/// [`parse_not`]); `eval` falls back to a permissive `true` past the same cap.
const MAX_DEPTH: usize = 128;

/// A parsed boolean query over opaque leaf tokens.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Node {
    And(Vec<Node>),
    Or(Vec<Node>),
    Not(Box<Node>),
    Leaf(String),
}

/// A single lexed token. Internal to the parser — never exposed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    LParen,
    RParen,
    Or,
    And,
    Not,
    Word(String),
}

/// Split `input` into tokens. `(` and `)` are hard separators (a leaf never contains one). A word starting
/// with `-` and having at least one more character splits into a [`Tok::Not`] followed by the rest as a
/// [`Tok::Word`] (so `-token` tokenizes identically to `NOT token`); the keywords `OR`/`AND`/`NOT` are
/// recognised case-insensitively, everything else is an opaque leaf word (case preserved).
fn lex(input: &str) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '(' {
            toks.push(Tok::LParen);
            chars.next();
            continue;
        }
        if c == ')' {
            toks.push(Tok::RParen);
            chars.next();
            continue;
        }
        let mut word = String::new();
        while let Some(&c2) = chars.peek() {
            if c2.is_whitespace() || c2 == '(' || c2 == ')' {
                break;
            }
            word.push(c2);
            chars.next();
        }
        if word.is_empty() {
            continue; // defensive; shouldn't happen given the checks above
        }
        if let Some(rest) = word.strip_prefix('-') {
            if rest.is_empty() {
                // A lone "-" with nothing attached: treat as a literal leaf rather than a dangling NOT.
                toks.push(Tok::Word(word));
            } else {
                toks.push(Tok::Not);
                toks.push(Tok::Word(rest.to_string()));
            }
            continue;
        }
        match word.to_ascii_uppercase().as_str() {
            "OR" => toks.push(Tok::Or),
            "AND" => toks.push(Tok::And),
            "NOT" => toks.push(Tok::Not),
            _ => toks.push(Tok::Word(word)),
        }
    }
    toks
}

/// Whether `tok` can begin an atom (a leaf, a `NOT`, or a parenthesised group) — used to decide whether the
/// implicit-AND loop should keep consuming another operand.
fn starts_atom(tok: Option<&Tok>) -> bool {
    matches!(tok, Some(Tok::Word(_)) | Some(Tok::Not) | Some(Tok::LParen))
}

/// Parse a query string into a predicate tree over opaque leaf tokens. See the module docs for the full
/// grammar, precedence, unbalanced-parens / empty-query rules, and the [`MAX_DEPTH`] nesting bound. Never
/// panics and never overflows the stack, however deeply nested (or however long) the input is.
pub fn parse(query: &str) -> Node {
    let tokens = lex(query);
    let mut pos = 0;
    let mut parts = Vec::new();
    loop {
        // A stray, unmatched `)` at top level is a no-op: skip past it and keep parsing.
        while matches!(tokens.get(pos), Some(Tok::RParen)) {
            pos += 1;
        }
        if pos >= tokens.len() {
            break;
        }
        parts.push(parse_or(&tokens, &mut pos, 0));
    }
    if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        Node::And(parts) // empty input -> And(vec![]) matches everything; multiple -> ANDed together
    }
}

/// `OR`-level: one or more AND-groups separated by `OR`. `depth` is the current paren/NOT nesting depth
/// (see [`MAX_DEPTH`]); it is only ever incremented by [`parse_atom`] opening a new group.
fn parse_or(tokens: &[Tok], pos: &mut usize, depth: usize) -> Node {
    let mut parts = vec![parse_and(tokens, pos, depth)];
    while matches!(tokens.get(*pos), Some(Tok::Or)) {
        *pos += 1;
        parts.push(parse_and(tokens, pos, depth));
    }
    if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        Node::Or(parts)
    }
}

/// `AND`-level: one or more atoms joined by juxtaposition or the explicit word `AND`.
fn parse_and(tokens: &[Tok], pos: &mut usize, depth: usize) -> Node {
    let mut parts = Vec::new();
    loop {
        if matches!(tokens.get(*pos), Some(Tok::And)) {
            *pos += 1; // explicit AND is a no-op separator; juxtaposition already means AND
            continue;
        }
        if starts_atom(tokens.get(*pos)) {
            parts.push(parse_not(tokens, pos, depth));
        } else {
            break;
        }
    }
    if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        Node::And(parts) // zero atoms between two operators (e.g. "a OR OR b") -> matches-all placeholder
    }
}

/// `NOT`-level: zero or more `NOT`/`-` prefixes (stacking, so `NOT NOT a` cancels out structurally, though
/// not simplified) around a single atom. Each prefix recurses and so counts against [`MAX_DEPTH`]: once the
/// cap is hit, any further `NOT`/`-` tokens are consumed without adding another `Node::Not` wrapper or
/// another stack frame — tolerant recovery, not a crash, for a pathological run of thousands of `NOT`s.
fn parse_not(tokens: &[Tok], pos: &mut usize, depth: usize) -> Node {
    if matches!(tokens.get(*pos), Some(Tok::Not)) {
        if depth < MAX_DEPTH {
            *pos += 1;
            return Node::Not(Box::new(parse_not(tokens, pos, depth + 1)));
        }
        // Cap reached: swallow any further NOT prefixes iteratively (no added recursion/stack growth).
        while matches!(tokens.get(*pos), Some(Tok::Not)) {
            *pos += 1;
        }
    }
    parse_atom(tokens, pos, depth)
}

/// Innermost level: a leaf token, or a parenthesised sub-expression. Tolerates a missing closing paren by
/// simply not consuming one (the sub-expression's content runs to EOF or to whatever stopped `parse_or`).
/// Opening a group recurses and so counts against [`MAX_DEPTH`]: once the cap is hit, a further `(` is
/// treated as an ordinary (non-grouping) leaf character instead of opening another nested group — tolerant
/// recovery, not a crash, for pathological input like thousands of nested parens.
fn parse_atom(tokens: &[Tok], pos: &mut usize, depth: usize) -> Node {
    match tokens.get(*pos) {
        Some(Tok::LParen) if depth < MAX_DEPTH => {
            *pos += 1;
            let inner = parse_or(tokens, pos, depth + 1);
            if matches!(tokens.get(*pos), Some(Tok::RParen)) {
                *pos += 1;
            }
            // else: unbalanced "(" with no matching ")" before EOF — auto-close, don't panic.
            inner
        }
        Some(Tok::LParen) => {
            // MAX_DEPTH reached: don't recurse into another parse_or (that's the stack-overflow vector for
            // adversarial input like "(".repeat(10_000)) — fold this "(" into a literal leaf instead.
            *pos += 1;
            Node::Leaf("(".to_string())
        }
        Some(Tok::Word(w)) => {
            let w = w.clone();
            *pos += 1;
            Node::Leaf(w)
        }
        // Called only when `starts_atom` said yes, or at the top of an empty/degenerate group; fall back to
        // the empty match-everything node rather than panicking on anything unexpected.
        _ => Node::And(vec![]),
    }
}

/// Evaluate the tree; `leaf` decides whether an opaque leaf token matches the item under test. An empty
/// `And(vec![])` (the empty-query / degenerate case) evaluates to `true`; an empty `Or(vec![])` evaluates to
/// `false` — each follows from the ordinary identity element of AND / OR over zero operands.
///
/// Recursion depth is capped at [`MAX_DEPTH`] as a second, independent guard: a tree produced by [`parse`]
/// is already bounded to that depth, so this never triggers on a parsed query, but `Node` is a public,
/// hand-constructible type, so `eval` doesn't rely solely on `parse`'s guarantee. Past the cap, `eval`
/// stops recursing and falls back to `true` (the same permissive default as an empty `And`) rather than
/// overflowing the stack.
pub fn eval(node: &Node, leaf: &impl Fn(&str) -> bool) -> bool {
    eval_capped(node, leaf, 0)
}

fn eval_capped(node: &Node, leaf: &impl Fn(&str) -> bool, depth: usize) -> bool {
    if depth >= MAX_DEPTH {
        return true; // pathological hand-built tree past the depth cap: permissive fallback, never a crash
    }
    match node {
        Node::Leaf(s) => leaf(s),
        Node::Not(inner) => !eval_capped(inner, leaf, depth + 1),
        Node::And(parts) => parts.iter().all(|n| eval_capped(n, leaf, depth + 1)),
        Node::Or(parts) => parts.iter().any(|n| eval_capped(n, leaf, depth + 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// A stub leaf matcher: a token matches iff it's in `present`.
    fn matcher<'a>(present: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
        let set: HashSet<&str> = present.iter().copied().collect();
        move |tok: &str| set.contains(tok)
    }

    #[test]
    fn empty_query_matches_all_no_panic() {
        let node = parse("");
        assert_eq!(node, Node::And(vec![]));
        assert!(eval(&node, &matcher(&[])));
        assert!(eval(&node, &matcher(&["anything"])));

        let ws = parse("   ");
        assert_eq!(ws, Node::And(vec![]));
        assert!(eval(&ws, &matcher(&[])));
    }

    #[test]
    fn single_leaf() {
        let node = parse("a");
        assert_eq!(node, Node::Leaf("a".to_string()));
        assert!(eval(&node, &matcher(&["a"])));
        assert!(!eval(&node, &matcher(&["b"])));
    }

    #[test]
    fn implicit_and_juxtaposition() {
        let node = parse("a b");
        assert_eq!(
            node,
            Node::And(vec![Node::Leaf("a".to_string()), Node::Leaf("b".to_string())])
        );
        assert!(eval(&node, &matcher(&["a", "b"])));
        assert!(!eval(&node, &matcher(&["a"])));
        assert!(!eval(&node, &matcher(&["b"])));
    }

    #[test]
    fn explicit_and_keyword_same_as_juxtaposition() {
        let explicit = parse("a AND b");
        let implicit = parse("a b");
        assert_eq!(explicit, implicit);
        let mixed_case = parse("a and b");
        assert_eq!(mixed_case, implicit);
    }

    #[test]
    fn or_binds_looser_than_and() {
        // a OR b c  ==  a OR (b AND c)
        let node = parse("a OR b c");
        assert_eq!(
            node,
            Node::Or(vec![
                Node::Leaf("a".to_string()),
                Node::And(vec![Node::Leaf("b".to_string()), Node::Leaf("c".to_string())]),
            ])
        );
        assert!(eval(&node, &matcher(&["a"])));
        assert!(eval(&node, &matcher(&["b", "c"])));
        assert!(!eval(&node, &matcher(&["b"]))); // b without c: neither disjunct holds
        assert!(!eval(&node, &matcher(&[])));
    }

    #[test]
    fn or_is_case_insensitive() {
        assert_eq!(parse("a or b"), parse("a OR b"));
        assert_eq!(parse("a Or b"), parse("a OR b"));
    }

    #[test]
    fn not_binds_tighter_than_and() {
        // NOT a b  ==  (NOT a) AND b
        let node = parse("NOT a b");
        assert_eq!(
            node,
            Node::And(vec![
                Node::Not(Box::new(Node::Leaf("a".to_string()))),
                Node::Leaf("b".to_string()),
            ])
        );
        assert!(eval(&node, &matcher(&["b"])));
        assert!(!eval(&node, &matcher(&["a", "b"])));
        assert!(!eval(&node, &matcher(&[])));
    }

    #[test]
    fn dash_prefix_equals_not() {
        assert_eq!(parse("-x"), parse("NOT x"));
        assert_eq!(parse("-x y"), parse("NOT x y"));
        let node = parse("-x");
        assert!(eval(&node, &matcher(&[])));
        assert!(!eval(&node, &matcher(&["x"])));
    }

    #[test]
    fn not_is_case_insensitive_keyword() {
        assert_eq!(parse("not x"), parse("NOT x"));
    }

    #[test]
    fn parens_group_and_override_precedence() {
        // Without parens, "a OR b c" is a OR (b AND c). With parens: (a OR b) AND c.
        let grouped = parse("(a OR b) c");
        assert_eq!(
            grouped,
            Node::And(vec![
                Node::Or(vec![Node::Leaf("a".to_string()), Node::Leaf("b".to_string())]),
                Node::Leaf("c".to_string()),
            ])
        );
        assert!(eval(&grouped, &matcher(&["a", "c"])));
        assert!(eval(&grouped, &matcher(&["b", "c"])));
        assert!(!eval(&grouped, &matcher(&["a", "b"]))); // missing c
        assert!(!eval(&grouped, &matcher(&["c"]))); // missing a and b
    }

    #[test]
    fn nested_parens() {
        // NOT (a OR (b AND NOT c))
        let node = parse("NOT (a OR (b -c))");
        let expect = Node::Not(Box::new(Node::Or(vec![
            Node::Leaf("a".to_string()),
            Node::And(vec![
                Node::Leaf("b".to_string()),
                Node::Not(Box::new(Node::Leaf("c".to_string()))),
            ]),
        ])));
        assert_eq!(node, expect);
        // True when: not a, and not(b and not c) => not a, and (not b or c)
        assert!(eval(&node, &matcher(&[]))); // nothing present
        assert!(!eval(&node, &matcher(&["a"])));
        assert!(!eval(&node, &matcher(&["b"]))); // b true, c false -> inner true -> outer false
        assert!(eval(&node, &matcher(&["b", "c"]))); // b and c -> inner false -> outer true
    }

    #[test]
    fn de_morgan_not_or_equals_and_of_nots() {
        for present in [
            vec![],
            vec!["a"],
            vec!["b"],
            vec!["a", "b"],
        ] {
            let m = matcher(&present);
            let lhs = eval(&parse("NOT (a OR b)"), &m);
            let rhs = eval(&parse("(NOT a) (NOT b)"), &m);
            assert_eq!(lhs, rhs, "De Morgan mismatch for present={present:?}");
        }
    }

    #[test]
    fn unbalanced_open_paren_auto_closes_at_eof() {
        // "(a b" never closes; tolerate by treating the rest of input as the group's contents.
        let node = parse("(a b");
        assert_eq!(
            node,
            Node::And(vec![Node::Leaf("a".to_string()), Node::Leaf("b".to_string())])
        );
        assert!(eval(&node, &matcher(&["a", "b"])));
    }

    #[test]
    fn unbalanced_close_paren_is_skipped() {
        // A stray ")" with nothing to match: skip it, still parse what's around it, never panic.
        let node = parse("a) b");
        assert_eq!(
            node,
            Node::And(vec![Node::Leaf("a".to_string()), Node::Leaf("b".to_string())])
        );
        assert!(eval(&node, &matcher(&["a", "b"])));

        let lone = parse(")");
        assert_eq!(lone, Node::And(vec![]));
        assert!(eval(&lone, &matcher(&[])));
    }

    #[test]
    fn deeply_nested_parens_still_parse() {
        let node = parse("(((a)))");
        assert_eq!(node, Node::Leaf("a".to_string()));
        assert!(eval(&node, &matcher(&["a"])));
    }

    #[test]
    fn or_chain_of_more_than_two() {
        let node = parse("a OR b OR c");
        assert_eq!(
            node,
            Node::Or(vec![
                Node::Leaf("a".to_string()),
                Node::Leaf("b".to_string()),
                Node::Leaf("c".to_string()),
            ])
        );
        assert!(eval(&node, &matcher(&["c"])));
        assert!(!eval(&node, &matcher(&[])));
    }

    #[test]
    fn opaque_leaf_tokens_are_preserved_verbatim() {
        // Leaves aren't lowercased or otherwise interpreted — a later ticket maps them to real filters.
        let node = parse("Size:>1MB");
        assert_eq!(node, Node::Leaf("Size:>1MB".to_string()));
        assert!(eval(&node, &matcher(&["Size:>1MB"])));
        assert!(!eval(&node, &matcher(&["size:>1mb"])));
    }

    // --- Regression: adversarial nesting must never overflow the stack (reviewer repro on PR #380) ---

    #[test]
    fn ten_thousand_open_parens_do_not_overflow_the_stack() {
        // Before the MAX_DEPTH bound, this recursed once per "(" with no cap: "(".repeat(1_000) parsed
        // fine but "(".repeat(10_000) crashed a release build with STATUS_STACK_OVERFLOW (an uncatchable
        // process abort, not a panic). It must now return normally and evaluate without crashing. Past
        // MAX_DEPTH, the excess "(" fold into literal leaves (an AND of 9,872 "(" tokens here), so the
        // specific boolean isn't the point — not crashing, on either parse or eval, is.
        let node = parse(&"(".repeat(10_000));
        let _ = eval(&node, &matcher(&[]));
        let _ = eval(&node, &matcher(&["anything"]));
    }

    #[test]
    fn ten_thousand_close_parens_do_not_overflow_the_stack() {
        // All-unmatched close-parens: each is skipped as a no-op at top level; must not recurse at all,
        // let alone overflow.
        let node = parse(&")".repeat(10_000));
        assert_eq!(node, Node::And(vec![]));
        assert!(eval(&node, &matcher(&[])));
    }

    #[test]
    fn ten_thousand_stacked_not_prefixes_do_not_overflow_the_stack() {
        // The other unbounded-recursion vector: a long run of "NOT NOT NOT ... x" (or "-" chained without
        // spaces isn't possible since "-" only prefixes once per lexed word, so use the keyword form).
        let query = "NOT ".repeat(10_000) + "x";
        let node = parse(&query);
        // Must not crash to build or to evaluate, whatever the resulting boolean is.
        let _ = eval(&node, &matcher(&["x"]));
        let _ = eval(&node, &matcher(&[]));
    }

    #[test]
    fn mixed_deep_nesting_does_not_overflow_the_stack() {
        // Parens and NOTs interleaved, past the cap in both dimensions at once.
        let query = format!("{}{}x{}", "(".repeat(5_000), "NOT ".repeat(5_000), ")".repeat(5_000));
        let node = parse(&query);
        let _ = eval(&node, &matcher(&["x"]));
    }

    #[test]
    fn depth_bound_leaves_normal_queries_unaffected() {
        // Nesting well under MAX_DEPTH must parse exactly as it did before the bound was added.
        let shallow = "(".repeat(10) + "a" + &")".repeat(10);
        assert_eq!(parse(&shallow), Node::Leaf("a".to_string()));
        let not_chain = "NOT ".repeat(3) + "a";
        assert_eq!(
            parse(&not_chain),
            Node::Not(Box::new(Node::Not(Box::new(Node::Not(Box::new(Node::Leaf(
                "a".to_string()
            )))))))
        );
    }
}
