import { describe, expect, it } from 'vitest';

import { sniffImageMime } from './sniff';

const bytes = (...b: number[]) => new Uint8Array(b);
const ascii = (s: string) => new TextEncoder().encode(s);

describe('sniffImageMime', () => {
  it('recognises PNG, JPEG, WebP and GIF', () => {
    expect(
      sniffImageMime(bytes(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a)),
    ).toBe('image/png');
    expect(sniffImageMime(bytes(0xff, 0xd8, 0xff, 0xe0))).toBe('image/jpeg');
    expect(sniffImageMime(ascii('RIFF\0\0\0\0WEBPVP8 '))).toBe('image/webp');
    expect(sniffImageMime(ascii('GIF89a'))).toBe('image/gif');
  });

  it('refuses SVG, HTML and short input', () => {
    expect(sniffImageMime(ascii('<svg onload=alert(1)>'))).toBeNull();
    expect(sniffImageMime(ascii('<html>'))).toBeNull();
    expect(sniffImageMime(bytes(0x89))).toBeNull();
  });
});
