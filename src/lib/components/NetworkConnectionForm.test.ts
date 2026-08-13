/**
 * NetworkConnectionForm — scheme→auth-kind coercion guard (CPE-1688).
 *
 * CPE-1686 scoped the auth radios to the chosen protocol (s3 → access key only; everything else →
 * password/key), and re-coerces the selected kind whenever the protocol changes so a saved profile
 * can never carry an auth kind its own scheme rejects. That coercion runs in `onSchemeChange`, wired
 * as `on:change` on the `<select>` — deliberately AFTER `bind:value` has already written the new
 * scheme into `form.scheme` (see the comment above `onSchemeChange` in the component). Whether
 * `bind:value` or `on:change` runs first on a shared `change` event is a real listener-ordering
 * question, not a formality: if it ran the other way, `coerceAuthKind` would read the OLD scheme and
 * the form would settle one protocol behind. `network.test.ts` unit-tests `coerceAuthKind` itself in
 * isolation, which proves the mapping is right but nothing about when it fires. This test mounts the
 * real component and fires a genuine DOM `change` event on the real `<select>`, so it exercises the
 * two listeners in whatever order the browser (here, jsdom) actually calls them — the same mechanism
 * a future edit could silently invert.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import NetworkConnectionForm from "./NetworkConnectionForm.svelte";

// The component imports the native file-picker for the key-file Browse button; unused by this test
// (no field currently needs it — s3 form auth is a plain text input), but the import must resolve.
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

function radioState() {
  const radios = screen.getAllByRole("radio") as HTMLInputElement[];
  return {
    values: radios.map((r) => r.value),
    checked: radios.filter((r) => r.checked).map((r) => r.value),
  };
}

describe("NetworkConnectionForm — scheme-switch auth coercion (CPE-1688)", () => {
  it("sftp -> s3 -> sftp coerces the auth radios, relabels the fields, and toggles the Access key ID input", async () => {
    render(NetworkConnectionForm);

    // sftp is the blank form's default scheme: both file-server auth kinds are offered with Password
    // selected, generic field labels are shown, and there is no Access key ID input yet.
    expect(radioState()).toEqual({ values: ["password", "key"], checked: ["password"] });
    expect(screen.getByText("Host")).toBeTruthy();
    expect(screen.getByText("User")).toBeTruthy();
    expect(screen.getByText("Remote path")).toBeTruthy();
    expect(screen.queryByLabelText("Access key ID")).toBeNull();

    const scheme = screen.getByLabelText("Protocol") as HTMLSelectElement;
    await fireEvent.change(scheme, { target: { value: "s3" } });

    // S3 signs with an access key and nothing else: exactly one radio remains, and — the property
    // that actually pins the ordering — that lone radio must be the CHECKED one. If the coercion ran
    // against the previous scheme ("sftp"), `password` would still be selected here (or nothing would
    // be, since "password" isn't even offered), not "access_key".
    expect(radioState()).toEqual({ values: ["access_key"], checked: ["access_key"] });
    expect(screen.getByText("Endpoint")).toBeTruthy();
    expect(screen.getByText("Region")).toBeTruthy();
    expect(screen.getByText("Bucket and prefix")).toBeTruthy();
    expect(screen.getByLabelText("Access key ID")).toBeTruthy();

    await fireEvent.change(scheme, { target: { value: "sftp" } });

    // Switching back restores both file-server kinds with Password selected again — not stuck on
    // access_key (a kind sftp can't use at all) and not merely "some" radio checked.
    expect(radioState()).toEqual({ values: ["password", "key"], checked: ["password"] });
    expect(screen.getByText("Host")).toBeTruthy();
    expect(screen.getByText("User")).toBeTruthy();
    expect(screen.getByText("Remote path")).toBeTruthy();
    expect(screen.queryByLabelText("Access key ID")).toBeNull();
  });
});
