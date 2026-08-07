// Pure, DOM/Tauri-free helpers for the certificate management dialogs (CPE-1423/1424, epic CPE-1417):
// CreateCertDialog.svelte (Create certificate…) and SignCertDialog.svelte (Sign / issue from CSR…). Both
// dialogs own their own backend call + form state; these functions are just the filename-default /
// path-join / create-enabled logic, so they're unit-testable without a webview (certCreate.test.ts) —
// same split as vaultCreate.ts for VaultCreateDialog.

/** Split a path's separator-preserving parent/base (POSIX `/` or Windows `\`), mirroring
 *  `vaultCreate.ts`'s `splitPath` so a computed default matches the folder's own convention. */
function splitPath(path: string): { parent: string; base: string; sep: string } {
  const idx = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (idx < 0) return { parent: "", base: path, sep: "/" };
  return { parent: path.slice(0, idx), base: path.slice(idx + 1), sep: path[idx] };
}

/** The filename (last path segment) of `path`, separator-agnostic. */
export function fileBaseName(path: string): string {
  return splitPath(path).base;
}

/** `name` without its final extension (`"req.csr"` → `"req"`); a leading dot (dotfile) is preserved
 *  as-is rather than treated as "no basename". */
export function stripExt(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(0, i) : name;
}

/** Join a folder + filename using the folder's own separator convention (Windows `\` when the folder
 *  already contains one and no `/`, POSIX `/` otherwise). Empty `dir` returns `file` unchanged. */
export function joinPath(dir: string, file: string): string {
  if (!dir) return file;
  const sep = dir.includes("\\") && !dir.includes("/") ? "\\" : "/";
  return dir.replace(/[\\/]+$/, "") + sep + file;
}

/** Turn a Common Name (or any free-text label) into a filesystem-safe filename base: path separators,
 *  reserved Windows characters, and whitespace runs collapse to `-`; an empty/whitespace-only name falls
 *  back to `"certificate"` so the filename fields are never blank. */
export function sanitizeFileBase(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) return "certificate";
  const safe = trimmed.replace(/[\\/:*?"<>|]+/g, "-").replace(/\s+/g, "-");
  return safe || "certificate";
}

/** Whether CreateCertDialog's "Create" button should be enabled: a non-empty CN, output folder, and
 *  both filenames, with no create already in flight. Pure — drives the disabled state + is unit-tested. */
export function canCreateCert(state: {
  commonName: string;
  folder: string;
  certFileName: string;
  keyFileName: string;
  busy: boolean;
}): boolean {
  return (
    !state.busy &&
    state.commonName.trim().length > 0 &&
    state.folder.trim().length > 0 &&
    state.certFileName.trim().length > 0 &&
    state.keyFileName.trim().length > 0
  );
}

/** The default issued-certificate filename for SignCertDialog, derived from the CSR's own basename
 *  (`"service.csr"` → `"service.crt"`) when one is known, else a generic fallback. */
export function defaultIssuedCertName(csrPath: string): string {
  if (!csrPath) return "issued-cert.pem";
  return `${stripExt(fileBaseName(csrPath))}.crt`;
}

/** Whether SignCertDialog's "Issue certificate" button should be enabled: every path field set, a
 *  positive validity, and no issue already in flight. Pure — drives the disabled state + is unit-tested. */
export function canSignCert(state: {
  csrPath: string;
  caCertPath: string;
  caKeyPath: string;
  outCertPath: string;
  validityDays: number;
  busy: boolean;
}): boolean {
  return (
    !state.busy &&
    state.csrPath.trim().length > 0 &&
    state.caCertPath.trim().length > 0 &&
    state.caKeyPath.trim().length > 0 &&
    state.outCertPath.trim().length > 0 &&
    state.validityDays >= 1
  );
}
