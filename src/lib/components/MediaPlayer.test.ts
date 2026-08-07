import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import MediaPlayer from "./MediaPlayer.svelte";

// jsdom doesn't implement HTMLMediaElement.play/pause; the component guards those calls, but stub them
// so nothing logs "Not implemented" noise during the play/pause spec.
Object.defineProperty(HTMLMediaElement.prototype, "play", {
  configurable: true,
  value: vi.fn(() => Promise.resolve()),
});
Object.defineProperty(HTMLMediaElement.prototype, "pause", {
  configurable: true,
  value: vi.fn(),
});

const q = (c: Element | Document, sel: string) => c.querySelector(sel) as HTMLElement | null;

describe("MediaPlayer transport (CPE-1429)", () => {
  it("renders an <audio> element (no raw controls) plus the full custom transport for audio", () => {
    const { container } = render(MediaPlayer, { src: "asset://a.mp3", type: "audio", name: "a.mp3" });
    const audio = container.querySelector("audio.mp-media") as HTMLAudioElement | null;
    expect(audio).toBeTruthy();
    expect(audio!.getAttribute("src")).toBe("asset://a.mp3");
    expect(audio!.hasAttribute("controls")).toBe(false);
    // Every transport control is present.
    for (const id of ["mp-playpause", "mp-scrub", "mp-vol", "mp-mute", "mp-rate", "mp-loop"]) {
      expect(q(container, `[data-testid="${id}"]`), id).toBeTruthy();
    }
  });

  it("renders a <video> element for a video source", () => {
    const { container } = render(MediaPlayer, { src: "asset://a.mp4", type: "video", name: "a.mp4" });
    expect(container.querySelector("video.mp-media")).toBeTruthy();
    expect(container.querySelector("audio.mp-media")).toBeNull();
  });

  it("play/pause button toggles the transport state and its label", async () => {
    const { getByTestId } = render(MediaPlayer, { src: "asset://a.mp3", type: "audio" });
    const btn = getByTestId("mp-playpause");
    expect(btn.getAttribute("aria-label")).toBe("Play");
    expect(btn.getAttribute("aria-pressed")).toBe("false");
    await fireEvent.click(btn);
    expect(btn.getAttribute("aria-label")).toBe("Pause");
    expect(btn.getAttribute("aria-pressed")).toBe("true");
    await fireEvent.click(btn);
    expect(btn.getAttribute("aria-label")).toBe("Play");
  });

  it("the scrub bar seeks — moving it updates the displayed current time and the element", async () => {
    const { container, getByTestId } = render(MediaPlayer, { src: "asset://a.mp4", type: "video" });
    const media = container.querySelector("video.mp-media") as HTMLMediaElement;
    // Feed a known duration through the element's event so the scrub bar becomes seekable.
    Object.defineProperty(media, "duration", { configurable: true, value: 120 });
    let elementSeekedTo = 0;
    Object.defineProperty(media, "currentTime", {
      configurable: true,
      get: () => elementSeekedTo,
      set: (v: number) => (elementSeekedTo = v),
    });
    await fireEvent(media, new Event("loadedmetadata"));

    const scrub = getByTestId("mp-scrub") as HTMLInputElement;
    expect(scrub.disabled).toBe(false);
    expect(scrub.max).toBe("120");
    expect(getByTestId("mp-duration").textContent).toBe("2:00");

    await fireEvent.input(scrub, { target: { value: "30" } });
    expect(getByTestId("mp-current").textContent).toBe("0:30");
    expect(elementSeekedTo).toBe(30); // the seek reached the actual element
  });

  it("mute toggles the button state and mutes the element", async () => {
    const { container, getByTestId } = render(MediaPlayer, { src: "asset://a.mp3", type: "audio" });
    const media = container.querySelector("audio.mp-media") as HTMLMediaElement;
    const mute = getByTestId("mp-mute");
    expect(mute.getAttribute("aria-label")).toBe("Mute");
    await fireEvent.click(mute);
    expect(mute.getAttribute("aria-label")).toBe("Unmute");
    expect(mute.getAttribute("aria-pressed")).toBe("true");
    expect(media.muted).toBe(true);
  });

  it("the speed control cycles through the playback rates", async () => {
    const { getByTestId } = render(MediaPlayer, { src: "asset://a.mp3", type: "audio" });
    const rate = getByTestId("mp-rate");
    expect(rate.textContent).toContain("1"); // 1× to start
    await fireEvent.click(rate);
    expect(rate.textContent).toContain("1.25");
  });

  it("the loop toggle flips its pressed state and the element's loop flag", async () => {
    const { container, getByTestId } = render(MediaPlayer, { src: "asset://a.mp4", type: "video" });
    const media = container.querySelector("video.mp-media") as HTMLMediaElement;
    const loop = getByTestId("mp-loop");
    expect(loop.getAttribute("aria-pressed")).toBe("false");
    await fireEvent.click(loop);
    expect(loop.getAttribute("aria-pressed")).toBe("true");
    expect(media.loop).toBe(true);
  });

  it("an element error degrades to a message + Open externally action", async () => {
    const openExternal = vi.fn();
    const { container, getByText, queryByTestId } = render(MediaPlayer, {
      src: "asset://broken.mkv",
      type: "video",
      openExternal,
    });
    const media = container.querySelector("video.mp-media") as HTMLMediaElement;
    await fireEvent(media, new Event("error"));

    await waitFor(() => expect(queryByTestId("mp-fallback")).toBeTruthy());
    expect(container.querySelector("video.mp-media")).toBeNull(); // element removed on error
    expect(getByText(/Can't play this media file/i)).toBeTruthy();

    await fireEvent.click(getByText("Open externally"));
    expect(openExternal).toHaveBeenCalledTimes(1);
  });
});
