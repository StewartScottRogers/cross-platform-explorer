/**
 * Pure media-transport controller (CPE-1429, epic CPE-720).
 *
 * A framework-free state machine over a media element's transport: play/pause, seek, volume, mute,
 * playback rate and loop, plus time formatting. It holds NO reference to any DOM element and performs
 * NO side effects — every operation takes the current {@link MediaState} and returns the next one, so
 * it is trivially unit-testable and reusable. The Svelte component (`MediaPlayer.svelte`) owns the
 * actual `<audio>`/`<video>` element and syncs it to and from this state; CPE-1430's full-screen
 * quick-look drives the exact same controller, so keep this API clean and stable.
 */

/** Which element the media is rendered in — decided by the file's category, not by this controller. */
export type MediaType = "audio" | "video";

export interface MediaState {
  /** Whether playback is (intended to be) running. Mirrors the element's play/pause, but is the
   *  authority the UI renders off, so the transport reflects intent even before the element reacts. */
  playing: boolean;
  /** Current playback position, in seconds. */
  currentTime: number;
  /** Total media duration, in seconds. 0 when unknown (not yet loaded, or a live/seekless stream). */
  duration: number;
  /** Output volume, 0..1 (independent of {@link MediaState.muted}, like the DOM). */
  volume: number;
  /** Muted flag, independent of volume (so unmuting restores the prior volume). */
  muted: boolean;
  /** Playback-rate multiplier (1 = normal speed). Clamped to {@link PLAYBACK_RATES}' range. */
  rate: number;
  /** Whether playback loops back to the start at the end. */
  loop: boolean;
}

/** Selectable playback speeds for the transport's speed control (0.5×–2×). */
export const PLAYBACK_RATES = [0.5, 0.75, 1, 1.25, 1.5, 2] as const;

/** Slowest / fastest supported rate — the clamp bounds for {@link setRate}. */
export const MIN_RATE = PLAYBACK_RATES[0];
export const MAX_RATE = PLAYBACK_RATES[PLAYBACK_RATES.length - 1];

/** A fresh transport state: paused at 0, full volume, un-muted, normal speed, not looping. */
export function initialMediaState(): MediaState {
  return { playing: false, currentTime: 0, duration: 0, volume: 1, muted: false, rate: 1, loop: false };
}

function clamp(n: number, lo: number, hi: number): number {
  if (!Number.isFinite(n)) return lo;
  return Math.min(hi, Math.max(lo, n));
}

/** Start (intend) playback. */
export function play(s: MediaState): MediaState {
  return { ...s, playing: true };
}

/** Pause playback. */
export function pause(s: MediaState): MediaState {
  return { ...s, playing: false };
}

/** Flip between play and pause. */
export function togglePlay(s: MediaState): MediaState {
  return { ...s, playing: !s.playing };
}

/** Seek to a target time, clamped into `[0, duration]` (or `≥0` while the duration is still unknown). */
export function seek(s: MediaState, time: number): MediaState {
  const hi = s.duration > 0 ? s.duration : Number.POSITIVE_INFINITY;
  return { ...s, currentTime: clamp(time, 0, hi) };
}

/** Record the observed play position (from the element's `timeupdate`). Clamped `≥0`. */
export function setCurrentTime(s: MediaState, time: number): MediaState {
  return { ...s, currentTime: Math.max(0, Number.isFinite(time) ? time : 0) };
}

/** Record the known duration (from `loadedmetadata`/`durationchange`). NaN/∞/≤0 → 0 (unknown/stream). */
export function setDuration(s: MediaState, duration: number): MediaState {
  return { ...s, duration: Number.isFinite(duration) && duration > 0 ? duration : 0 };
}

/** Set the volume (0..1). A non-zero volume also un-mutes — matching what dragging the slider means. */
export function setVolume(s: MediaState, volume: number): MediaState {
  const v = clamp(volume, 0, 1);
  return { ...s, volume: v, muted: v === 0 ? s.muted : false };
}

/** Toggle mute without disturbing the stored volume (so unmuting restores it). */
export function toggleMute(s: MediaState): MediaState {
  return { ...s, muted: !s.muted };
}

/** Snap an arbitrary rate to the nearest preset in {@link PLAYBACK_RATES}. */
export function nearestRate(rate: number): number {
  return PLAYBACK_RATES.reduce(
    (best, r) => (Math.abs(r - rate) < Math.abs(best - rate) ? r : best),
    PLAYBACK_RATES[0] as number,
  );
}

/** Set the playback rate, clamped to the supported range. */
export function setRate(s: MediaState, rate: number): MediaState {
  return { ...s, rate: clamp(rate, MIN_RATE, MAX_RATE) };
}

/** Advance to the next preset speed, wrapping 2× → 0.5×. */
export function cycleRate(s: MediaState): MediaState {
  const i = PLAYBACK_RATES.indexOf(nearestRate(s.rate) as (typeof PLAYBACK_RATES)[number]);
  const next = PLAYBACK_RATES[(i + 1) % PLAYBACK_RATES.length];
  return { ...s, rate: next };
}

/** Toggle looping. */
export function toggleLoop(s: MediaState): MediaState {
  return { ...s, loop: !s.loop };
}

/** The volume the element should actually output: 0 when muted, else the stored volume. */
export function effectiveVolume(s: MediaState): number {
  return s.muted ? 0 : s.volume;
}

/** Fraction of the media played so far, 0..1 (0 when the duration is unknown). For the scrub fill. */
export function progress(s: MediaState): number {
  return s.duration > 0 ? clamp(s.currentTime / s.duration, 0, 1) : 0;
}

/**
 * Format a duration in seconds as `m:ss`, or `h:mm:ss` once it passes an hour. Negative, NaN and
 * non-finite inputs render as `0:00` rather than throwing or showing `NaN:NaN`.
 */
export function formatTime(seconds: number): string {
  const safe = Number.isFinite(seconds) && seconds > 0 ? seconds : 0;
  const total = Math.floor(safe);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}
