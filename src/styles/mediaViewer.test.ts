/// <reference types="node" />
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

// Read from disk: Vitest's CSS handling empties a `?raw` .css import.
const css = readFileSync(path.resolve(__dirname, 'globals.css'), 'utf8');

/** Body of the first top-level `selector` rule (outside any @media). */
function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const found = new RegExp(`\\n${escaped} \\{([^}]*)\\}`).exec(css);
  expect(found).not.toBeNull();
  return found![1]!;
}

describe('media viewer', () => {
  it('fits a video to the stage, letterboxed, instead of its own pixel size', () => {
    const canvas = rule('.media-viewer__stage--video .media-viewer__canvas');
    expect(canvas).toMatch(/width:\s*100%;/);
    expect(canvas).toMatch(/height:\s*100%;/);
    const video = rule('.media-viewer__stage--video .media-viewer__video');
    expect(video).toMatch(/width:\s*100%;/);
    expect(video).toMatch(/height:\s*100%;/);
    expect(video).toMatch(/max-height:\s*none;/);
    expect(video).toMatch(/object-fit:\s*contain;/);
  });
});
