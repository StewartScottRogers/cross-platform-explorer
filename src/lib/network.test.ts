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

  describe("hasAnyNetworkRows", () => {
    it("is false when both tiers are empty", () => {
      expect(hasAnyNetworkRows([], [])).toBe(false);
    });
    it("is true when either tier has rows", () => {
      expect(hasAnyNetworkRows([conn()], [])).toBe(true);
      expect(hasAnyNetworkRows([], [{ name: "a", path: "a", kind: "mapped" }])).toBe(true);
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
      expect(buildConnection(input({ scheme: "smb" }))).toMatch(/Unsupported protocol/);
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
});
