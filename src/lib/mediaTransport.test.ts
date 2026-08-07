import { describe, it, expect } from "vitest";
import {
  initialMediaState,
  play,
  pause,
  togglePlay,
  seek,
  setCurrentTime,
  setDuration,
  setVolume,
  toggleMute,
  setRate,
  cycleRate,
  nearestRate,
  toggleLoop,
  effectiveVolume,
  progress,
  formatTime,
  PLAYBACK_RATES,
  MIN_RATE,
  MAX_RATE,
} from "./mediaTransport";

describe("mediaTransport controller (CPE-1429)", () => {
  it("starts paused at 0, full volume, un-muted, 1× speed, not looping", () => {
    const s = initialMediaState();
    expect(s).toEqual({
      playing: false,
      currentTime: 0,
      duration: 0,
      volume: 1,
      muted: false,
      rate: 1,
      loop: false,
    });
  });

  it("is pure — transitions return a new object and never mutate the input", () => {
    const s = initialMediaState();
    const next = play(s);
    expect(next).not.toBe(s);
    expect(s.playing).toBe(false); // original untouched
    expect(next.playing).toBe(true);
  });

  it("play / pause / togglePlay flip the playing flag", () => {
    expect(play(initialMediaState()).playing).toBe(true);
    expect(pause(play(initialMediaState())).playing).toBe(false);
    const s = initialMediaState();
    expect(togglePlay(s).playing).toBe(true);
    expect(togglePlay(togglePlay(s)).playing).toBe(false);
  });

  it("seek clamps into [0, duration]", () => {
    const s = setDuration(initialMediaState(), 100);
    expect(seek(s, 30).currentTime).toBe(30);
    expect(seek(s, -5).currentTime).toBe(0); // below start
    expect(seek(s, 999).currentTime).toBe(100); // past the end
  });

  it("seek only clamps to ≥0 while the duration is still unknown", () => {
    const s = initialMediaState(); // duration 0 => unknown
    expect(seek(s, 42).currentTime).toBe(42);
    expect(seek(s, -1).currentTime).toBe(0);
  });

  it("setDuration treats NaN / ∞ / non-positive as unknown (0)", () => {
    const s = initialMediaState();
    expect(setDuration(s, 120).duration).toBe(120);
    expect(setDuration(s, NaN).duration).toBe(0);
    expect(setDuration(s, Infinity).duration).toBe(0);
    expect(setDuration(s, 0).duration).toBe(0);
    expect(setDuration(s, -3).duration).toBe(0);
  });

  it("setCurrentTime clamps to ≥0 and ignores NaN", () => {
    const s = initialMediaState();
    expect(setCurrentTime(s, 12.5).currentTime).toBe(12.5);
    expect(setCurrentTime(s, -4).currentTime).toBe(0);
    expect(setCurrentTime(s, NaN).currentTime).toBe(0);
  });

  it("setVolume clamps 0..1 and un-mutes when a non-zero volume is set", () => {
    const muted = toggleMute(initialMediaState());
    expect(muted.muted).toBe(true);
    const raised = setVolume(muted, 0.5);
    expect(raised.volume).toBe(0.5);
    expect(raised.muted).toBe(false); // dragging the slider up un-mutes
    expect(setVolume(initialMediaState(), 2).volume).toBe(1); // clamp high
    expect(setVolume(initialMediaState(), -1).volume).toBe(0); // clamp low
  });

  it("setVolume to exactly 0 leaves the mute flag alone", () => {
    const s = initialMediaState();
    expect(setVolume(s, 0).muted).toBe(false);
    expect(setVolume(toggleMute(s), 0).muted).toBe(true);
  });

  it("toggleMute flips mute without disturbing the stored volume", () => {
    const s = setVolume(initialMediaState(), 0.3);
    const m = toggleMute(s);
    expect(m.muted).toBe(true);
    expect(m.volume).toBe(0.3); // preserved for un-mute
    expect(toggleMute(m).muted).toBe(false);
  });

  it("effectiveVolume is 0 while muted, else the stored volume", () => {
    const s = setVolume(initialMediaState(), 0.4);
    expect(effectiveVolume(s)).toBe(0.4);
    expect(effectiveVolume(toggleMute(s))).toBe(0);
  });

  it("setRate clamps to the supported range", () => {
    expect(setRate(initialMediaState(), 1.5).rate).toBe(1.5);
    expect(setRate(initialMediaState(), 5).rate).toBe(MAX_RATE);
    expect(setRate(initialMediaState(), 0.1).rate).toBe(MIN_RATE);
  });

  it("cycleRate walks the presets and wraps 2× back to 0.5×", () => {
    let s = initialMediaState(); // rate 1 (index 2)
    expect(cycleRate(s).rate).toBe(1.25);
    s = setRate(s, MAX_RATE);
    expect(cycleRate(s).rate).toBe(MIN_RATE); // wrap
  });

  it("nearestRate snaps an off-grid rate onto a preset", () => {
    expect(nearestRate(1.3)).toBe(1.25);
    expect(nearestRate(0.6)).toBe(0.5);
    expect(PLAYBACK_RATES).toContain(nearestRate(1.9));
  });

  it("toggleLoop flips looping", () => {
    const s = initialMediaState();
    expect(toggleLoop(s).loop).toBe(true);
    expect(toggleLoop(toggleLoop(s)).loop).toBe(false);
  });

  it("progress is the played fraction, 0 when duration is unknown", () => {
    const s = setCurrentTime(setDuration(initialMediaState(), 200), 50);
    expect(progress(s)).toBe(0.25);
    expect(progress(initialMediaState())).toBe(0); // duration unknown
  });

  describe("formatTime", () => {
    it("renders m:ss below an hour", () => {
      expect(formatTime(0)).toBe("0:00");
      expect(formatTime(5)).toBe("0:05");
      expect(formatTime(65)).toBe("1:05");
      expect(formatTime(600)).toBe("10:00");
    });
    it("renders h:mm:ss at and past an hour", () => {
      expect(formatTime(3600)).toBe("1:00:00");
      expect(formatTime(3661)).toBe("1:01:01");
      expect(formatTime(7325)).toBe("2:02:05");
    });
    it("floors fractional seconds", () => {
      expect(formatTime(9.9)).toBe("0:09");
    });
    it("renders 0:00 for negative, NaN and non-finite input", () => {
      expect(formatTime(-10)).toBe("0:00");
      expect(formatTime(NaN)).toBe("0:00");
      expect(formatTime(Infinity)).toBe("0:00");
    });
  });
});
