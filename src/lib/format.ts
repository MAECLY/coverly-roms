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

const DECIMAL_UNITS = ['B', 'kB', 'MB', 'GB', 'TB'] as const;

/**
 * Decimal units (1 kB = 1000 B) for what organise extracts, like the CLI
 * and the spec: `405,7 MB`, `1,19 GB` (two decimals from GB on, so a disc
 * game's size stays readable), locale separators.
 */
export function formatDecimalSize(bytes: number, locale: string): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < DECIMAL_UNITS.length - 1) {
    value /= 1000;
    unit += 1;
  }
  const n = new Intl.NumberFormat(locale, {
    maximumFractionDigits: unit === 0 ? 0 : unit >= 3 ? 2 : 1,
  }).format(value);
  return `${n} ${DECIMAL_UNITS[unit]}`;
}

export function formatDate(mtime: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }).format(
    new Date(mtime * 1000),
  );
}
