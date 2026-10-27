import { formatDecimalSize } from './format';

/** Samples older than this are dropped (D4). */
export const WINDOW_MS = 5_000;
/** No rate is shown before the samples span this long (D4). */
export const MIN_SPAN_MS = 2_000;
/** Without growth for this long, the rate goes back to measuring. */
export const STALL_MS = 10_000;

export interface Sample {
  t: number;
  v: number;
}

export interface RateState {
  key: number | string | null;
  samples: Sample[];
  /** Carried across items of one run (D4). */
  lastRate: number | null;
  /** When the value last grew. */
  movedAt: number | null;
}

export const initialRate = (): RateState => ({
  key: null,
  samples: [],
  lastRate: null,
  movedAt: null,
});

function spanRate(samples: Sample[]): number | null {
  const first = samples[0];
  const last = samples.at(-1);
  if (!first || !last || last.t - first.t < MIN_SPAN_MS) return null;
  return ((last.v - first.v) * 1000) / (last.t - first.t);
}

/** Adds a sample `v` at monotonic time `t` (ms) for `key` (an item, a job). */
export function pushSample(
  s: RateState,
  key: number | string,
  v: number,
  t: number,
): RateState {
  const prev = s.samples.at(-1);
  if (s.key !== key || !prev || v < prev.v) {
    return { key, samples: [{ t, v }], lastRate: s.lastRate, movedAt: t };
  }
  const cutoff = t - WINDOW_MS;
  const kept = s.samples.filter((x) => x.t >= cutoff);
  // Keep the newest sample older than the window as the anchor.
  const anchor = s.samples.filter((x) => x.t < cutoff).at(-1);
  const samples = [...(anchor ? [anchor] : []), ...kept, { t, v }];
  const rate = spanRate(samples);
  return {
    key,
    samples,
    // A stalled window measures 0; carry the last moving rate instead.
    lastRate: rate !== null && rate > 0 ? rate : s.lastRate,
    movedAt: v > prev.v ? t : s.movedAt,
  };
}

/** Units per second, or null while measuring or stalled. */
export function rateOf(s: RateState, now: number): number | null {
  if (s.movedAt !== null && now - s.movedAt >= STALL_MS) return null;
  return spanRate(s.samples) ?? s.lastRate;
}

/** Seconds left, or null when either side cannot give a finite estimate. */
export const etaSeconds = (rest: number, rate: number | null): number | null =>
  rate !== null && rate > 0 && Number.isFinite(rate) && Number.isFinite(rest)
    ? Math.max(0, rest) / rate
    : null;

/** D5: one key per wording, never «unos 1 minuto». */
export function etaKey(seconds: number): {
  key: string;
  vars: { n: number } | undefined;
} {
  if (seconds <= 60)
    return { key: 'organize.progress.leftUnderMinute', vars: undefined };
  if (seconds < 3600) {
    return {
      key: 'organize.progress.leftMinutes',
      vars: { n: Math.ceil(seconds / 60) },
    };
  }
  const h = Math.floor(seconds / 3600);
  return h === 1
    ? { key: 'organize.progress.leftOverHour', vars: undefined }
    : { key: 'organize.progress.leftOverHours', vars: { n: h } };
}

/** A rate in decimal units («24,1 MB»); the caller adds «/s». */
export const formatRate = (locale: string, perSecond: number): string =>
  formatDecimalSize(perSecond, locale);
