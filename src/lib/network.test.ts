import { describe, it, expect } from "vitest";
import {
  connectionLocation,
  secretAlwaysRequired,
  stateOf,
  stateTitle,
  isDuplicateShare,
  dedupeShares,
  hasAnyNetworkRows,
  blankConnectionForm,
  formFromConnection,
  buildConnection,
  isSavableScheme,
  discoveredShareToFormInput,
  mergeDiscovered,
  type ConnectionFormInput,
} from "./network";
import type { Connection, NetShare } from "./types";

function conn(over: Partial<Connection> = {}): Connection {
  return {
    name: "prod",
    scheme: "sftp",
    host: "host.example.com",
    port: 22,
    user: "deploy",
    auth: { kind: "password" },
    ...over,
  };
}

describe("network (CPE-1513)", () => {
  describe("connectionLocation", () => {
    it("omits the port when it matches the scheme default", () => {
      expect(connectionLocation(conn())).toBe("sftp://deploy@host.example.com/");
    });

    it("includes a non-default port", () => {
      expect(connectionLocation(conn({ port: 2222 }))).toBe("sftp://deploy@host.example.com:2222/");
    });

    it("uses the connection's path when set", () => {
      expect(connectionLocation(conn({ path: "/var/www" }))).toBe("sftp://deploy@host.example.com/var/www");
    });

    it("defaults webdav's default port (80) to omitted, davs (443) too", () => {
      expect(connectionLocation(conn({ scheme: "webdav", port: 80 }))).toBe("webdav://deploy@host.example.com/");
      expect(connectionLocation(conn({ scheme: "davs", port: 443 }))).toBe("davs://deploy@host.example.com/");
    });
  });

  describe("secretAlwaysRequired", () => {
    it("is true for password auth", () => {
      expect(secretAlwaysRequired({ kind: "password" })).toBe(true);
    });
    it("is false for key auth (a key may be unencrypted)", () => {
      expect(secretAlwaysRequired({ kind: "key", key_path: "/home/me/.ssh/id_ed25519" })).toBe(false);
    });
  });

  describe("stateOf / stateTitle", () => {
    it("defaults an untracked connection to disconnected", () => {
      expect(stateOf({}, "prod")).toBe("disconnected");
      expect(stateOf({ prod: "connected" }, "prod")).toBe("connected");
      expect(stateOf({ prod: "error" }, "staging")).toBe("disconnected");
    });

    it("titles each state, including the error detail when present", () => {
      expect(stateTitle("connected")).toBe("Connected");
      expect(stateTitle("disconnected")).toBe("Saved — not connected");
      expect(stateTitle("error")).toBe("Connection error");
      expect(stateTitle("error", "host key changed")).toBe("Connection error: host key changed");
    });
  });

  describe("isDuplicateShare / dedupeShares", () => {
    const connections = [conn({ name: "prod", host: "files.example.com" })];

    it("flags a share whose name or path contains a saved connection's host", () => {
      const share: NetShare = { name: "files.example.com (Z:)", path: "Z:\\", kind: "mapped" };
      expect(isDuplicateShare(share, connections)).toBe(true);
    });

    it("is case-insensitive", () => {
      const share: NetShare = { name: "FILES.EXAMPLE.COM", path: "Z:\\", kind: "mapped" };
      expect(isDuplicateShare(share, connections)).toBe(true);
    });

    it("does not flag an unrelated share", () => {
      const share: NetShare = { name: "backups (Y:)", path: "Y:\\", kind: "mapped" };
      expect(isDuplicateShare(share, connections)).toBe(false);
    });

    it("a connection with a blank host never matches (defensive, shouldn't occur post-validation)", () => {
      const share: NetShare = { name: "anything", path: "anything", kind: "mapped" };
      expect(isDuplicateShare(share, [conn({ host: "" })])).toBe(false);
    });

    it("dedupeShares filters only the duplicates, preserving order", () => {
      const shares: NetShare[] = [
        { name: "backups (Y:)", path: "Y:\\", kind: "mapped" },
        { name: "files.example.com (Z:)", path: "Z:\\", kind: "mapped" },
        { name: "archive (X:)", path: "X:\\", kind: "mapped" },
      ];
      expect(dedupeShares(shares, connections)).toEqual([shares[0], shares[2]]);
    });
  });

  describe("dedupe across three tiers (CPE-1519: tier 3 'discovered' vs tier 1 connections + tier 2 shares)", () => {
    const connections = [conn({ name: "prod", host: "files.example.com" })];
    const tier2: NetShare[] = [
      // A mapped drive whose name embeds the same UNC a WNet discovery would find for that server.
      { name: "\\\\qnap\\media (Z:)", path: "Z:\\", kind: "mapped" },
      { name: "backups (Y:)", path: "Y:\\", kind: "mapped" },
    ];

    it("flags a discovered share whose UNC is already mapped (tier 2)", () => {
      const discovered: NetShare = { name: "\\\\qnap\\media", path: "\\\\qnap\\media", kind: "discovered" };
      expect(isDuplicateShare(discovered, connections, tier2)).toBe(true);
    });

    it("is case- and trailing-slash-insensitive against tier 2", () => {
      const discovered: NetShare = { name: "Media share", path: "\\\\QNAP\\MEDIA\\", kind: "discovered" };
      expect(isDuplicateShare(discovered, connections, tier2)).toBe(true);
    });

    it("still flags a discovered share whose host matches a saved connection (tier 1)", () => {
      const discovered: NetShare = { name: "\\\\files.example.com\\docs", path: "\\\\files.example.com\\docs", kind: "discovered" };
      expect(isDuplicateShare(discovered, connections, tier2)).toBe(true);
    });

    it("does not flag a genuinely new discovered share", () => {
      const discovered: NetShare = { name: "\\\\nas2\\photos", path: "\\\\nas2\\photos", kind: "discovered" };
      expect(isDuplicateShare(discovered, connections, tier2)).toBe(false);
    });

    it("dedupeShares(discovered, connections, tier2) drops both kinds of duplicate, keeps the rest, preserves order", () => {
      const discovered: NetShare[] = [
        { name: "\\\\nas2\\photos", path: "\\\\nas2\\photos", kind: "discovered" }, // new
        { name: "\\\\qnap\\media", path: "\\\\qnap\\media", kind: "discovered" }, // dup of tier 2
        { name: "\\\\files.example.com\\docs", path: "\\\\files.example.com\\docs", kind: "discovered" }, // dup of tier 1
        { name: "\\\\nas3\\vault", path: "\\\\nas3\\vault", kind: "discovered" }, // new
      ];
      expect(dedupeShares(discovered, connections, tier2)).toEqual([discovered[0], discovered[3]]);
    });

    it("the 2-arg call (tier 2 vs tier 1) is unaffected — existingShares defaults to empty", () => {
      const shares: NetShare[] = [{ name: "\\\\qnap\\media (Z:)", path: "Z:\\", kind: "mapped" }];
      expect(dedupeShares(shares, connections)).toEqual(shares);
    });
  });

  describe("hasAnyNetworkRows", () => {
    it("is false when both tiers are empty", () => {
      expect(hasAnyNetworkRows([], [])).toBe(false);
    });
    it("is true when either tier has rows", () => {
      expect(hasAnyNetworkRows([conn()], [])).toBe(true);
      expect(hasAnyNetworkRows([], [{ name: "a", path: "a", kind: "mapped" }])).toBe(true);
    });

    it("is true when only the discovered tier (CPE-1519) has rows", () => {
      expect(hasAnyNetworkRows([], [], [{ name: "a", path: "\\\\a\\b", kind: "discovered" }])).toBe(true);
    });

    it("is false when all three tiers are empty", () => {
      expect(hasAnyNetworkRows([], [], [])).toBe(false);
    });
  });

  describe("blankConnectionForm / formFromConnection", () => {
    it("blank form defaults to sftp + password", () => {
      const f = blankConnectionForm();
      expect(f.scheme).toBe("sftp");
      expect(f.authKind).toBe("password");
      expect(f.name).toBe("");
    });

    it("formFromConnection round-trips a password connection", () => {
      const f = formFromConnection(conn({ path: "/srv" }));
      expect(f).toEqual({
        name: "prod",
        scheme: "sftp",
        host: "host.example.com",
        port: "22",
        user: "deploy",
        authKind: "password",
        keyPath: "",
        path: "/srv",
      });
    });

    it("formFromConnection carries the key path for key auth", () => {
      const f = formFromConnection(conn({ auth: { kind: "key", key_path: "/home/me/.ssh/id_ed25519" } }));
      expect(f.authKind).toBe("key");
      expect(f.keyPath).toBe("/home/me/.ssh/id_ed25519");
    });
  });

  describe("buildConnection", () => {
    function input(over: Partial<ConnectionFormInput> = {}): ConnectionFormInput {
      return { ...blankConnectionForm(), name: "prod", host: "host.example.com", ...over };
    }

    it("builds a valid password connection with default port", () => {
      const c = buildConnection(input());
      expect(c).toEqual({
        name: "prod",
        scheme: "sftp",
        host: "host.example.com",
        port: 22,
        user: "",
        auth: { kind: "password" },
        path: undefined,
      });
    });

    it("trims fields and parses a custom port", () => {
      const c = buildConnection(input({ name: " prod ", host: " host.example.com ", port: "2222", user: " deploy " }));
      expect(c).toMatchObject({ name: "prod", host: "host.example.com", port: 2222, user: "deploy" });
    });

    it("rejects a blank name", () => {
      expect(buildConnection(input({ name: "  " }))).toBe("Give the connection a name.");
    });

    it("rejects a blank host", () => {
      expect(buildConnection(input({ host: "  " }))).toBe("Host is required.");
    });

    it("rejects an unsupported scheme", () => {
      expect(buildConnection(input({ scheme: "s3" }))).toMatch(/Unsupported protocol/);
    });

    it("accepts smb (CPE-1519: a discovered share's pre-filled scheme)", () => {
      const c = buildConnection(input({ name: "qnap-media", scheme: "smb", path: "/media" }));
      expect(c).toEqual({
        name: "qnap-media",
        scheme: "smb",
        host: "host.example.com",
        port: 445,
        user: "",
        auth: { kind: "password" },
        path: "/media",
      });
    });

    it("accepts ftp (CPE-1523: cpe-ftp ships, so a discovered _ftp._tcp mDNS row must validate)", () => {
      const c = buildConnection(input({ name: "nas-ftp", scheme: "ftp" }));
      expect(c).toEqual({
        name: "nas-ftp",
        scheme: "ftp",
        host: "host.example.com",
        port: 21,
        user: "",
        auth: { kind: "password" },
        path: undefined,
      });
    });

    it("rejects an out-of-range port", () => {
      expect(buildConnection(input({ port: "0" }))).toMatch(/Port must be/);
      expect(buildConnection(input({ port: "70000" }))).toMatch(/Port must be/);
      expect(buildConnection(input({ port: "abc" }))).toMatch(/Port must be/);
    });

    it("requires a key path for key auth", () => {
      expect(buildConnection(input({ authKind: "key", keyPath: "" }))).toBe(
        "Key file path is required for key auth.",
      );
    });

    it("builds a valid key connection", () => {
      const c = buildConnection(input({ authKind: "key", keyPath: "/home/me/.ssh/id_ed25519", path: "/srv" }));
      expect(c).toEqual({
        name: "prod",
        scheme: "sftp",
        host: "host.example.com",
        port: 22,
        user: "",
        auth: { kind: "key", key_path: "/home/me/.ssh/id_ed25519" },
        path: "/srv",
      });
    });

    it("editing (same name) is allowed — upsert-by-name IS how Edit works", () => {
      const c = buildConnection(input({ name: "prod", host: "new-host.example.com" }));
      expect(c).toMatchObject({ name: "prod", host: "new-host.example.com" });
    });
  });

  describe("isSavableScheme (CPE-1524: gate the discovered-row ＋Add affordance on unsavable schemes)", () => {
    it("flags nfs as not savable — mDNS browses _nfs._tcp but no NFS provider exists yet", () => {
      expect(isSavableScheme("nfs")).toBe(false);
    });

    it("flags every SUPPORTED_SCHEMES entry as savable", () => {
      expect(isSavableScheme("sftp")).toBe(true);
      expect(isSavableScheme("webdav")).toBe(true);
      expect(isSavableScheme("ftp")).toBe(true);
      expect(isSavableScheme("smb")).toBe(true);
    });

    it("is case/whitespace-tolerant, matching a raw discovered-row scheme", () => {
      expect(isSavableScheme(" SFTP ")).toBe(true);
      expect(isSavableScheme("NFS")).toBe(false);
    });
  });

  describe("discoveredShareToFormInput (CPE-1519: discovered row → pre-filled 'Add a connection' form)", () => {
    it("maps a server+share UNC to scheme smb, host = server, path = /share", () => {
      const share: NetShare = { name: "Media", path: "\\\\qnap\\media", kind: "discovered" };
      const f = discoveredShareToFormInput(share);
      expect(f).toEqual({
        name: "qnap-media",
        scheme: "smb",
        host: "qnap",
        port: "",
        user: "",
        authKind: "password",
        keyPath: "",
        path: "/media",
      });
    });

    it("the pre-filled form builds a valid connection one click later", () => {
      const share: NetShare = { name: "Media", path: "\\\\qnap\\media", kind: "discovered" };
      const c = buildConnection(discoveredShareToFormInput(share));
      expect(c).toMatchObject({ name: "qnap-media", scheme: "smb", host: "qnap", path: "/media" });
    });

    it("handles a nested sub-path UNC, keeping only the first segment as the share", () => {
      const share: NetShare = { name: "Media", path: "\\\\qnap\\media\\movies", kind: "discovered" };
      expect(discoveredShareToFormInput(share)).toMatchObject({ host: "qnap", path: "/media" });
    });

    it("a server-only UNC (no share) still pre-fills the host, with a blank path", () => {
      const share: NetShare = { name: "\\\\qnap", path: "\\\\qnap", kind: "discovered" };
      expect(discoveredShareToFormInput(share)).toMatchObject({ host: "qnap", path: "", name: "qnap" });
    });

    it("falls back to a blank smb form for a non-UNC path (defensive; shouldn't occur post-backend-filter)", () => {
      const share: NetShare = { name: "junk", path: "not-a-unc-path", kind: "discovered" };
      expect(discoveredShareToFormInput(share)).toEqual({ ...blankConnectionForm(), scheme: "smb" });
    });

    // ── CPE-1523: mDNS `scheme://host[:port]` rows (sftp/webdav/davs/ftp/nfs — not UNC) ──────────────

    it("maps an mDNS sftp row (no port — default omitted by the backend) to scheme+host, name from the row", () => {
      const share: NetShare = { name: "Office QNAP", path: "sftp://qnap.local", kind: "discovered" };
      expect(discoveredShareToFormInput(share)).toEqual({
        name: "Office QNAP",
        scheme: "sftp",
        host: "qnap.local",
        port: "",
        user: "",
        authKind: "password",
        keyPath: "",
        path: "",
      });
    });

    it("carries a non-default port through from an mDNS row", () => {
      const share: NetShare = { name: "nas.local", path: "sftp://nas.local:2222", kind: "discovered" };
      expect(discoveredShareToFormInput(share)).toMatchObject({ host: "nas.local", port: "2222" });
    });

    it("maps an mDNS webdav/davs row", () => {
      expect(discoveredShareToFormInput({ name: "nas", path: "webdav://nas.local", kind: "discovered" })).toMatchObject(
        { scheme: "webdav", host: "nas.local" },
      );
      expect(discoveredShareToFormInput({ name: "nas", path: "davs://nas.local", kind: "discovered" })).toMatchObject({
        scheme: "davs",
        host: "nas.local",
      });
    });

    it("maps an mDNS ftp row and it builds a valid connection one click later (CPE-1523: cpe-ftp ships)", () => {
      const share: NetShare = { name: "nas.local", path: "ftp://nas.local", kind: "discovered" };
      const c = buildConnection(discoveredShareToFormInput(share));
      expect(c).toMatchObject({ scheme: "ftp", host: "nas.local", port: 21 });
    });

    it("falls back to the host when the row has a blank name", () => {
      const share: NetShare = { name: "   ", path: "sftp://qnap.local", kind: "discovered" };
      expect(discoveredShareToFormInput(share)).toMatchObject({ name: "qnap.local" });
    });
  });

  describe("mergeDiscovered (CPE-1523: WNet tier + mDNS tier → one deduplicated 'Discovered' list)", () => {
    it("concatenates both tiers, WNet first, when there's no overlap", () => {
      const windows: NetShare[] = [{ name: "\\\\qnap\\media", path: "\\\\qnap\\media", kind: "discovered" }];
      const mdns: NetShare[] = [{ name: "nas.local", path: "sftp://nas.local", kind: "discovered" }];
      expect(mergeDiscovered(windows, mdns)).toEqual([...windows, ...mdns]);
    });

    it("drops an mDNS row that duplicates a WNet row's path, keeping the WNet (UNC) one", () => {
      const windows: NetShare[] = [{ name: "\\\\QNAP\\media", path: "\\\\QNAP\\media", kind: "discovered" }];
      // Same path, different case/trailing-slash — still a duplicate by `shareDedupKey`'s rules.
      const mdns: NetShare[] = [{ name: "qnap dup", path: "\\\\qnap\\media\\", kind: "discovered" }];
      const merged = mergeDiscovered(windows, mdns);
      expect(merged).toHaveLength(1);
      expect(merged[0]).toEqual(windows[0]);
    });

    it("dedupes within a single tier too, keeping the first occurrence", () => {
      const mdns: NetShare[] = [
        { name: "first", path: "sftp://nas.local", kind: "discovered" },
        { name: "second (dup)", path: "sftp://nas.local", kind: "discovered" },
      ];
      expect(mergeDiscovered([], mdns)).toEqual([mdns[0]]);
    });

    it("both tiers empty is empty, not a hang", () => {
      expect(mergeDiscovered([], [])).toEqual([]);
    });
  });
});
