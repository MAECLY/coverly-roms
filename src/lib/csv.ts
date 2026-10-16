/**
 * CSV text like the core's writer: RFC 4180 quoting, `\n` line ends, and
 * fields a spreadsheet would run as a formula prefixed with `'` (ROM names
 * are hostile; OWASP CSV injection guidance).
 */
const FORMULA = /^[=+\-@\t\r]/;

export function csvField(value: string): string {
  const v = FORMULA.test(value) ? `'${value}` : value;
  return /[",\n\r]/.test(v) || v.startsWith(' ') || v.endsWith(' ')
    ? `"${v.replace(/"/g, '""')}"`
    : v;
}

export function csvRow(fields: readonly (string | number)[]): string {
  return fields.map((f) => csvField(String(f))).join(',');
}

export function csvText(
  rows: readonly (readonly (string | number)[])[],
): string {
  return rows.map(csvRow).join('\n') + '\n';
}
