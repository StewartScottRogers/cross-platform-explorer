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
/// grammar, precedence, and the unbalanced-parens / empty-query rules. Never panics.
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
        parts.push(parse_or(&tokens, &mut pos));
    }
    if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        Node::And(parts) // empty input -> And(vec![]) matches everything; multiple -> ANDed together
    }
}

/// `OR`-level: one or more AND-groups separated by `OR`.
fn parse_or(tokens: &[Tok], pos: &mut usize) -> Node {
    let mut parts = vec![parse_and(tokens, pos)];
    while matches!(tokens.get(*pos), Some(Tok::Or)) {
        *pos += 1;
        parts.push(parse_and(tokens, pos));
    }
    if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        Node::Or(parts)
    }
}

/// `AND`-level: one or more atoms joined by juxtaposition or the explicit word `AND`.
fn parse_and(tokens: &[Tok], pos: &mut usize) -> Node {
    let mut parts = Vec::new();
    loop {
        if matches!(tokens.get(*pos), Some(Tok::And)) {
            *pos += 1; // explicit AND is a no-op separator; juxtaposition already means AND
            continue;
        }
        if starts_atom(tokens.get(*pos)) {
            parts.push(parse_not(tokens, pos));
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
/// not simplified) around a single atom.
fn parse_not(tokens: &[Tok], pos: &mut usize) -> Node {
    if matches!(tokens.get(*pos), Some(Tok::Not)) {
        *pos += 1;
        return Node::Not(Box::new(parse_not(tokens, pos)));
    }
    parse_atom(tokens, pos)
}

/// Innermost level: a leaf token, or a parenthesised sub-expression. Tolerates a missing closing paren by
/// simply not consuming one (the sub-expression's content runs to EOF or to whatever stopped `parse_or`).
fn parse_atom(tokens: &[Tok], pos: &mut usize) -> Node {
    match tokens.get(*pos) {
        Some(Tok::LParen) => {
            *pos += 1;
            let inner = parse_or(tokens, pos);
            if matches!(tokens.get(*pos), Some(Tok::RParen)) {
                *pos += 1;
            }
            // else: unbalanced "(" with no matching ")" before EOF — auto-close, don't panic.
            inner
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
pub fn eval(node: &Node, leaf: &impl Fn(&str) -> bool) -> bool {
    match node {
        Node::Leaf(s) => leaf(s),
        Node::Not(inner) => !eval(inner, leaf),
        Node::And(parts) => parts.iter().all(|n| eval(n, leaf)),
        Node::Or(parts) => parts.iter().any(|n| eval(n, leaf)),
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
}
