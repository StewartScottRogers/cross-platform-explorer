//! Clone planning (CPE-436) — the **hardened** `git clone` command builder + its safety checks.
//!
//! Cloning materialises **attacker-controlled content** on the user's disk (forge-threat-model §C),
//! so this module builds a `git` invocation that runs *none* of a repo's embedded code and cannot be
//! redirected off a safe transport. The building + validation is pure (unit-tested here); the host
//! runs the resulting argv against a real `git` and streams progress (the runtime tail, CPE-436).
//!
//! The hardening mirrors forge-threat-model §C exactly:
//! - `-c core.hooksPath=` (empty) — no repo hooks run on clone/checkout,
//! - `-c protocol.ext.allow=never -c protocol.file.allow=never` — no `ext::`/`file::` transports,
//! - `-c core.fsmonitor=false` — no fsmonitor hook,
//! - `--recurse-submodules=no` — no submodule-URL injection,
//! - an **https/ssh-only** clone URL (no `git://`, `file://`, `ext::`, …),
//! - a target that must be a fresh, non-nested directory.

/// A request to clone `url` into `target_dir`. `depth` optionally makes a shallow clone; `branch`
/// optionally checks out a single ref. The token (if any) is injected by the host at run time via
/// the credential env (CPE-439), never written into these args or the repo config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneRequest {
    pub url: String,
    pub target_dir: String,
    pub depth: Option<u32>,
    pub branch: Option<String>,
}

/// Why a clone was refused before any process ran — every variant is a safe pre-flight rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneError {
    /// The URL isn't an allowed transport (only `https://` and ssh are permitted).
    BadUrl,
    /// The target path is empty, relative, or would nest inside an existing repo.
    BadTarget,
    /// The branch/ref name contains option-injection or control characters.
    BadRef,
}

/// True if `url` is an allowed clone transport: `https://…`, `ssh://…`, or scp-like `user@host:path`.
/// Everything else — `git://` (cleartext), `file://`, `ext::`, `http://`, a bare path — is refused,
/// closing the local-transport / cleartext / arbitrary-command surface (forge-threat-model §C).
pub fn is_allowed_clone_url(url: &str) -> bool {
    let u = url.trim();
    if u.is_empty() || u.contains(['\n', '\r', ' ']) || u.starts_with('-') {
        return false;
    }
    if let Some(rest) = u.strip_prefix("https://").or_else(|| u.strip_prefix("ssh://")) {
        return !rest.is_empty() && !rest.starts_with('/'); // must have a host
    }
    // scp-like `git@github.com:owner/repo.git` — an `@host:` before any slash, no scheme.
    if !u.contains("://") {
        if let Some((userhost, path)) = u.split_once(':') {
            return userhost.contains('@')
                && !userhost.contains('/')
                && !path.is_empty()
                && !path.starts_with('/'); // a leading '/' after ':' would be an absolute local path
        }
    }
    false
}

/// Validate a target directory path: it must be **absolute** and must not itself be, or sit directly
/// inside, an existing `.git` worktree marker in the path (a cheap nest guard; the host also checks
/// the dir is empty on disk). Returns the path unchanged on success. Pure over the string.
pub fn validate_target(path: &str) -> Result<&str, CloneError> {
    let p = path.trim();
    let absolute = p.starts_with('/') || is_windows_absolute(p);
    if p.is_empty() || !absolute || p.contains(['\n', '\r']) {
        return Err(CloneError::BadTarget);
    }
    // Refuse cloning straight into a `.git` (or a path component that is one) — that would corrupt or
    // escape an existing repo.
    if p.split(['/', '\\']).any(|seg| seg == ".git") {
        return Err(CloneError::BadTarget);
    }
    Ok(path)
}

/// `C:\…` / `\\server\share` style absolute paths.
fn is_windows_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || p.starts_with("\\\\")
}

/// A ref/branch name safe to pass as a value: no leading `-` (option injection), no whitespace or
/// control chars, no `..` or ref-special chars that could smuggle a second argument.
fn is_safe_ref(r: &str) -> bool {
    !r.is_empty()
        && !r.starts_with('-')
        && !r.contains("..")
        && r.chars().all(|c| !c.is_control() && !c.is_whitespace() && !matches!(c, '~' | '^' | ':' | '\\' | '?' | '*' | '['))
}

/// Build the hardened `git` argv (everything after the program name `git`) for `req`. This is where
/// the §C hardening is applied. `--` precedes the URL + target so neither can be read as an option,
/// even though both are separately validated. Returns an error if the URL/target/ref is unsafe.
pub fn build_clone_args(req: &CloneRequest) -> Result<Vec<String>, CloneError> {
    if !is_allowed_clone_url(&req.url) {
        return Err(CloneError::BadUrl);
    }
    validate_target(&req.target_dir)?;
    if let Some(b) = &req.branch {
        if !is_safe_ref(b) {
            return Err(CloneError::BadRef);
        }
    }
    let mut args: Vec<String> = [
        // Hardening config (before the subcommand so it applies to the whole clone).
        "-c", "core.hooksPath=",
        "-c", "protocol.ext.allow=never",
        "-c", "protocol.file.allow=never",
        "-c", "core.fsmonitor=false",
        "clone",
        "--no-tags",
        "--recurse-submodules=no",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if let Some(d) = req.depth {
        if d > 0 {
            args.push("--depth".into());
            args.push(d.to_string());
        }
    }
    if let Some(b) = &req.branch {
        args.push("--branch".into());
        args.push(b.clone());
        args.push("--single-branch".into());
    }
    args.push("--".into());
    args.push(req.url.clone());
    args.push(req.target_dir.clone());
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(url: &str, target: &str) -> CloneRequest {
        CloneRequest { url: url.into(), target_dir: target.into(), depth: None, branch: None }
    }

    #[test]
    fn only_https_and_ssh_transports_are_allowed() {
        assert!(is_allowed_clone_url("https://github.com/o/r.git"));
        assert!(is_allowed_clone_url("ssh://git@github.com/o/r.git"));
        assert!(is_allowed_clone_url("git@github.com:o/r.git")); // scp-like
        for bad in [
            "git://github.com/o/r",       // cleartext
            "http://github.com/o/r",      // not TLS
            "file:///etc/passwd",         // local
            "ext::sh -c whoami",          // arbitrary command
            "/local/path",                // bare path
            "-oProxyCommand=evil",        // option injection
            "https://",                   // no host
            "https:///no-host",           // empty host
        ] {
            assert!(!is_allowed_clone_url(bad), "{bad:?} must be refused");
        }
    }

    #[test]
    fn target_must_be_absolute_and_not_a_git_dir() {
        assert!(validate_target("/home/me/proj").is_ok());
        assert!(validate_target(r"C:\Users\me\proj").is_ok());
        for bad in ["", "relative/dir", "/home/me/proj/.git", r"C:\repo\.git\hooks", "/a\nb"] {
            assert_eq!(validate_target(bad), Err(CloneError::BadTarget), "{bad:?} should fail");
        }
    }

    #[test]
    fn build_applies_all_the_hardening_flags_in_order() {
        let args = build_clone_args(&req("https://github.com/o/r.git", "/tmp/r")).unwrap();
        let joined = args.join(" ");
        assert!(joined.contains("-c core.hooksPath="));
        assert!(joined.contains("-c protocol.ext.allow=never"));
        assert!(joined.contains("-c protocol.file.allow=never"));
        assert!(joined.contains("-c core.fsmonitor=false"));
        assert!(joined.contains("--recurse-submodules=no"));
        // URL + target come after `--`, so neither can be parsed as an option.
        let dd = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(&args[dd + 1..], &["https://github.com/o/r.git".to_string(), "/tmp/r".to_string()]);
    }

    #[test]
    fn depth_and_branch_are_wired_and_validated() {
        let r = CloneRequest {
            url: "https://github.com/o/r.git".into(),
            target_dir: "/tmp/r".into(),
            depth: Some(1),
            branch: Some("main".into()),
        };
        let args = build_clone_args(&r).unwrap();
        let j = args.join(" ");
        assert!(j.contains("--depth 1"));
        assert!(j.contains("--branch main --single-branch"));
        // A depth of 0 is ignored (full clone), not passed as `--depth 0`.
        let r0 = CloneRequest { depth: Some(0), branch: None, ..r.clone() };
        assert!(!build_clone_args(&r0).unwrap().join(" ").contains("--depth"));
        // An option-injection ref is refused.
        let bad = CloneRequest { branch: Some("--upload-pack=evil".into()), ..r };
        assert_eq!(build_clone_args(&bad), Err(CloneError::BadRef));
    }

    #[test]
    fn a_bad_url_or_target_refuses_the_whole_build() {
        assert_eq!(build_clone_args(&req("git://x/y", "/tmp/r")), Err(CloneError::BadUrl));
        assert_eq!(build_clone_args(&req("https://github.com/o/r", "rel")), Err(CloneError::BadTarget));
    }
}
