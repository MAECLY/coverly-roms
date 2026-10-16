const XML: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&apos;',
};

export function escapeXml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => XML[c] ?? c);
}

/** A labelled placeholder image (the label is escaped). */
export function labelSvg(label: string, wide: boolean, hue: number): string {
  const [w, h] = wide ? [320, 240] : [250, 360];
  const text = escapeXml(label.length > 40 ? `${label.slice(0, 39)}…` : label);
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">` +
    `<rect width="100%" height="100%" fill="hsl(${hue} 40% 35%)"/>` +
    `<text x="50%" y="50%" fill="#fff" font-family="system-ui" font-size="15" text-anchor="middle">${text}</text>` +
    `</svg>`
  );
}
