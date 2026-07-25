//! Parse a coding agent's terminal output for token/cost **usage** (CPE-311).
//!
//! Running an agent against a paid provider costs tokens and money. Some agents print their usage to
//! the terminal — Claude Code shows a session cost line, others print token counts. This scanner taps
//! that already-captured output stream — **read-only, it never modifies the stream**, exactly like
//! the read-tap (CPE-405) — and extracts per-session token/cost figures so the console can surface
//! "this session has cost you ~$X / N tokens" without surprising the user.
//!
//! It is deliberately conservative and degrades **silently**: an unrecognized line contributes
//! nothing, an agent that prints no usage simply shows none, and nothing here can block or corrupt
//! the PTY. Because different agents print cumulative-vs-delta figures inconsistently, each metric
//! keeps the **maximum** value seen in the session — robust to a running total being reprinted, and
//! never double-counting. This is a *display* of what the provider itself reported; it sends nothing
//! anywhere (there is no outbound telemetry — see the ticket's AC2/AC3).

use serde::Serialize;

/// Accumulated, provider-reported usage for one session. All fields are best-effort and may be zero
/// if the agent prints nothing recognizable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Usage {
    /// Input/prompt tokens reported.
    pub input_tokens: u64,
    /// Output/completion tokens reported.
    pub output_tokens: u64,
    /// Cost in US dollars reported (e.g. Claude Code's session cost line).
    pub cost_usd: f64,
}

impl Usage {
    /// Whether anything was ever reported (so the UI can hide an all-zero readout).
    pub fn is_empty(&self) -> bool {
        self.input_tokens == 0 && self.output_tokens == 0 && self.cost_usd == 0.0
    }

    /// Fold in a newly-observed figure, keeping the max per metric (see module note on why max).
    fn absorb(&mut self, other: Observed) {
        if let Some(i) = other.input_tokens {
            self.input_tokens = self.input_tokens.max(i);
        }
        if let Some(o) = other.output_tokens {
            self.output_tokens = self.output_tokens.max(o);
        }
        if let Some(c) = other.cost_usd {
            if c > self.cost_usd {
                self.cost_usd = c;
            }
        }
    }
}

/// What a single line reported (any subset of the metrics).
#[derive(Default)]
struct Observed {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cost_usd: Option<f64>,
}

impl Observed {
    fn is_empty(&self) -> bool {
        self.input_tokens.is_none() && self.output_tokens.is_none() && self.cost_usd.is_none()
    }
}

/// A trailing line longer than this without a newline is dropped, so a pathological stream (a
/// spinner/redraw with no `\n`) can't grow the buffer without bound. Mirrors the read-tap.
const MAX_LINE: usize = 8 * 1024;

/// Stateful, incremental scanner over an agent's terminal output. Feed it raw output chunks; it
/// folds any usage it recognizes into a running [`Usage`] total for the session. Line-buffered so a
/// figure split across two `feed` calls is still matched.
#[derive(Default)]
pub struct UsageScanner {
    buf: String,
    total: Usage,
}

impl UsageScanner {
    pub fn new() -> UsageScanner {
        UsageScanner::default()
    }

    /// Feed the next chunk; returns the updated session total (cheap `Copy`).
    pub fn feed(&mut self, chunk: &str) -> Usage {
        self.buf.push_str(chunk);
        while let Some(nl) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=nl).collect();
            let obs = parse_line(&line);
            if !obs.is_empty() {
                self.total.absorb(obs);
            }
        }
        if self.buf.len() > MAX_LINE {
            self.buf.clear();
        }
        self.total
    }

    /// The current session total.
    pub fn total(&self) -> Usage {
        self.total
    }
}

/// Strip ANSI so matching runs on visible text (same handling as the read-tap).
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                for d in chars.by_ref() {
                    if ('@'..='~').contains(&d) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some(d) = chars.next() {
                    if d == '\x07' {
                        break;
                    }
                    if d == '\x1b' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Parse one line for usage figures. `<n>` may carry thousands separators or a `k`/`m` suffix
/// (`1.2k`, `3,456`, `1.5M`). Recognizes, conservatively:
/// - a **cost** line: contains "cost" and a `$<number>` (e.g. `Total cost: $0.1234`).
/// - **token** figures: `<n> input`/`input: <n>` and `<n> output`/`output: <n>`, and a bare
///   `<n> tokens` (counted as input when no direction is given).
fn parse_line(line: &str) -> Observed {
    let clean = strip_ansi(line);
    let lower = clean.to_lowercase();
    let mut obs = Observed::default();

    if lower.contains("cost") {
        if let Some(c) = dollars_after(&lower, "cost") {
            obs.cost_usd = Some(c);
        }
    }
    obs.input_tokens = tokens_for(&lower, "input");
    obs.output_tokens = tokens_for(&lower, "output");
    // A bare "N tokens" with no direction → treat as input so it isn't lost.
    if obs.input_tokens.is_none() && obs.output_tokens.is_none() {
        if let Some(n) = bare_tokens(&lower) {
            obs.input_tokens = Some(n);
        }
    }
    obs
}

/// The first `$<number>` appearing at or after the first occurrence of `anchor`.
fn dollars_after(lower: &str, anchor: &str) -> Option<f64> {
    let from = lower.find(anchor)? + anchor.len();
    let rest = &lower[from..];
    let dol = rest.find('$')?;
    let after = &rest[dol + 1..];
    let num: String = after.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    let num = num.trim_end_matches([',', '.']).replace(',', "");
    num.parse::<f64>().ok()
}

/// A token count for direction `dir` ("input"/"output"). Two shapes are supported, disambiguated by
/// what follows the direction word:
/// - **`<dir>: <n>`** (also `<dir> tokens: <n>`) — the number *follows*; taken when a number sits
///   right after `dir` (skipping an optional `tokens` word and a `:`).
/// - **`<n> <dir>`** — the number *precedes* (e.g. `1.2k input, 3.4k output`); taken otherwise, as
///   the last number before `dir`.
fn tokens_for(lower: &str, dir: &str) -> Option<u64> {
    let pos = lower.find(dir)?;
    let after = &lower[pos + dir.len()..];
    if let Some(n) = number_after(after) {
        return Some(n);
    }
    last_count(&lower[..pos])
}

/// If the text right after a direction word is `[tokens][:] <n>`, return `<n>`. Skips one optional
/// `tokens` word and one optional `:`, plus surrounding spaces; returns `None` if a number isn't
/// what follows (i.e. this is the `<n> <dir>` shape instead).
fn number_after(after: &str) -> Option<u64> {
    let mut s = after.trim_start();
    if let Some(rest) = s.strip_prefix("tokens") {
        s = rest.trim_start();
    }
    if let Some(rest) = s.strip_prefix(':') {
        s = rest.trim_start();
    }
    if s.chars().next()?.is_ascii_digit() {
        first_count(s)
    } else {
        None
    }
}

/// A bare `<n> tokens` (no input/output direction).
fn bare_tokens(lower: &str) -> Option<u64> {
    let pos = lower.find("token")?;
    let before = &lower[..pos];
    last_count(before)
}

/// Parse the first number-with-optional-k/m-suffix in `s`.
fn first_count(s: &str) -> Option<u64> {
    let bytes = s.char_indices().collect::<Vec<_>>();
    let mut i = 0;
    while i < bytes.len() {
        let (_, c) = bytes[i];
        if c.is_ascii_digit() {
            let start = bytes[i].0;
            let mut end = s.len();
            for &(idx, ch) in &bytes[i..] {
                if !(ch.is_ascii_digit() || ch == '.' || ch == ',' || ch == 'k' || ch == 'm') {
                    end = idx;
                    break;
                }
            }
            return parse_count(&s[start..end]);
        }
        i += 1;
    }
    None
}

/// Parse the last number-with-optional-k/m-suffix in `s`. Anchors on the last *count* character
/// (so a trailing `k`/`m` suffix is kept), extends left over the run, then validates via
/// [`parse_count`] — which rejects a suffix-only run like `"k"`.
fn last_count(s: &str) -> Option<u64> {
    let is_count = |c: char| c.is_ascii_digit() || c == '.' || c == ',' || c == 'k' || c == 'm';
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut end: Option<usize> = None;
    let mut start = 0usize;
    let mut i = chars.len();
    while i > 0 {
        i -= 1;
        let (idx, c) = chars[i];
        if end.is_none() {
            if is_count(c) {
                end = Some(idx + c.len_utf8());
                start = idx;
            }
            // else: trailing space/letter/punct — keep scanning left for a number.
        } else if is_count(c) {
            start = idx;
        } else {
            break;
        }
    }
    end.map(|e| &s[start..e]).and_then(parse_count)
}

/// Turn `"1.2k"`, `"3,456"`, `"1.5m"`, `"789"` into a token count.
fn parse_count(raw: &str) -> Option<u64> {
    let raw = raw.trim().trim_end_matches([',', '.']);
    if raw.is_empty() {
        return None;
    }
    let (mult, body) = if let Some(b) = raw.strip_suffix('k') {
        (1_000.0, b)
    } else if let Some(b) = raw.strip_suffix('m') {
        (1_000_000.0, b)
    } else {
        (1.0, raw)
    };
    let body = body.replace(',', "");
    let n: f64 = body.parse().ok()?;
    Some((n * mult).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_claude_code_cost_line() {
        let mut s = UsageScanner::new();
        let u = s.feed("Total cost: $0.1234 (some session)\n");
        assert!((u.cost_usd - 0.1234).abs() < 1e-9, "got {u:?}");
    }

    #[test]
    fn parses_directional_token_counts() {
        let mut s = UsageScanner::new();
        let u = s.feed("Tokens: 1.2k input, 3.4k output\n");
        assert_eq!(u.input_tokens, 1_200);
        assert_eq!(u.output_tokens, 3_400);
    }

    #[test]
    fn parses_colon_style_and_thousands_separators() {
        let mut s = UsageScanner::new();
        let u = s.feed("input: 12,345  output: 6,789\n");
        assert_eq!(u.input_tokens, 12_345);
        assert_eq!(u.output_tokens, 6_789);
    }

    #[test]
    fn keeps_the_max_when_a_running_total_is_reprinted() {
        let mut s = UsageScanner::new();
        s.feed("Total cost: $0.10\n");
        s.feed("Total cost: $0.25\n");
        let u = s.feed("Total cost: $0.25\n"); // reprint must not double
        assert!((u.cost_usd - 0.25).abs() < 1e-9, "got {u:?}");
    }

    #[test]
    fn a_bare_token_count_is_counted_as_input() {
        let mut s = UsageScanner::new();
        let u = s.feed("used 500 tokens\n");
        assert_eq!(u.input_tokens, 500);
        assert_eq!(u.output_tokens, 0);
    }

    #[test]
    fn strips_ansi_before_parsing() {
        let mut s = UsageScanner::new();
        let u = s.feed("\x1b[2mTotal cost:\x1b[0m \x1b[1m$1.50\x1b[0m\n");
        assert!((u.cost_usd - 1.50).abs() < 1e-9, "got {u:?}");
    }

    #[test]
    fn joins_a_figure_split_across_chunks() {
        let mut s = UsageScanner::new();
        assert_eq!(s.feed("Tokens: 2.5k inp").input_tokens, 0); // no newline yet
        let u = s.feed("ut, 1k output\n");
        assert_eq!(u.input_tokens, 2_500);
        assert_eq!(u.output_tokens, 1_000);
    }

    #[test]
    fn unknown_output_yields_nothing() {
        let mut s = UsageScanner::new();
        let u = s.feed("aider> loading model...\nApplied edit to main.py\n");
        assert!(u.is_empty(), "got {u:?}");
    }

    #[test]
    fn a_runaway_line_without_newline_does_not_grow_unbounded() {
        let mut s = UsageScanner::new();
        let huge = "x".repeat(MAX_LINE + 100);
        assert!(s.feed(&huge).is_empty());
        // Buffer reset; a subsequent real figure still parses.
        let u = s.feed("Total cost: $0.05\n");
        assert!((u.cost_usd - 0.05).abs() < 1e-9);
    }
}
