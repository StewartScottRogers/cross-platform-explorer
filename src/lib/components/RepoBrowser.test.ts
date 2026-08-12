/**
 * RepoBrowser render test (CPE-434/435/436/439) — browse a forge repo's tree in-app, clone it, and
 * remember the token. Uses a command-router `invoke` mock (the component also calls forge_get_token
 * on mount and forge_set/delete_token on Remember, so a sequential mock would mis-align).
 */
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/svelte";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
import RepoBrowser, { stripRepoUrl, looksLikeUrl, isRepoId } from "./RepoBrowser.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const openMock = openFolderDialog as unknown as ReturnType<typeof vi.fn>;

const root = [
  { name: "src", path: "src", is_dir: true, size: 0 },
  { name: "README.md", path: "README.md", is_dir: false, size: 1024 },
];
const srcDir = [{ name: "lib.rs", path: "src/lib.rs", is_dir: false, size: 42 }];

/** Configure the invoke router. `browse` maps a path → entries (or throws if an Error); `savedToken`
 *  is what forge_get_token returns on mount. Records forge_set_token/forge_clone calls. */
function route(opts: { browse?: (path: string) => any; clone?: any; savedToken?: string | null }) {
  const calls: { cmd: string; args: any }[] = [];
  invokeMock.mockImplementation(async (cmd: string, args: any) => {
    calls.push({ cmd, args });
    if (cmd === "forge_get_token") return opts.savedToken ?? null;
    if (cmd === "forge_set_token" || cmd === "forge_delete_token") return undefined;
    if (cmd === "forge_browse") {
      const r = opts.browse ? opts.browse(args.path ?? "") : [];
      if (r instanceof Error) throw r;
      return r;
    }
    if (cmd === "forge_clone") {
      if (opts.clone instanceof Error) throw opts.clone;
      return opts.clone ?? "ok";
    }
    return undefined;
  });
  return calls;
}

beforeEach(() => { invokeMock.mockReset(); openMock.mockReset(); });

describe("RepoBrowser", () => {
  it("browses a repo and calls forge_browse with owner/name + provider", async () => {
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider: "github", repo: "tauri-apps/tauri" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));

    await waitFor(() => expect(calls.some((c) => c.cmd === "forge_browse" && c.args.repo === "tauri-apps/tauri" && c.args.path === "")).toBe(true));
    expect(await screen.findByText("src")).toBeTruthy();
    expect(screen.getByText("README.md")).toBeTruthy();
  });

  it("navigates into a folder and back up", async () => {
    route({ browse: (p) => (p === "src" ? srcDir : root) });
    render(RepoBrowser, { props: { repo: "o/r" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    await screen.findByText("src");
    await fireEvent.click(screen.getByText("src"));
    expect(await screen.findByText("lib.rs")).toBeTruthy();
    expect(screen.getByText("..")).toBeTruthy();
  });

  it("rejects a bare name without owner/, without hitting the backend", async () => {
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { repo: "justaname" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    expect(calls.some((c) => c.cmd === "forge_browse")).toBe(false);
    expect(await screen.findByText(/owner\/name/i)).toBeTruthy();
  });

  it("clones into <chosen>/<repo-name> via forge_clone after a folder pick", async () => {
    openMock.mockResolvedValueOnce("/home/me/code");
    const calls = route({ clone: "ok" });
    render(RepoBrowser, { props: { provider: "github", repo: "tauri-apps/tauri" } });
    await fireEvent.click(screen.getByRole("button", { name: "Clone" }));

    await waitFor(() => expect(calls.some((c) => c.cmd === "forge_clone" && c.args.targetDir === "/home/me/code/tauri")).toBe(true));
    expect(await screen.findByText(/Cloned to \/home\/me\/code\/tauri/)).toBeTruthy();
  });

  it("does nothing if the folder pick is cancelled", async () => {
    openMock.mockResolvedValueOnce(null);
    const calls = route({});
    render(RepoBrowser, { props: { repo: "o/r" } });
    await fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    await waitFor(() => expect(openMock).toHaveBeenCalled());
    expect(calls.some((c) => c.cmd === "forge_clone")).toBe(false);
  });

  it("surfaces a backend error inline", async () => {
    route({ browse: () => new Error("Repo 'o/r' not found (or private — add a token).") });
    render(RepoBrowser, { props: { repo: "o/r" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    expect(await screen.findByText(/not found/i)).toBeTruthy();
  });

  it("loads a saved token on mount and remembers it (CPE-439)", async () => {
    route({ savedToken: "ghp_saved123", browse: () => root });
    render(RepoBrowser, { props: { provider: "github", repo: "o/r" } });
    // The saved token pre-fills and Remember is checked.
    const tokenInput = screen.getByPlaceholderText(/token/i) as HTMLInputElement;
    await waitFor(() => expect(tokenInput.value).toBe("ghp_saved123"));
    expect((screen.getByRole("checkbox") as HTMLInputElement).checked).toBe(true);
  });
});

// CPE-1620 — the URL-stripping regex was hardcoded to github.com, so pasting a GitLab/Bitbucket/
// Codeberg URL while that provider was selected fell through to the backend as a fake "owner/name"
// instead of getting the friendly "enter owner/name" guidance.
describe("stripRepoUrl / looksLikeUrl (CPE-1620)", () => {
  it.each([
    ["github", "https://github.com/owner/name"],
    ["gitlab", "https://gitlab.com/owner/name"],
    ["bitbucket", "https://bitbucket.org/owner/name"],
    ["codeberg", "https://codeberg.org/owner/name"],
  ])("strips the %s host from its own URL", (provider, url) => {
    expect(stripRepoUrl(url, provider)).toBe("owner/name");
    expect(looksLikeUrl(stripRepoUrl(url, provider))).toBe(false);
  });

  it("strips a trailing .git after the host strip", () => {
    expect(stripRepoUrl("https://gitlab.com/owner/name.git", "gitlab")).toBe("owner/name");
  });

  it("leaves owner/name untouched for every provider", () => {
    for (const provider of ["github", "gitlab", "bitbucket", "codeberg", "generic"]) {
      expect(stripRepoUrl("owner/name", provider)).toBe("owner/name");
    }
  });

  it("negative control: a foreign-host URL is not stripped and still looks like a URL", () => {
    // Bitbucket selected, but a GitLab URL pasted — the strip must not fire for the wrong host.
    const r = stripRepoUrl("https://gitlab.com/owner/name", "bitbucket");
    expect(r).toBe("https://gitlab.com/owner/name");
    expect(looksLikeUrl(r)).toBe(true);
  });
});

// CPE-1650 — an SCP-style SSH URL (`git@github.com:owner/repo.git`) has no `://`, so the CPE-1620
// host strip (which only matched `https?://`) never fired, and `looksLikeUrl` (which only matched
// `scheme://`) didn't flag it either — it slipped through as a fake `owner/name` and reached
// `forge_browse` with a malformed repo id. Covers the SCP-style short form, the `ssh://` long form,
// a URL with an explicit port, and a non-`git` user, per the independent review of PR #837.
describe("stripRepoUrl / looksLikeUrl — SSH repo URLs (CPE-1650)", () => {
  it.each([
    ["SCP-style short form", "git@github.com:owner/name.git"],
    ["ssh:// long form", "ssh://git@github.com/owner/name.git"],
    ["ssh:// with an explicit port", "ssh://git@github.com:22/owner/name.git"],
    ["SCP-style with a non-git user", "deploy@github.com:owner/name.git"],
    ["ssh:// with a non-git user", "ssh://deploy@github.com/owner/name.git"],
  ])("strips the host from an SSH URL — %s", (_label, url) => {
    const r = stripRepoUrl(url, "github");
    expect(r).toBe("owner/name");
    expect(looksLikeUrl(r)).toBe(false);
  });

  it("negative control: an SCP-style URL for a foreign host is not stripped and still looks like a URL", () => {
    // GitHub selected, but a GitLab SSH URL pasted — the strip must not fire for the wrong host.
    // The trailing `.git` is still stripped unconditionally (pre-existing behavior, unrelated to the
    // host match) — what must NOT happen is the host/user prefix coming off.
    const r = stripRepoUrl("git@gitlab.com:owner/name.git", "github");
    expect(r).toBe("git@gitlab.com:owner/name");
    expect(looksLikeUrl(r)).toBe(true);
  });

  it("negative control: an ssh:// URL for a foreign host is not stripped and still looks like a URL", () => {
    const r = stripRepoUrl("ssh://git@gitlab.com/owner/name.git", "github");
    expect(r).toBe("ssh://git@gitlab.com/owner/name");
    expect(looksLikeUrl(r)).toBe(true);
  });
});

describe("RepoBrowser — per-provider URL paste (CPE-1620)", () => {
  it.each([
    ["gitlab", "https://gitlab.com/owner/name"],
    ["bitbucket", "https://bitbucket.org/owner/name"],
    ["codeberg", "https://codeberg.org/owner/name"],
  ])("browses correctly after pasting a %s URL", async (provider, url) => {
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider, repo: url } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));

    await waitFor(() =>
      expect(calls.some((c) => c.cmd === "forge_browse" && c.args.repo === "owner/name")).toBe(true),
    );
    expect(await screen.findByText("src")).toBeTruthy();
  });

  it("shows the friendly owner/name guidance for a foreign-host URL instead of forwarding it", async () => {
    // GitLab selected, but a GitHub URL pasted — must not reach forge_browse with the raw URL.
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider: "gitlab", repo: "https://github.com/owner/name" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));

    expect(calls.some((c) => c.cmd === "forge_browse")).toBe(false);
    expect(await screen.findByText(/owner\/name/i)).toBeTruthy();
  });
});

// CPE-1650 — pasting an SCP-style/ssh:// SSH repo URL into a named-provider field used to bypass the
// host strip entirely and reach forge_browse as a malformed identifier (the confusing not-found
// failure CPE-1620 set out to remove, via a different input shape).
describe("RepoBrowser — SSH repo URL paste (CPE-1650)", () => {
  it.each([
    ["SCP-style short form", "git@github.com:owner/name.git"],
    ["ssh:// long form", "ssh://git@github.com/owner/name.git"],
  ])("browses correctly after pasting a matching-host %s", async (_label, url) => {
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider: "github", repo: url } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));

    await waitFor(() =>
      expect(calls.some((c) => c.cmd === "forge_browse" && c.args.repo === "owner/name")).toBe(true),
    );
    expect(await screen.findByText("src")).toBeTruthy();
  });

  it("shows the friendly owner/name guidance for a foreign-host SCP-style URL instead of forwarding it", async () => {
    // GitHub selected, but a GitLab SSH URL pasted — must never reach forge_browse with the raw string.
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider: "github", repo: "git@gitlab.com:owner/name.git" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));

    expect(calls.some((c) => c.cmd === "forge_browse")).toBe(false);
    expect(await screen.findByText(/owner\/name/i)).toBeTruthy();
  });
});

// CPE-1663 — the guard was `!r.includes("/") || looksLikeUrl(r)`, a growing list of negative special
// cases (scheme://, user@host:) that was already two exceptions deep and still let a Windows path and
// an ordinary colon-bearing sentence through — neither looks like a recognized URL shape, so both
// reached forge_browse as a fake owner/name. Fixed by tightening the *positive* test instead: isRepoId
// requires exactly one "/" and only repo-name characters (letters, digits, ., _, -) in each segment,
// which rejects everything the old guard missed — plus two more holes an independent reviewer found
// that the same predicate closes for free (a double-@ SCP string, a bare host:owner/repo with no user).
describe("isRepoId (CPE-1663)", () => {
  it.each([
    "owner/name",
    "tauri-apps/tauri",
    "owner-name/repo.name_v2",
    "a/b",
    // GitLab nested groups (PR #852 UAT). The first version of isRepoId required EXACTLY one slash and
    // silently broke these — common in real organisations, previously reachable, and supported all the
    // way through the backend: is_safe_repo_slug accepts `segs.len() >= 2`, and browse_path builds
    // GitLab's project id with `repo.replace('/', "%2F")`, which replaces every slash precisely so an
    // arbitrary depth works. A client guard stricter than its own server is a regression.
    "group/subgroup/project",
    "group/sub/deeper/project",
  ])("accepts a well-formed repo id — %s", (r) => {
    expect(isRepoId(r)).toBe(true);
  });

  it.each([
    ["a Windows path with forward slashes", "C:/repos/thing"],
    ["an ordinary sentence with a colon and a slash", "Fix: update src/main.rs docs"],
    ["a double-@ SCP-style string", "git@github.com@evil.com:o/r"],
    ["a bare host:owner/repo with no user", "github.com:owner/repo"],
    ["a scheme:// URL", "https://github.com/owner/name"],
    ["an SCP-style SSH URL", "git@github.com:owner/name"],
    ["no slash at all", "justaname"],
    ["a trailing slash / empty second segment", "owner/"],
    ["a leading slash / empty first segment", "/name"],
    ["whitespace inside a segment", "owner/na me"],
    // These three mirror is_safe_repo_slug's own rules, so the client and the server now agree on what a
    // repository id is instead of each holding a separate opinion.
    ["an empty middle segment", "group//project"],
    ["a `..` segment", "owner/../secrets"],
    ["a segment starting with a dash (reads as a flag)", "owner/-rf"],
  ])("rejects %s — %s", (_label, r) => {
    expect(isRepoId(r)).toBe(false);
  });
});

describe("RepoBrowser — non-URL junk rejected without hitting forge_browse (CPE-1663)", () => {
  it.each([
    ["a Windows path with forward slashes", "C:/repos/thing"],
    ["an ordinary sentence with a colon and a slash", "Fix: update src/main.rs docs"],
    ["a double-@ SCP-style string", "git@github.com@evil.com:o/r"],
    ["a bare host:owner/repo with no user", "github.com:owner/repo"],
  ])("rejects %s", async (_label, input) => {
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider: "github", repo: input } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    expect(calls.some((c) => c.cmd === "forge_browse")).toBe(false);
    expect(await screen.findByText(/owner\/name/i)).toBeTruthy();
  });

  // Everything CPE-1650 fixed must still work through the new positive guard.
  it.each([
    ["SCP-style short form", "git@github.com:owner/name.git"],
    ["ssh:// long form", "ssh://git@github.com/owner/name.git"],
    ["ssh:// with an explicit port", "ssh://git@github.com:22/owner/name.git"],
  ])("still browses correctly for a matching-host SSH URL — %s", async (_label, url) => {
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider: "github", repo: url } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    await waitFor(() =>
      expect(calls.some((c) => c.cmd === "forge_browse" && c.args.repo === "owner/name")).toBe(true),
    );
    expect(await screen.findByText("src")).toBeTruthy();
  });

  it("browses a GitLab nested-group project through the real component (PR #852 UAT regression)", async () => {
    // The end-to-end half of the nested-group fix: the UAT found this rejected in the UI with
    // "Enter a repository as owner/name." and forge_browse never called, even though GitLab supports
    // nested namespaces and the backend encodes every slash for exactly this case.
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider: "gitlab", repo: "group/subgroup/project" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    await waitFor(() =>
      expect(
        calls.some((c) => c.cmd === "forge_browse" && c.args.repo === "group/subgroup/project"),
      ).toBe(true),
    );
    expect(await screen.findByText("src")).toBeTruthy();
  });

  it("still rejects a foreign-host SSH URL that the selected provider's strip doesn't touch", async () => {
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider: "github", repo: "git@gitlab.com:owner/name.git" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    expect(calls.some((c) => c.cmd === "forge_browse")).toBe(false);
    expect(await screen.findByText(/owner\/name/i)).toBeTruthy();
  });

  it("still accepts a plain owner/name with dots, dashes and underscores", async () => {
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider: "github", repo: "some-org_1/repo.name_2" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    await waitFor(() =>
      expect(calls.some((c) => c.cmd === "forge_browse" && c.args.repo === "some-org_1/repo.name_2")).toBe(true),
    );
  });
});

// CPE-1668 — clone() used its own weaker guard (`!r.includes("/")`), while browse() gates on the
// isRepoId predicate CPE-1663 introduced. Clone is reachable directly (paste + click Clone, no Browse
// click required — the "does nothing if the folder pick is cancelled" test above already proves that
// path), so a pasted Windows path or a colon-bearing sentence reached forge_clone unguarded. Fixed by
// routing clone() through the same stripRepoUrl + isRepoId gate browse() uses. These tests drive the
// REAL component — type into the real repo input, click the real Clone button, read what the UI shows —
// rather than calling isRepoId directly, since a direct predicate call can't prove clone() is wired to it.
describe("RepoBrowser — Clone shares the Browse-side repo-id guard (CPE-1668)", () => {
  async function typeAndClickClone(provider: string, input: string) {
    render(RepoBrowser, { props: { provider, repo: "" } });
    const repoInput = screen.getByPlaceholderText(/owner\/name/i) as HTMLInputElement;
    await fireEvent.input(repoInput, { target: { value: input } });
    await fireEvent.click(screen.getByRole("button", { name: "Clone" }));
  }

  it.each([
    ["a Windows path with forward slashes", "C:/repos/thing"],
    ["a Windows path with backslashes", "C:\\repos\\thing"],
    ["an ordinary sentence with a colon and a slash", "Fix: update src/main.rs docs"],
    ["a double-@ SCP-style string", "git@github.com@evil.com:o/r"],
    ["a bare host:owner/repo with no user", "github.com:owner/repo"],
  ])("rejects %s without ever reaching forge_clone", async (_label, input) => {
    // A folder IS available to pick, so a guard that fails to reject `input` would sail straight
    // through to forge_clone — a folder-pick that's never offered (default unmocked `undefined`)
    // would mask a missing guard by returning early for an unrelated reason.
    openMock.mockResolvedValueOnce("/home/me/code");
    const calls = route({ clone: "ok" });
    await typeAndClickClone("github", input);
    expect(calls.some((c) => c.cmd === "forge_clone")).toBe(false);
    expect(await screen.findByText(/owner\/name/i)).toBeTruthy();
  });

  it("gives the same friendly message Browse gives for the same bad input", async () => {
    const calls = route({ browse: () => root });
    // Browse-side message for a Windows path.
    render(RepoBrowser, { props: { provider: "github", repo: "C:/repos/thing" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    const browseMsg = (await screen.findByText(/owner\/name/i)).textContent;
    expect(calls.some((c) => c.cmd === "forge_browse")).toBe(false);
    cleanup(); // tear down the Browse render before mounting a second component for Clone

    // Clone-side message for the same input, on a fresh render.
    await typeAndClickClone("github", "C:/repos/thing");
    const cloneMsg = (await screen.findByText(/owner\/name/i)).textContent;
    expect(cloneMsg).toBe(browseMsg);
  });

  // Every form CPE-1650 fixed, and a plain owner/name (incl. dots/dashes/underscores/digits), must still
  // clone through Clone directly — not just through Browse.
  it.each([
    ["SCP-style short form", "git@github.com:owner/name.git", "owner/name"],
    ["ssh:// long form", "ssh://git@github.com/owner/name.git", "owner/name"],
    ["ssh:// with an explicit port", "ssh://git@github.com:22/owner/name.git", "owner/name"],
    ["ssh:// without .git", "ssh://git@github.com/owner/name", "owner/name"],
    ["SCP-style without .git", "git@github.com:owner/name", "owner/name"],
    ["plain owner/name", "owner/name", "owner/name"],
    [
      "plain owner/name with dots, dashes, underscores, digits",
      "some-org_1/repo.name_2",
      "some-org_1/repo.name_2",
    ],
  ])("still clones — %s", async (_label, input, expectedRepo) => {
    openMock.mockResolvedValueOnce("/home/me/code");
    const calls = route({ clone: "ok" });
    await typeAndClickClone("github", input);
    await waitFor(() => expect(calls.some((c) => c.cmd === "forge_clone")).toBe(true));
    const cloneCall = calls.find((c) => c.cmd === "forge_clone")!;
    // The stripped/validated owner/name form is what reaches forge_clone, matching what browse() sends.
    expect(cloneCall.args.repo).toBe(expectedRepo);
    expect(await screen.findByText(new RegExp(`Cloned to`))).toBeTruthy();
  });

  it("a GitLab nested group still clones through Clone directly", async () => {
    openMock.mockResolvedValueOnce("/home/me/code");
    const calls = route({ clone: "ok" });
    await typeAndClickClone("gitlab", "group/subgroup/project");
    await waitFor(() => expect(calls.some((c) => c.cmd === "forge_clone")).toBe(true));
    const cloneCall = calls.find((c) => c.cmd === "forge_clone")!;
    expect(cloneCall.args.repo).toBe("group/subgroup/project");
  });

  it("still rejects a foreign-host SSH URL the selected provider's strip doesn't touch", async () => {
    const calls = route({});
    await typeAndClickClone("github", "git@gitlab.com:owner/name.git");
    expect(calls.some((c) => c.cmd === "forge_clone")).toBe(false);
    expect(await screen.findByText(/owner\/name/i)).toBeTruthy();
  });
});
