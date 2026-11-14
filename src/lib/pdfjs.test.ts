import { beforeEach, describe, expect, it, vi } from 'vitest';

// pdf.js itself is replaced: this file checks how it is loaded and opened,
// not how it renders.
const pdfjs = vi.hoisted(() => ({
  evaluated: 0,
  getDocument: vi.fn((opts: unknown) => ({
    opts,
    promise: Promise.resolve({}),
    destroy: vi.fn(),
  })),
  GlobalWorkerOptions: { workerSrc: '' },
}));

vi.mock('pdfjs-dist/legacy/build/pdf.mjs', () => {
  pdfjs.evaluated += 1;
  return {
    getDocument: pdfjs.getDocument,
    GlobalWorkerOptions: pdfjs.GlobalWorkerOptions,
  };
});
vi.mock('pdfjs-dist/legacy/build/pdf.worker.min.mjs?url', () => ({
  default: '/assets/pdf.worker.min-abc.mjs',
}));

import { documentOptions, loadPdfjs, openPdf, pdfAssetUrl } from './pdfjs';

describe('pdf.js loader (extras D18)', () => {
  beforeEach(() => pdfjs.getDocument.mockClear());

  it('does not evaluate pdf.js until a PDF is opened', async () => {
    expect(pdfjs.evaluated).toBe(0);
    await loadPdfjs();
    await loadPdfjs();
    expect(pdfjs.evaluated).toBe(1);
  });

  it('points the worker at the bundled same-origin file', async () => {
    const m = await loadPdfjs();
    expect(m.GlobalWorkerOptions.workerSrc).toBe(
      '/assets/pdf.worker.min-abc.mjs',
    );
  });

  it('opens with no eval, no scripting, no wasm and the bundled data', () => {
    const o = documentOptions(
      'asset://localhost/Guide.pdf',
      'tauri://localhost/',
    );
    expect(o).toMatchObject({
      url: 'asset://localhost/Guide.pdf',
      isEvalSupported: false,
      enableScripting: false,
      useWasm: false,
      cMapUrl: 'tauri://localhost/pdfjs/cmaps/',
      cMapPacked: true,
      standardFontDataUrl: 'tauri://localhost/pdfjs/standard_fonts/',
    });
    expect(o).not.toHaveProperty('wasmUrl');
    expect(o).not.toHaveProperty('iccUrl');
  });

  it('builds the data URLs under /pdfjs/ from the page origin', () => {
    expect(pdfAssetUrl('cmaps', 'http://tauri.localhost/index.html')).toBe(
      'http://tauri.localhost/pdfjs/cmaps/',
    );
    expect(pdfAssetUrl('standard_fonts', 'http://localhost:1473/')).toBe(
      'http://localhost:1473/pdfjs/standard_fonts/',
    );
  });

  it('opens the document through getDocument with those options', async () => {
    const { task } = await openPdf('blob:guide');
    expect(pdfjs.getDocument).toHaveBeenCalledTimes(1);
    const opts = pdfjs.getDocument.mock.calls[0]![0] as Record<string, unknown>;
    expect(opts).toMatchObject({
      url: 'blob:guide',
      isEvalSupported: false,
      enableScripting: false,
    });
    expect(String(opts.cMapUrl)).toMatch(/\/pdfjs\/cmaps\/$/);
    expect(String(opts.standardFontDataUrl)).toMatch(
      /\/pdfjs\/standard_fonts\/$/,
    );
    expect(opts).not.toHaveProperty('wasmUrl');
    expect(task).toBeDefined();
  });
});
