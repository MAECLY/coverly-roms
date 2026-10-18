// Tests for site/assets/release.js, the latest release read in the browser.
// Node's own runner, no dependencies: `node --test scripts/site/`.
//
// Each test fills site/ with apply_release.py (the fixture release v9.8.7, or
// none), lifts the elements release.js touches out of the filled page into a
// small stand-in DOM, runs release.js in a VM with a fake fetch and
// sessionStorage, and looks at what it changed.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import vm from 'node:vm';

const ROOT = new URL('../../', import.meta.url).pathname;
const JS = readFileSync(join(ROOT, 'site/assets/release.js'), 'utf8');
const FIXTURE = JSON.parse(readFileSync(join(ROOT, 'scripts/site/fixtures/release.json'), 'utf8'));
const BASE = 'https://github.com/MAECLY/coverly-roms/releases/';
const PYTHON = process.env.PYTHON || 'python3';

const pages = {};
function filled(release, lang) {
  const key = `${release}-${lang}`;
  if (pages[key]) return pages[key];
  const out = mkdtempSync(join(tmpdir(), 'cr-release-js-'));
  rmSync(out, { recursive: true });
  const src = release ? ['--release-json', join(ROOT, 'scripts/site/fixtures/release.json')] : ['--no-release'];
  execFileSync(PYTHON, ['-I', join(ROOT, 'scripts/site/apply_release.py'), ...src,
    '--repo', 'MAECLY/coverly-roms', '--site', join(ROOT, 'site'), '--out', out], { stdio: 'pipe' });
  pages[key] = readFileSync(join(out, lang === 'es' ? 'index.html' : 'en/index.html'), 'utf8');
  rmSync(out, { recursive: true });
  return pages[key];
}

const entity = (s) => s.replace(/<[^>]*>/g, '').replace(/&nbsp;/g, ' ').replace(/&amp;/g, '&');

// The elements release.js looks for: a[data-asset], [data-rel=...],
// [data-rel-new], [data-rel-old], a[data-rel-file], button[data-copy]. Their own markup holds no nesting
// release.js depends on, so a flat scan is enough.
function dom(html, os = 'other') {
  const els = [];
  const tag = /<(a|span|p|h3|div|button)\b([^>]*)>/g;
  let m;
  while ((m = tag.exec(html))) {
    const attrs = {};
    for (const [, k, , v] of m[2].matchAll(/([\w-]+)(="([^"]*)")?/g)) attrs[k] = v ?? '';
    if (!['data-asset', 'data-rel', 'data-rel-new', 'data-rel-old', 'data-rel-file', 'data-copy'].some((k) => k in attrs)) continue;
    const end = html.indexOf(`</${m[1]}>`, tag.lastIndex);
    els.push({
      tag: m[1], attrs, textContent: entity(html.slice(tag.lastIndex, end)), hidden: 'hidden' in attrs,
      getAttribute(k) { return k in this.attrs ? this.attrs[k] : null; },
      setAttribute(k, v) { this.attrs[k] = String(v); },
    });
  }
  const match = (sel) => {
    const [, t, k, v] = /^(\w*)\[([\w-]+)(?:="([^"]*)")?\]$/.exec(sel);
    return (e) => (!t || e.tag === t) && k in e.attrs && (v === undefined || e.attrs[k] === v);
  };
  return {
    els,
    document: {
      documentElement: { lang: /<html lang="(\w+)"/.exec(html)[1], getAttribute: (k) => (k === 'data-os' ? os : null) },
      querySelectorAll: (sel) => els.filter(match(sel)),
      createElement: () => ({ setAttribute() {} }),
    },
  };
}

// A release `version` made from the fixture, every file renamed and 1 MiB
// bigger, optionally edited by `edit`.
function release(version, edit = (r) => r) {
  const r = JSON.parse(JSON.stringify(FIXTURE).replaceAll('9.8.7', version));
  r.published_at = '2027-06-09T10:00:00Z';
  for (const a of r.assets) a.size += 1048576;
  return edit(r);
}

async function run(html, answer, store = {}, os = 'other', userAgent = '') {
  const page = dom(html, os);
  const calls = [];
  const ctx = {
    document: page.document,
    navigator: { userAgent },
    sessionStorage: {
      getItem: (k) => (k in store ? store[k] : null),
      setItem: (k, v) => { store[k] = v; },
    },
    AbortController,
    setTimeout,
    clearTimeout,
    Date,
    JSON,
    Promise,
    fetch: async (url, opts) => {
      calls.push([url, opts]);
      if (answer instanceof Error) throw answer;
      return { ok: answer.status === 200, status: answer.status, json: async () => answer.body };
    },
  };
  ctx.window = ctx;
  vm.runInNewContext(JS, ctx);
  await new Promise((r) => setTimeout(r, 20));
  const links = page.els.filter((e) => 'data-asset' in e.attrs);
  const rel = (k) => page.els.filter((e) => e.attrs['data-rel'] === k);
  return { page, links, rel, calls, store };
}

for (const lang of ['es', 'en']) {
  test(`${lang}: a newer release replaces every download, size, version and date`, async () => {
    const html = filled(true, lang);
    const { links, rel, calls } = await run(html, { status: 200, body: release('9.9.0') });
    assert.equal(calls.length, 1);
    assert.equal(calls[0][0], 'https://api.github.com/repos/MAECLY/coverly-roms/releases/latest');
    assert.ok(links.length >= 13, 'every download and SHA256SUMS is marked');
    for (const a of links) {
      assert.match(a.attrs.href, /^https:\/\/github\.com\/MAECLY\/coverly-roms\/releases\/download\/v9\.9\.0\//);
      assert.ok(!a.attrs.href.includes('9.8.7'), a.attrs.href);
    }
    const dmg = links.find((a) => a.attrs.href.endsWith('_universal.dmg'));
    assert.equal(dmg.attrs.href, `${BASE}download/v9.9.0/Coverly.ROMs_9.9.0_universal.dmg`);
    // 18,934,784 + 1,048,576 bytes = 19.1 MiB.
    assert.ok(dmg.textContent.includes(lang === 'es' ? '19,1 MiB' : '19.1 MiB'), dmg.textContent);
    const msi = links.find((a) => a.attrs.href.endsWith('.msi'));
    assert.equal(msi.attrs.href, `${BASE}download/v9.9.0/Coverly.ROMs_9.9.0_x64_en-US.msi`);
    for (const v of rel('version')) assert.equal(v.textContent, '9.9.0');
    for (const d of rel('date')) assert.equal(d.textContent, lang === 'es' ? '9 de junio de 2027' : '9 June 2027');
    for (const n of rel('notes')) assert.equal(n.attrs.href, `${BASE}tag/v9.9.0`);
  });
}

test('the same, an older or a malformed release changes nothing', async () => {
  const html = filled(true, 'en');
  const before = dom(html).els.map((e) => [e.attrs.href, e.textContent]);
  const cases = [
    FIXTURE,
    release('9.8.6'),
    release('9.9.0', (r) => ({ ...r, prerelease: true })),
    release('9.9.0', (r) => ({ ...r, draft: true })),
    release('9.9.0', (r) => ({ ...r, tag_name: '9.9.0' })),
    release('9.9.0', (r) => ({ ...r, html_url: 'https://example.com/' })),
    release('10.0.0-beta.1'),
  ];
  for (const body of cases) {
    const { page } = await run(html, { status: 200, body });
    assert.deepEqual(page.els.map((e) => [e.attrs.href, e.textContent]), before, body.tag_name);
  }
});

test('one file missing, empty, not uploaded or elsewhere changes nothing at all', async () => {
  const html = filled(true, 'es');
  const before = dom(html).els.map((e) => [e.attrs.href, e.textContent]);
  const edits = [
    (a) => a.name.endsWith('arm64.deb') ? null : a,
    (a) => a.name === 'SHA256SUMS' ? null : a,
    (a) => a.name.endsWith('.msi') ? { ...a, size: 0 } : a,
    (a) => a.name.endsWith('.dmg') ? { ...a, state: 'starter' } : a,
    (a) => a.name.endsWith('linux-aarch64') ? { ...a, browser_download_url: 'https://example.com/x' } : a,
  ];
  for (const edit of edits) {
    const body = release('9.9.0', (r) => ({ ...r, assets: r.assets.map(edit).filter(Boolean) }));
    const { page } = await run(html, { status: 200, body });
    assert.deepEqual(page.els.map((e) => [e.attrs.href, e.textContent]), before);
  }
});

test('an API error, a rate limit or no network leaves the built page', async () => {
  const html = filled(true, 'es');
  const before = dom(html).els.map((e) => [e.attrs.href, e.textContent]);
  for (const answer of [{ status: 403 }, { status: 500 }, new TypeError('offline')]) {
    const { page, store } = await run(html, answer);
    assert.deepEqual(page.els.map((e) => [e.attrs.href, e.textContent]), before);
    assert.deepEqual(store, {}, 'a failure is not cached');
  }
});

test('before the first release, a published one shows the hidden note', async () => {
  for (const lang of ['es', 'en']) {
    const html = filled(false, lang);
    const quiet = await run(html, { status: 404 });
    assert.ok(quiet.page.els.find((e) => 'data-rel-new' in e.attrs).hidden, 'still hidden on 404');
    const { page, rel } = await run(html, { status: 200, body: release('0.1.0') });
    assert.equal(page.els.find((e) => 'data-rel-new' in e.attrs).hidden, false);
    assert.equal(rel('version')[0].textContent, '0.1.0');
    assert.equal(rel('notes')[0].attrs.href, `${BASE}tag/v0.1.0`);
    for (const e of page.els) {
      if ('data-rel-new' in e.attrs) assert.equal(e.hidden, false, e.textContent);
      if ('data-rel-old' in e.attrs) assert.equal(e.hidden, true, e.textContent);
    }
    const offer = page.els.find((e) => 'data-rel-file' in e.attrs);
    assert.ok(offer.hidden, 'no installer offered to an unknown system');
    assert.equal(offer.attrs.href, `${BASE}latest`);
  }
});

test('before the first release, the installer for this system is offered', async () => {
  const cases = [
    ['mac', '', 'Coverly.ROMs_0.1.0_universal.dmg', 'macOS', ['19,1', '19.1']],
    ['win', '', 'Coverly.ROMs_0.1.0_x64-setup.exe', 'Windows', ['10,1', '10.1']],
    ['linux', 'X11; Linux x86_64', 'Coverly.ROMs_0.1.0_amd64.AppImage', 'Linux x64', ['82,0', '82.0']],
    ['linux', 'X11; Linux aarch64', 'Coverly.ROMs_0.1.0_aarch64.AppImage', 'Linux ARM64', ['81,0', '81.0']],
  ];
  for (const lang of ['es', 'en']) {
    const html = filled(false, lang);
    for (const [os, ua, file, label, mib] of cases) {
      const { page, rel } = await run(html, { status: 200, body: release('0.1.0') }, {}, os, ua);
      const offer = page.els.find((e) => 'data-rel-file' in e.attrs);
      assert.equal(offer.hidden, false, `${lang} ${os} ${ua}`);
      assert.equal(offer.attrs.href, `${BASE}download/v0.1.0/${file}`);
      assert.equal(rel('os')[0].textContent, label);
      assert.equal(rel('mib')[0].textContent, mib[lang === 'es' ? 0 : 1]);
    }
    const missing = release('0.1.0', (r) => ({ ...r, assets: r.assets.filter((a) => !a.name.endsWith('.dmg')) }));
    const { page } = await run(html, { status: 200, body: missing }, {}, 'mac');
    assert.ok(page.els.find((e) => 'data-rel-file' in e.attrs).hidden, 'a missing file is not offered');
    assert.equal(page.els.find((e) => 'data-rel-new' in e.attrs).hidden, false, 'the release is still announced');
  }
});

test('the answer is cached for the tab, trimmed to what the page reads', async () => {
  const html = filled(true, 'en');
  const first = await run(html, { status: 200, body: release('9.9.0') });
  const cached = JSON.parse(first.store['cr-release']);
  assert.equal(cached.r.tag_name, 'v9.9.0');
  assert.deepEqual(Object.keys(cached.r.assets[0]).sort(), ['browser_download_url', 'name', 'size', 'state']);
  assert.equal(cached.r.body, undefined);
  const second = await run(html, new Error('must not fetch'), first.store);
  assert.equal(second.calls.length, 0);
  assert.equal(second.rel('version')[0].textContent, '9.9.0');
  const stale = { 'cr-release': JSON.stringify({ ...cached, t: Date.now() - 11 * 60 * 1000 }) };
  const third = await run(html, { status: 200, body: release('9.9.1') }, stale);
  assert.equal(third.calls.length, 1);
  assert.equal(third.rel('version')[0].textContent, '9.9.1');
});
