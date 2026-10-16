const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const;

/** Binary units (1 KB = 1024 B), one decimal, locale separators. */
export function formatSize(bytes: number, locale: string): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const n = new Intl.NumberFormat(locale, {
    maximumFractionDigits: unit === 0 ? 0 : 1,
  }).format(value);
  return `${n} ${UNITS[unit]}`;
}

export function formatDate(mtime: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }).format(
    new Date(mtime * 1000),
  );
}
