import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import MediaQuickLook from "./MediaQuickLook.svelte";
import type { MediaTrack } from "../mediaQuickLook";

// jsdom doesn't implement HTMLMediaElement.play/pause; the player guards those calls, but stub them so
// the autoplay path (CPE-1430) doesn't log "Not implemented" noise.
Object.defineProperty(HTMLMediaElement.prototype, "play", {
  configurable: true,
  value: vi.fn(() => Promise.resolve()),
});
Object.defineProperty(HTMLMediaElement.prototype, "pause", {
  configurable: true,
  value: vi.fn(),
});

const a: MediaTrack = { path: "/dir/a.mp3", name: "a.mp3", type: "audio" };
const b: MediaTrack = { path: "/dir/b.mp4", name: "b.mp4", type: "video" };

describe("MediaQuickLook overlay (CPE-1430)", () => {
  it("mounts a bordered dialog and plays the current track through the shared MediaPlayer transport", () => {
    const { container, getByTestId } = render(MediaQuickLook, {
      track: a,
      position: 0,
      count: 2,
      assetUrl: (p: string) => `asset://${p}`,
    });
    // Dialog-standard overlay is present.
    const panel = container.querySelector('[role="dialog"]');
    expect(panel).toBeTruthy();
    expect(panel!.getAttribute("aria-modal")).toBe("true");
    // The audio element from the reused MediaPlayer, wired to the asset URL.
    const audio = container.querySelector("audio.mp-media") as HTMLAudioElement | null;
    expect(audio).toBeTruthy();
    expect(audio!.getAttribute("src")).toBe("asset:///dir/a.mp3");
    // The whole transport (from MediaPlayer) is embedded.
    expect(getByTestId("mp-transport")).toBeTruthy();
    // autoplay=true → the transport intends to play (Pause label showing).
    expect(getByTestId("mp-playpause").getAttribute("aria-label")).toBe("Pause");
  });

  it("renders a <video> element when the current track is video, and swaps the src on step", async () => {
    const { container, rerender } = render(MediaQuickLook, { track: a, position: 0, count: 2, assetUrl: (p: string) => p });
    expect(container.querySelector("audio.mp-media")).toBeTruthy();
    // Stepping to the next track (App re-renders with the new current) swaps the media element + src.
    await rerender({ track: b, position: 1, count: 2, assetUrl: (p: string) => p });
    await waitFor(() => expect(container.querySelector("video.mp-media")).toBeTruthy());
    expect((container.querySelector("video.mp-media") as HTMLVideoElement).getAttribute("src")).toBe("/dir/b.mp4");
    expect(container.querySelector("audio.mp-media")).toBeNull();
  });

  it("the step + close controls dispatch prev/next/close", async () => {
    const { getByLabelText, component } = render(MediaQuickLook, { track: a, position: 0, count: 2 });
    const prev = vi.fn(), next = vi.fn(), close = vi.fn();
    component.$on("prev", prev);
    component.$on("next", next);
    component.$on("close", close);
    await fireEvent.click(getByLabelText("Next"));
    await fireEvent.click(getByLabelText("Previous"));
    await fireEvent.click(getByLabelText("Close"));
    expect(next).toHaveBeenCalledTimes(1);
    expect(prev).toHaveBeenCalledTimes(1);
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("clicking the backdrop closes, but clicking inside the panel does not", async () => {
    const { getByTestId, container, component } = render(MediaQuickLook, { track: a, position: 0, count: 1 });
    const close = vi.fn();
    component.$on("close", close);
    // Clicking the panel body doesn't close (stopPropagation).
    await fireEvent.click(container.querySelector('[role="dialog"]')!);
    expect(close).not.toHaveBeenCalled();
    // Clicking the backdrop itself closes.
    await fireEvent.click(getByTestId("media-quicklook"));
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("the repeat + shuffle chips reflect state and dispatch their toggles", async () => {
    const { getByLabelText, component } = render(MediaQuickLook, {
      track: a, position: 0, count: 3, repeat: "all", shuffled: true,
    });
    // State is reflected in the labels.
    expect(getByLabelText("Repeat all")).toBeTruthy();
    expect(getByLabelText("Shuffle on")).toBeTruthy();
    const cycleRepeat = vi.fn(), toggleShuffle = vi.fn();
    component.$on("cycleRepeat", cycleRepeat);
    component.$on("toggleShuffle", toggleShuffle);
    await fireEvent.click(getByLabelText("Repeat all"));
    await fireEvent.click(getByLabelText("Shuffle on"));
    expect(cycleRepeat).toHaveBeenCalledTimes(1);
    expect(toggleShuffle).toHaveBeenCalledTimes(1);
  });

  it("hides the step buttons' affordance for a lone track (single-item folder)", () => {
    const { getByLabelText, queryByText } = render(MediaQuickLook, { track: a, position: 0, count: 1 });
    expect((getByLabelText("Next") as HTMLButtonElement).disabled).toBe(true);
    expect((getByLabelText("Previous") as HTMLButtonElement).disabled).toBe(true);
    expect(queryByText("1 / 1")).toBeNull(); // no counter for a single item
  });
});
