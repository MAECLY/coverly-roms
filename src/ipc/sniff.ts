/** The MIME type of PNG/JPEG/WebP/GIF bytes, or null (SVG is never accepted). */
export function sniffImageMime(b: Uint8Array): string | null {
  const ascii = (from: number, to: number) =>
    String.fromCharCode(...b.subarray(from, to));
  if (b.length >= 8 && b[0] === 0x89 && ascii(1, 4) === 'PNG') {
    return 'image/png';
  }
  if (b.length >= 3 && b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff) {
    return 'image/jpeg';
  }
  if (b.length >= 12 && ascii(0, 4) === 'RIFF' && ascii(8, 12) === 'WEBP') {
    return 'image/webp';
  }
  if (b.length >= 6 && ascii(0, 3) === 'GIF') return 'image/gif';
  return null;
}
