// Pure certificate-management dialog logic (CPE-1423/1424, epic CPE-1417): filename sanitization,
// separator-preserving path join, the default issued-cert name, and both dialogs' create/sign-enabled
// gates. DOM/Tauri-free, mirroring vaultCreate.test.ts's split.
import { describe, it, expect } from "vitest";
import {
  fileBaseName,
  stripExt,
  joinPath,
  sanitizeFileBase,
  canCreateCert,
  defaultIssuedCertName,
  canSignCert,
} from "./certCreate";

describe("fileBaseName / stripExt", () => {
  it("extracts the last path segment, POSIX or Windows", () => {
    expect(fileBaseName("/a/b/req.csr")).toBe("req.csr");
    expect(fileBaseName("C:\\a\\b\\req.csr")).toBe("req.csr");
    expect(fileBaseName("bare.csr")).toBe("bare.csr");
  });

  it("strips the final extension only", () => {
    expect(stripExt("req.csr")).toBe("req");
    expect(stripExt("archive.tar.gz")).toBe("archive.tar");
    expect(stripExt("noext")).toBe("noext");
    expect(stripExt(".dotfile")).toBe(".dotfile"); // leading dot is not treated as "no basename"
  });
});

describe("joinPath — separator-preserving", () => {
  it("uses / for a POSIX folder", () => {
    expect(joinPath("/out", "svc.pem")).toBe("/out/svc.pem");
  });

  it("uses \\ for a Windows folder", () => {
    expect(joinPath("C:\\Users\\me\\out", "svc.pem")).toBe("C:\\Users\\me\\out\\svc.pem");
  });

  it("tolerates a trailing separator", () => {
    expect(joinPath("/out/", "svc.pem")).toBe("/out/svc.pem");
    expect(joinPath("C:\\out\\", "svc.pem")).toBe("C:\\out\\svc.pem");
  });

  it("returns the filename unchanged when dir is empty", () => {
    expect(joinPath("", "svc.pem")).toBe("svc.pem");
  });
});

describe("sanitizeFileBase", () => {
  it("passes a plain name through", () => {
    expect(sanitizeFileBase("svc.local")).toBe("svc.local");
  });

  it("collapses whitespace and strips reserved characters", () => {
    expect(sanitizeFileBase("My  Service: prod?")).toBe("My-Service--prod-");
  });

  it("falls back to 'certificate' for empty/whitespace-only input", () => {
    expect(sanitizeFileBase("")).toBe("certificate");
    expect(sanitizeFileBase("   ")).toBe("certificate");
  });
});

describe("canCreateCert — the Create-button gate", () => {
  const ok = { commonName: "svc.local", folder: "/out", certFileName: "svc.pem", keyFileName: "svc.key", busy: false };

  it("enabled once CN + folder + both filenames are set", () => {
    expect(canCreateCert(ok)).toBe(true);
  });

  it("disabled while busy", () => {
    expect(canCreateCert({ ...ok, busy: true })).toBe(false);
  });

  it("disabled with no common name", () => {
    expect(canCreateCert({ ...ok, commonName: "  " })).toBe(false);
  });

  it("disabled with no output folder", () => {
    expect(canCreateCert({ ...ok, folder: "" })).toBe(false);
  });

  it("disabled with an empty cert or key filename", () => {
    expect(canCreateCert({ ...ok, certFileName: "" })).toBe(false);
    expect(canCreateCert({ ...ok, keyFileName: "" })).toBe(false);
  });
});

describe("defaultIssuedCertName", () => {
  it("derives <basename>.crt from the CSR path", () => {
    expect(defaultIssuedCertName("/a/b/service.csr")).toBe("service.crt");
    expect(defaultIssuedCertName("C:\\a\\service.csr")).toBe("service.crt");
  });

  it("falls back to a generic name when no CSR is known yet", () => {
    expect(defaultIssuedCertName("")).toBe("issued-cert.pem");
  });
});

describe("canSignCert — the Issue-button gate", () => {
  const ok = {
    csrPath: "/a/req.csr",
    caCertPath: "/a/ca.pem",
    caKeyPath: "/a/ca.key",
    outCertPath: "/out/issued.crt",
    validityDays: 365,
    busy: false,
  };

  it("enabled once every path field + a positive validity are set", () => {
    expect(canSignCert(ok)).toBe(true);
  });

  it("disabled while busy", () => {
    expect(canSignCert({ ...ok, busy: true })).toBe(false);
  });

  it("disabled with any path field missing", () => {
    expect(canSignCert({ ...ok, csrPath: "" })).toBe(false);
    expect(canSignCert({ ...ok, caCertPath: "" })).toBe(false);
    expect(canSignCert({ ...ok, caKeyPath: "" })).toBe(false);
    expect(canSignCert({ ...ok, outCertPath: "" })).toBe(false);
  });

  it("disabled with a non-positive validity", () => {
    expect(canSignCert({ ...ok, validityDays: 0 })).toBe(false);
  });
});
