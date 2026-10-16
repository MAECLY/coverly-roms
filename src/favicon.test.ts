import { describe, expect, it } from 'vitest';

import indexHtml from '../index.html?raw';

describe('index.html', () => {
  it('links the favicon so the dev server never 404s on it', () => {
    expect(indexHtml).toContain(
      '<link rel="icon" type="image/png" href="/favicon.png" />',
    );
  });
});
