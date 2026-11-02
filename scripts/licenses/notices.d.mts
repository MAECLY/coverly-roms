/** Types for notices.mjs, so TypeScript tests can import it. */
export interface DevPackage {
  name: string;
  version: string;
  license: string;
  dir: string;
}

export interface PnpmLicence {
  name: string;
  versions: string[];
  paths: string[];
}

export const RULE: string;
export const SUBRULE: string;
export const BUNDLED_DEV: readonly { name: string; via?: string }[];
export function block(packages: string[], license: string, text: string): string;
export function rustBlocks(rendered: string, ownCrates: Set<string>): string[];
export function jsBlocks(
  prod: Record<string, PnpmLicence[]>,
  devPackages: DevPackage[],
): string[];
