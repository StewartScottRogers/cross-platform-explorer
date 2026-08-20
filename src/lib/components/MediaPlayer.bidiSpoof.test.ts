/**
 * CPE-1760 — a filename reaches MediaPlayer's `aria-label`s raw through a prop.
 *
 * `PreviewPane.svelte` and `MediaQuickLook.svelte` both pass a filesystem entry's `name` straight into
 * `<MediaPlayer name={...}>` as a plain prop — a legitimate shape the render guard (`bidiRenderScan.ts`)
 * cannot see, by its own documented boundary: "a component prop pass-through whose LEAF doesn't escape
 * its own render is invisible here by design". Whether that prop reaches a screen reader raw depends
 * entirely on what the leaf does with it, so the leaf itself must escape — and `MediaPlayer.svelte`
 * does, via `displaySafeName(name)` on both the `<audio>` and `<video>` elements' `aria-label`.
 *
 * This test renders the REAL component (not the `displaySafeName` unit in isolation) and asserts on the
 * rendered `aria-label` ATTRIBUTE for both media types, per Evidence Rule "assert what the user/AT
 * actually gets" — the same convention `FileList.bidiSpoof.test.ts` etc. already establish for this
 * class of bug. Breaking `MediaPlayer.svelte`'s escape (reverting `displaySafeName(name)` back to bare
 * `name`) reds this test directly on the rendered attribute; it does not merely re-assert
 * `displaySafeName`'s own return value, which `filename.test.ts` already covers on its own.
 */
import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import MediaPlayer from "./MediaPlayer.svelte";

// Built from a decimal code point, not a literal character, so this test file itself stays plain ASCII
// and immune to the exact hazard it is proving a fix for (see filename.ts's own doc comment).
const RLO = String.fromCharCode(0x202e);

describe("MediaPlayer does not leak a raw bidi-spoofed name into aria-label (CPE-1760)", () => {
  it("escapes the spoofed name in the <video> element's aria-label, and leaves src raw", () => {
    const spoofed = `${RLO}gnp.mp4`; // reads as "mp4.png" if a bidi renderer draws it raw
    const { container } = render(MediaPlayer, { src: "asset://spoofed-path", type: "video", name: spoofed });
    const video = container.querySelector("video.mp-media") as HTMLVideoElement;
    expect(video.getAttribute("aria-label")).toBe("[RLO]gnp.mp4");
    // The failure mode this guards: the raw override character never reaches the accessibility tree.
    expect(video.getAttribute("aria-label")).not.toBe(spoofed);
    // The fetch needs the real bytes — src is deliberately left untouched (same as QuickLook's <img src>).
    expect(video.getAttribute("src")).toBe("asset://spoofed-path");
  });

  it("escapes the spoofed name in the <audio> element's aria-label", () => {
    const spoofed = `${RLO}gnp.mp3`;
    const { container } = render(MediaPlayer, { src: "asset://spoofed-audio", type: "audio", name: spoofed });
    const audio = container.querySelector("audio.mp-media") as HTMLAudioElement;
    expect(audio.getAttribute("aria-label")).toBe("[RLO]gnp.mp3");
    expect(audio.getAttribute("aria-label")).not.toBe(spoofed);
  });

  it("still shows a real Arabic name and a real Hebrew name unchanged, on both element types", () => {
    // "تقرير.mp4" — "report.mp4" — a real Arabic filename with no bidi control characters.
    const arabic = "تقرير.mp4";
    // "מוזיקה.mp3" — "music.mp3" — a real Hebrew filename with no bidi control characters.
    const hebrew = "מוזיקה.mp3";

    const { container: videoContainer } = render(MediaPlayer, { src: "asset://a", type: "video", name: arabic });
    expect((videoContainer.querySelector("video.mp-media") as HTMLVideoElement).getAttribute("aria-label")).toBe(
      arabic,
    );

    const { container: audioContainer } = render(MediaPlayer, { src: "asset://h", type: "audio", name: hebrew });
    expect((audioContainer.querySelector("audio.mp-media") as HTMLAudioElement).getAttribute("aria-label")).toBe(
      hebrew,
    );
  });
});
