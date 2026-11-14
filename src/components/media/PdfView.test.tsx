import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import { createRef } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend, type Backend } from '@/ipc/backend';
import type { CategoryRef, ExtraFileView, ExtrasView } from '@/ipc/types';
import { useViewerStore } from '@/stores/viewerStore';

import { MediaViewer } from './MediaViewer';
import { PdfView, type PdfHandle } from './PdfView';
import { GAME, listing as trackListing } from './testTracks';

/* ---- A stand-in pdf.js: pages of 600 × 800 pt, a text layer that writes
 * its text, renders that settle at once unless a page is told to fail. */
const pdf = vi.hoisted(() => {
  const state = {
    evaluated: 0,
    pages: 5,
    failPage: 0,
    failDocument: false,
    rendered: [] as number[],
    destroyed: 0,
    options: [] as Record<string, unknown>[],
  };
  class TextLayer {
    container: HTMLElement;
    page: number;
    constructor({
      container,
      textContentSource,
    }: {
      container: HTMLElement;
      textContentSource: { page: number };
    }) {
      this.container = container;
      this.page = textContentSource.page;
    }
    render() {
      const span = document.createElement('span');
      span.textContent = `Texto de la página ${this.page}`;
      this.container.append(span);
      return Promise.resolve();
    }
    cancel() {}
  }
  class RenderingCancelledException extends Error {}
  function page(n: number) {
    return {
      pageNumber: n,
      getViewport: ({ scale }: { scale: number }) => ({
        width: 600 * scale,
        height: 800 * scale,
        scale,
        rotation: 0,
      }),
      render: () => {
        state.rendered.push(n);
        return {
          promise:
            n === state.failPage
              ? Promise.reject(new Error('JpxError'))
              : Promise.resolve(),
          cancel: () => undefined,
        };
      },
      streamTextContent: () => ({ page: n }),
      cleanup: () => true,
    };
  }
  const getDocument = (opts: Record<string, unknown>) => {
    state.options.push(opts);
    return {
      promise: state.failDocument
        ? Promise.reject(new Error('InvalidPDFException'))
        : Promise.resolve({
            numPages: state.pages,
            getPage: (n: number) => Promise.resolve(page(n)),
          }),
      destroy: () => {
        state.destroyed += 1;
        return Promise.resolve();
      },
    };
  };
  return { state, TextLayer, RenderingCancelledException, getDocument };
});

vi.mock('pdfjs-dist/legacy/build/pdf.mjs', () => {
  pdf.state.evaluated += 1;
  return {
    getDocument: pdf.getDocument,
    GlobalWorkerOptions: { workerSrc: '' },
    TextLayer: pdf.TextLayer,
    RenderingCancelledException: pdf.RenderingCancelledException,
  };
});
vi.mock('pdfjs-dist/legacy/build/pdf.worker.min.mjs?url', () => ({
  default: '/assets/pdf.worker.min.mjs',
}));

/* ---- IntersectionObserver driven by the test: `near(…pages)` says which
 * page boxes are near the viewport. */
type IoCallback = (entries: Partial<IntersectionObserverEntry>[]) => void;
const observers: { cb: IoCallback; els: Set<Element> }[] = [];
class FakeIO {
  cb: IoCallback;
  els = new Set<Element>();
  constructor(cb: IoCallback) {
    this.cb = cb;
    observers.push(this);
  }
  observe(el: Element) {
    this.els.add(el);
  }
  unobserve(el: Element) {
    this.els.delete(el);
  }
  disconnect() {
    this.els.clear();
  }
  takeRecords() {
    return [];
  }
}

function near(...pages: number[]) {
  act(() => {
    for (const o of observers) {
      o.cb(
        [...o.els].map((target) => ({
          target,
          isIntersecting: pages.includes(
            Number(target.getAttribute('data-page')),
          ),
        })),
      );
    }
  });
}

const canvases = () => document.querySelectorAll('canvas').length;

beforeEach(() => {
  observers.length = 0;
  vi.stubGlobal('IntersectionObserver', FakeIO);
  Object.assign(pdf.state, {
    pages: 5,
    failPage: 0,
    failDocument: false,
    rendered: [],
    destroyed: 0,
    options: [],
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  act(() => useViewerStore.getState().close());
});

function showPdf(over: Partial<Parameters<typeof PdfView>[0]> = {}) {
  const ref = createRef<PdfHandle>();
  const props = {
    url: 'blob:guide',
    name: 'Field Guide.pdf',
    zoom: null,
    onFit: vi.fn(),
    onPage: vi.fn(),
    onUnplayable: vi.fn(),
    canExternal: true,
    onOpenExternal: vi.fn(),
    ...over,
  };
  const utils = render(
    <I18nProvider locale="es">
      <PdfView ref={ref} {...props} />
    </I18nProvider>,
  );
  return { ...utils, ref, props };
}

async function loaded() {
  await waitFor(() =>
    expect(document.querySelectorAll('[data-page]').length).toBe(
      pdf.state.pages,
    ),
  );
}

// First in the file: pdf.js is cached once imported, so this sees the
// first import.
it('imports pdf.js only when a PDF is opened', async () => {
  viewer();
  openAt('Artwork/Box Front.png');
  await screen.findByRole('img', { name: 'Box Front.png' });
  expect(pdf.state.evaluated).toBe(0);
  openAt('Guides/Field Guide.pdf');
  await waitFor(() =>
    expect(document.querySelectorAll('[data-page]').length).toBe(5),
  );
  expect(pdf.state.evaluated).toBe(1);
  expect(pdf.state.options.at(-1)).toMatchObject({ url: 'blob:media-1' });
});

describe('PdfView (extras D18)', () => {
  it('opens with no eval, no scripting and the bundled data, without wasm', async () => {
    showPdf();
    await loaded();
    const o = pdf.state.options[0]!;
    expect(o).toMatchObject({
      url: 'blob:guide',
      isEvalSupported: false,
      enableScripting: false,
    });
    expect(String(o.cMapUrl)).toContain('/pdfjs/cmaps/');
    expect(String(o.standardFontDataUrl)).toContain('/pdfjs/standard_fonts/');
    expect(o).not.toHaveProperty('wasmUrl');
  });

  it('shows a spinner until the document is ready', async () => {
    showPdf();
    expect(screen.getByRole('status')).toBeInTheDocument();
    await loaded();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('renders only the pages near the viewport', async () => {
    showPdf();
    await loaded();
    near(1, 2);
    await waitFor(() => expect(canvases()).toBe(2));
    expect(document.querySelector('[data-page="1"] canvas')).not.toBeNull();
    expect(document.querySelector('[data-page="2"] canvas')).not.toBeNull();
    expect(document.querySelector('[data-page="5"] canvas')).toBeNull();
    expect(new Set(pdf.state.rendered)).toEqual(new Set([1, 2]));
  });

  it('keeps at most three rendered canvases', async () => {
    showPdf();
    await loaded();
    near(1, 2, 3, 4, 5);
    await waitFor(() => expect(canvases()).toBe(3));
    // The three nearest the current page (1).
    for (const n of [1, 2, 3])
      expect(
        document.querySelector(`[data-page="${n}"] canvas`),
      ).not.toBeNull();
    near(4, 5);
    await waitFor(() =>
      expect(document.querySelector('[data-page="5"] canvas')).not.toBeNull(),
    );
    expect(canvases()).toBeLessThanOrEqual(3);
    expect(document.querySelector('[data-page="1"] canvas')).toBeNull();
  });

  it('lays the text layer over each rendered page', async () => {
    showPdf();
    await loaded();
    near(1);
    expect(await screen.findByText('Texto de la página 1')).toBeInTheDocument();
    expect(document.querySelector('[data-page="1"] .textLayer')).not.toBeNull();
  });

  it('names each page for assistive tech and reports the page count', async () => {
    const { props } = showPdf();
    await loaded();
    expect(
      screen.getByRole('group', { name: 'Página 3 de 5' }),
    ).toBeInTheDocument();
    await waitFor(() => expect(props.onPage).toHaveBeenLastCalledWith(1, 5));
  });

  it('steps pages with the handle (PageUp/PageDown/Home/End)', async () => {
    const { ref, props } = showPdf();
    await loaded();
    act(() => ref.current!.step('next'));
    expect(props.onPage).toHaveBeenLastCalledWith(2, 5);
    act(() => ref.current!.step('last'));
    expect(props.onPage).toHaveBeenLastCalledWith(5, 5);
    act(() => ref.current!.step('next'));
    expect(props.onPage).toHaveBeenLastCalledWith(5, 5);
    act(() => ref.current!.step('previous'));
    expect(props.onPage).toHaveBeenLastCalledWith(4, 5);
    act(() => ref.current!.step('first'));
    expect(props.onPage).toHaveBeenLastCalledWith(1, 5);
  });

  it('says a page that fails to render cannot be shown here, with «Abrir con otra app»', async () => {
    pdf.state.failPage = 2;
    const { props } = showPdf();
    await loaded();
    near(1, 2);
    const page2 = document.querySelector<HTMLElement>('[data-page="2"]')!;
    await waitFor(() =>
      expect(page2).toHaveTextContent('Esta página no se puede mostrar aquí.'),
    );
    expect(document.querySelector('[data-page="1"]')).not.toHaveTextContent(
      'Esta página no se puede mostrar aquí.',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Abrir con otra app' }));
    expect(props.onOpenExternal).toHaveBeenCalledTimes(1);
  });

  it('offers no other app for a failed page when the type is not allowed', async () => {
    pdf.state.failPage = 1;
    showPdf({ canExternal: false });
    await loaded();
    near(1);
    await screen.findByText('Esta página no se puede mostrar aquí.');
    expect(
      screen.queryByRole('button', { name: 'Abrir con otra app' }),
    ).not.toBeInTheDocument();
  });

  it('gives up on a document pdf.js cannot open', async () => {
    pdf.state.failDocument = true;
    const { props } = showPdf();
    await waitFor(() => expect(props.onUnplayable).toHaveBeenCalledTimes(1));
  });

  it('destroys the document when closed', async () => {
    const { unmount } = showPdf();
    await loaded();
    unmount();
    expect(pdf.state.destroyed).toBe(1);
  });

  it('destroys a document that starts loading after the viewer closed', async () => {
    const { unmount } = showPdf();
    // Closed before pdf.js handed back its loading task.
    unmount();
    await waitFor(() => expect(pdf.state.options).toHaveLength(1));
    await waitFor(() => expect(pdf.state.destroyed).toBe(1));
  });

  it('sizes pages by the zoom, in CSS pixels', async () => {
    showPdf({ zoom: 1 });
    await loaded();
    const box = document.querySelector<HTMLElement>('[data-page="1"]')!;
    expect(box.style.width).toBe('800px');
    expect(box.style.height).toBe(`${Math.round((800 * 96) / 72)}px`);
  });
});

/* ---- Inside the viewer: lazy import, keys and the page counter. */

const GUIDES: CategoryRef = { kind: 'fixed', name: 'guides' };
const ARTWORK: CategoryRef = { kind: 'fixed', name: 'artwork' };

function file(
  index: number,
  path: string,
  category: CategoryRef,
  kind: ExtraFileView['kind'],
): ExtraFileView {
  return {
    index,
    category,
    path,
    name: path.split('/').pop()!,
    kind,
    provenance: 'user',
    format: null,
    tags: [],
    size: 1000,
    link: false,
    playable: 'inline',
  };
}

const FILES = [
  file(0, 'Artwork/Box Front.png', ARTWORK, 'image'),
  file(1, 'Guides/Field Guide.pdf', GUIDES, 'document'),
];

function listing(): ExtrasView {
  return { ...trackListing(FILES, 3, 7), files: FILES };
}

function viewer() {
  const b = {
    extrasMedia: vi.fn(
      async (_g: number, _e: number, _r: number, i: number) => ({
        url: `blob:media-${i}`,
        mime: i === 1 ? 'application/pdf' : 'image/png',
        kind: i === 1 ? ('pdf' as const) : ('image' as const),
      }),
    ),
    extrasReveal: vi.fn(async () => undefined),
    extrasOpenExternal: vi.fn(async () => undefined),
  };
  setBackend(b as unknown as Backend);
  render(
    <I18nProvider locale="es">
      <MediaViewer generation={GAME.generation} listing={listing()} />
    </I18nProvider>,
  );
  return b;
}

function openAt(path: string) {
  act(() =>
    useViewerStore
      .getState()
      .show({ generation: GAME.generation, entryId: 7, path }),
  );
}

describe('the PDF in the viewer (extras D18, D23)', () => {
  it('shows «Página 1 de 5» in mono and moves with PageDown, End, PageUp and Home', async () => {
    viewer();
    openAt('Guides/Field Guide.pdf');
    const dialog = await screen.findByRole('dialog');
    const counter = await screen.findByText(
      (_, el) =>
        el?.classList.contains('media-viewer__page') === true &&
        el.textContent === 'Página 1 de 5',
    );
    expect(counter.querySelectorAll('.mono')).toHaveLength(2);
    fireEvent.keyDown(dialog, { key: 'PageDown' });
    expect(counter).toHaveTextContent('Página 2 de 5');
    fireEvent.keyDown(dialog, { key: 'End' });
    expect(counter).toHaveTextContent('Página 5 de 5');
    fireEvent.keyDown(dialog, { key: 'PageUp' });
    expect(counter).toHaveTextContent('Página 4 de 5');
    fireEvent.keyDown(dialog, { key: 'Home' });
    expect(counter).toHaveTextContent('Página 1 de 5');
  });

  it('zooms the PDF with + and 0', async () => {
    viewer();
    openAt('Guides/Field Guide.pdf');
    const dialog = await screen.findByRole('dialog');
    await waitFor(() =>
      expect(document.querySelectorAll('[data-page]').length).toBe(5),
    );
    // No layout in jsdom: fitted at 100 %.
    expect(screen.getByText('100')).toBeInTheDocument();
    fireEvent.keyDown(dialog, { key: '+' });
    expect(screen.getByText('150')).toBeInTheDocument();
    fireEvent.keyDown(dialog, { key: '0' });
    expect(screen.getByText('100')).toBeInTheDocument();
  });

  it('falls back to the unplayable view when the PDF does not open', async () => {
    pdf.state.failDocument = true;
    viewer();
    openAt('Guides/Field Guide.pdf');
    expect(
      await screen.findByText('Coverly no puede mostrar este archivo aquí.'),
    ).toBeInTheDocument();
  });
});
