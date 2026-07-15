//! Parse a coding agent's terminal output for file READ operations (CPE-405).
//!
//! The Agent Watch filesystem watcher (CPE-398) sees writes/creates/deletes, but a Windows FS
//! watcher can't report **reads** — and reads are the missing half of "understand what the agent
//! is doing" (knowing which files it consulted explains its edits). Reads therefore can't come from
//! the filesystem; they must come from the agent's own activity. Agents like Claude Code print each
//! tool call (`Read(path)`, `Edit(path)`, `Bash(…)`) to the terminal. This scanner taps that
//! already-captured output stream — **read-only, it never modifies the stream** — and extracts the
//! paths the agent reported reading.
//!
//! It is deliberately conservative and fragile-by-design (the ticket says so): it recognizes one
//! agent's format (Claude Code) and degrades **silently** — an unrecognized line yields nothing, a
//! different agent simply produces no reads, and nothing here can block or corrupt the PTY.

/// Stateful, incremental scanner over an agent's terminal output. Feed it raw output chunks (which
/// may split lines mid-way and carry ANSI escapes); it returns the file paths reported as *reads*
/// in any newly-completed lines. Line-buffered, so a tool-call line split across two `feed` calls
/// is still matched.
#[derive(Default)]
pub struct ReadScanner {
    /// Carry of an incomplete trailing line between `feed` calls.
    buf: String,
}

/// A trailing line longer than this without a newline (e.g. a spinner/redraw with no `\n`) is
/// dropped, so a pathological stream can't grow the buffer without bound.
const MAX_LINE: usize = 8 * 1024;

impl ReadScanner {
    pub fn new() -> ReadScanner {
        ReadScanner::default()
    }

    /// Feed the next chunk of terminal output; return any read paths detected in lines it completed.
    /// The returned strings are the raw captured path text (relative or absolute) — the caller
    /// resolves them against the session's working directory.
    pub fn feed(&mut self, chunk: &str) -> Vec<String> {
        self.buf.push_str(chunk);
        let mut out = Vec::new();
        while let Some(nl) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=nl).collect();
            if let Some(path) = read_path_in_line(&line) {
                out.push(path);
            }
        }
        if self.buf.len() > MAX_LINE {
            self.buf.clear();
        }
        out
    }
}

/// Extract the path from a single line if it is a Claude Code `Read(<path>)` tool-call line, else
/// `None`. Strips ANSI, then requires the visible text to be *tool-call-shaped* — only a bullet /
/// whitespace may precede the tool name — so `Read(` appearing mid-sentence (e.g. "I Read(the
/// docs)") is ignored. Case-sensitive `Read` (the literal tool name), and truncated paths (with an
/// ellipsis) are dropped since they can't be resolved.
pub fn read_path_in_line(line: &str) -> Option<String> {
    let clean = strip_ansi(line);
    // Drop any leading non-alphanumeric run (the `●`/`⏺` bullet and spaces). If real text precedes
    // the tool name (prose, a numbered list, a `⎿ Read 42 lines` result), the prefix stops at that
    // alphanumeric and `Read(` is no longer at the head.
    let head = clean.trim().trim_start_matches(|c: char| !c.is_alphanumeric());
    let inner = head.strip_prefix("Read(")?;
    let end = inner.find(')')?;
    let path = inner[..end].trim().trim_matches('"').trim();
    if path.is_empty() || path.contains('…') {
        return None;
    }
    Some(path.to_string())
}

/// Remove ANSI/VT escape sequences so matching runs on the visible text: CSI (`ESC [ … final`),
/// OSC (`ESC ] … BEL`/`ST`), and a lone `ESC`. Everything else — including UTF-8 path bytes — is
/// kept.
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
                // CSI: consume params/intermediates up to and including the final byte @..~.
                for d in chars.by_ref() {
                    if ('@'..='~').contains(&d) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                // OSC: consume up to BEL or the ST (ESC \) terminator.
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
            _ => { /* lone ESC — drop it */ }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_a_plain_read_tool_call() {
        assert_eq!(read_path_in_line("● Read(src/main.rs)").as_deref(), Some("src/main.rs"));
        assert_eq!(read_path_in_line("⏺ Read(/abs/path/file.txt)").as_deref(), Some("/abs/path/file.txt"));
        // No bullet at all, just the tool call.
        assert_eq!(read_path_in_line("Read(Cargo.toml)").as_deref(), Some("Cargo.toml"));
    }

    #[test]
    fn strips_ansi_before_matching() {
        let line = "\x1b[1m\x1b[38;5;10m● \x1b[0mRead(\x1b[4msrc/lib.rs\x1b[0m)";
        assert_eq!(read_path_in_line(line).as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn ignores_prose_and_non_read_tools_and_result_lines() {
        assert_eq!(read_path_in_line("I need to Read(the docs) before editing"), None);
        assert_eq!(read_path_in_line("● Edit(src/main.rs)"), None);
        assert_eq!(read_path_in_line("● Write(out.txt)"), None);
        assert_eq!(read_path_in_line("● Bash(cat file)"), None);
        // The result summary line under a Read call must not match.
        assert_eq!(read_path_in_line("  ⎿  Read 42 lines (ctrl+r to expand)"), None);
    }

    #[test]
    fn drops_truncated_and_empty_paths() {
        assert_eq!(read_path_in_line("● Read(a/very/long/pa…)"), None);
        assert_eq!(read_path_in_line("● Read()"), None);
    }

    #[test]
    fn scanner_joins_a_line_split_across_chunks() {
        let mut s = ReadScanner::new();
        assert!(s.feed("● Read(src/").is_empty()); // no newline yet
        assert_eq!(s.feed("app/mod.rs)\n"), vec!["src/app/mod.rs".to_string()]);
    }

    #[test]
    fn scanner_handles_several_lines_and_only_reports_reads() {
        let mut s = ReadScanner::new();
        let captured = concat!(
            "\x1b[2m● \x1b[0mRead(src/a.rs)\n",
            "  ⎿  Read 10 lines\n",
            "● Edit(src/a.rs)\n",
            "some prose about Read(x) inline\n",
            "● Read(tests/b.rs)\n",
        );
        assert_eq!(s.feed(captured), vec!["src/a.rs".to_string(), "tests/b.rs".to_string()]);
    }

    #[test]
    fn unknown_agent_output_yields_nothing() {
        let mut s = ReadScanner::new();
        assert!(s.feed("aider> loading model...\nApplied edit to main.py\n").is_empty());
        assert!(s.feed("\x1b[32m✓\x1b[0m done\n").is_empty());
    }

    #[test]
    fn a_runaway_line_without_newline_does_not_grow_unbounded() {
        let mut s = ReadScanner::new();
        let huge = "x".repeat(MAX_LINE + 100);
        assert!(s.feed(&huge).is_empty());
        // Buffer was reset; a subsequent real read still works.
        assert_eq!(s.feed("● Read(ok.rs)\n"), vec!["ok.rs".to_string()]);
    }
}
