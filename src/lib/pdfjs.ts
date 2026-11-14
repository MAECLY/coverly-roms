/**
 * pdf.js for the Extras viewer (extras spec D18), loaded only when a PDF
 * is opened: its own chunk, the legacy build (the app targets Safari 13 /
 * macOS 11, and Vite lowers syntax but does not polyfill the APIs the
 * modern build needs). The worker is a same-origin file (`worker-src
 * 'self'`; where module workers are missing, pdf.js falls back to running
 * it on the page, still without eval). CMaps and the standard fonts are
 * bundled under `/pdfjs/` by the `pdfjsAssets` Vite plugin. No eval, no
 * PDF JavaScript and no WebAssembly (the CSP has no `wasm-unsafe-eval`).
 */
import type {
  DocumentInitParameters,
  PDFDocumentLoadingTask,
} from 'pdfjs-dist/types/src/display/api';
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';

export type PdfjsModule = typeof import('pdfjs-dist/legacy/build/pdf.mjs');

/** Where the bundled pdf.js data lives, beside the app's own files. */
export function pdfAssetUrl(
  dir: 'cmaps' | 'standard_fonts',
  base: string = document.baseURI,
): string {
  return new URL(`/pdfjs/${dir}/`, base).href;
}

/** `getDocument`'s options plus the two flags pdf.js 6 no longer reads. */
export type PdfOpenOptions = DocumentInitParameters & {
  isEvalSupported: false;
  enableScripting: false;
};

/**
 * The options every document is opened with (D18). pdf.js 6 no longer
 * has an eval path (the build check asserts its shipped files contain
 * none) and runs PDF JavaScript only through the annotation layer, which
 * this viewer never renders; `isEvalSupported` and `enableScripting` stay
 * off anyway, so an older pdf.js could never turn either on.
 */
export function documentOptions(
  url: string,
  base: string = document.baseURI,
): PdfOpenOptions {
  return {
    url,
    isEvalSupported: false,
    enableScripting: false,
    useWasm: false,
    cMapUrl: pdfAssetUrl('cmaps', base),
    cMapPacked: true,
    standardFontDataUrl: pdfAssetUrl('standard_fonts', base),
  };
}

let loading: Promise<PdfjsModule> | null = null;

/** pdf.js, imported on first use; a failed import is tried again later. */
export function loadPdfjs(): Promise<PdfjsModule> {
  loading ??= import('pdfjs-dist/legacy/build/pdf.mjs').then(
    (m) => {
      m.GlobalWorkerOptions.workerSrc = workerUrl;
      return m;
    },
    (e: unknown) => {
      loading = null;
      throw e;
    },
  );
  return loading;
}

/** Starts loading `url`; the caller destroys the task when done. */
export async function openPdf(
  url: string,
): Promise<{ pdfjs: PdfjsModule; task: PDFDocumentLoadingTask }> {
  const pdfjs = await loadPdfjs();
  return { pdfjs, task: pdfjs.getDocument(documentOptions(url)) };
}
