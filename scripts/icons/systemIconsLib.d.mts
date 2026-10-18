/** Types for systemIconsLib.mjs, so TypeScript tests can import it. */
export const COMMIT: 'd9f969054dc7fbb6fa89519036d2b971e0855b51';
export const REPO_URL: string;
export const SOURCE_DIR: string;
/** https://raw.githubusercontent.com/libretro/retroarch-assets/<COMMIT>/src/xmb/monochrome/ */
export const SOURCE_BASE: string;
export type Depicts = 'console' | 'handheld' | 'pad' | 'cabinet' | 'computer';
export interface IconSource {
  file: string;
  depicts: Depicts;
  audit: string;
}
export const SYSTEM_ICON_SOURCES: Readonly<Record<string, IconSource>>;
export interface NormalisedIcon {
  viewBox: string;
  body: string;
  aspect: number;
}
/** svgo pass + fills → currentColor + strip ids/classes/styles. Throws on a dirty result. */
export function normaliseSvg(svg: string): string;
/** The source viewBox, or null when it has none. */
export function readViewBox(svg: string): string | null;
/** "0 0 w h" from width/height when there is no viewBox; throws when neither exists. */
export function synthesiseViewBox(svg: string): string;
/** Parse an SVG transform list into one affine matrix [a b c d e f]. */
export function parseTransform(text: string | null): number[];
/** Bounding box [x0, y0, x1, y1] of every drawn path, or null. */
export function drawingBbox(
  svg: string,
): [number, number, number, number] | null;
/** Union bbox of every <path d> (svg-path-bbox), padded 2% per axis, clamped to the source viewBox. */
export function cropViewBox(
  svg: string,
  sourceViewBox: string,
): { viewBox: string; aspect: number };
/** Throws naming the offending token: #fff, #ffffff, <text, <image, style=, fill="<other>". */
export function assertClean(id: string, body: string): void;
export function innerMarkup(svg: string): string;
export function toSvgFile(icon: NormalisedIcon): string;
export function toGenerated(icons: Record<string, NormalisedIcon>): string;
export function toContactSheet(icons: Record<string, NormalisedIcon>): string;
export function toReadme(sources: Record<string, IconSource>): string;
