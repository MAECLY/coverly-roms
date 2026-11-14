/**
 * The browser mock's audio extra: a sine tone made here as a 16-bit mono
 * WAV, so the mini player has something of ours to play (no third-party
 * content, no file in the repository).
 */

export interface SineOptions {
  seconds: number;
  hz: number;
  /** Samples per second. */
  rate: number;
}

/** A 16-bit PCM mono WAV of a sine at `hz`, starting at zero. */
export function sineWav({ seconds, hz, rate }: SineOptions): Uint8Array {
  const samples = Math.round(seconds * rate);
  const bytes = new Uint8Array(44 + samples * 2);
  const view = new DataView(bytes.buffer);
  const ascii = (at: number, s: string) => {
    for (let i = 0; i < s.length; i += 1) bytes[at + i] = s.charCodeAt(i);
  };
  ascii(0, 'RIFF');
  view.setUint32(4, bytes.length - 8, true);
  ascii(8, 'WAVE');
  ascii(12, 'fmt ');
  view.setUint32(16, 16, true); // fmt chunk size
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // mono
  view.setUint32(24, rate, true);
  view.setUint32(28, rate * 2, true); // bytes per second
  view.setUint16(32, 2, true); // block align
  view.setUint16(34, 16, true); // bits per sample
  ascii(36, 'data');
  view.setUint32(40, samples * 2, true);
  // A sine starts at zero; a short fade out keeps the end from clicking.
  const fade = Math.max(1, Math.round(rate * 0.02));
  for (let i = 0; i < samples; i += 1) {
    const gain = Math.min(1, (samples - 1 - i) / fade);
    const v = Math.sin((2 * Math.PI * hz * i) / rate) * 0.6 * gain;
    view.setInt16(44 + i * 2, Math.round(v * 32_767), true);
  }
  return bytes;
}

let cached: string | null = null;

/** The 1 s 440 Hz tone as a `data:` URL (made once). */
export function toneDataUrl(): string {
  if (cached) return cached;
  const wav = sineWav({ seconds: 1, hz: 440, rate: 22_050 });
  let binary = '';
  const step = 0x8000;
  for (let i = 0; i < wav.length; i += step) {
    binary += String.fromCharCode(...wav.subarray(i, i + step));
  }
  cached = `data:audio/wav;base64,${btoa(binary)}`;
  return cached;
}
