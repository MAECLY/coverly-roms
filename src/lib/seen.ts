/**
 * «Nuevo» (addon views D18): an update or DLC is new when its `rel_path`
 * was not in this library's seen set at the previous open.
 *
 * There is no scan history and mtimes are no signal (copies keep them),
 * so each viewer keeps the set in browser storage, per library root, for
 * the 20 most recent libraries and at most 50 000 paths per library (above
 * that, «Nuevo» is off for that library). The first open seeds the set and
 * tags nothing; storage that throws or is missing tags nothing. A
 * convenience only: nothing crosses the bridge.
 */
import type { Game } from './games';

export const SEEN_KEY = 'coverly.seen.v1';
export const MAX_SEEN_LIBRARIES = 20;
export const MAX_SEEN_PATHS = 50_000;

interface SeenFile {
  /** Library roots, least recent first. */
  order: string[];
  byRoot: Record<string, string[]>;
}

/**
 * What each library had at the start of this session (`null`: «Nuevo» is
 * off for it). Read once per open, so the tags hold until the next open
 * even though the stored set is updated right away.
 */
const session = new Map<string, ReadonlySet<string> | null>();

/** Forgets this session's baselines, as if the app were opened again. */
export function resetSeenSession(): void {
  session.clear();
}

function defaultStorage(): Storage | undefined {
  try {
    return typeof localStorage === 'undefined' ? undefined : localStorage;
  } catch {
    return undefined;
  }
}

function hasOwn(o: object, k: string): boolean {
  return Object.prototype.hasOwnProperty.call(o, k);
}

/** The stored file, an empty one when absent or corrupt, `null` when the
 * storage cannot be read. */
function readFile(storage: Storage): SeenFile | null {
  let raw: string | null;
  try {
    raw = storage.getItem(SEEN_KEY);
  } catch {
    return null;
  }
  const empty: SeenFile = { order: [], byRoot: {} };
  if (raw === null) return empty;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null) return empty;
    const { order, byRoot } = parsed as Partial<SeenFile>;
    const clean: SeenFile = { order: [], byRoot: {} };
    if (typeof byRoot === 'object' && byRoot !== null)
      for (const [root, paths] of Object.entries(byRoot))
        if (root !== '__proto__' && Array.isArray(paths))
          clean.byRoot[root] = paths.filter(
            (p): p is string => typeof p === 'string',
          );
    if (Array.isArray(order))
      for (const root of order)
        if (
          typeof root === 'string' &&
          hasOwn(clean.byRoot, root) &&
          !clean.order.includes(root)
        )
          clean.order.push(root);
    for (const root of Object.keys(clean.byRoot))
      if (!clean.order.includes(root)) clean.order.unshift(root);
    return clean;
  } catch {
    return empty;
  }
}

function writeFile(storage: Storage, file: SeenFile): void {
  const drop = file.order.slice(
    0,
    Math.max(0, file.order.length - MAX_SEEN_LIBRARIES),
  );
  for (const root of drop) delete file.byRoot[root];
  file.order = file.order.slice(drop.length);
  try {
    storage.setItem(SEEN_KEY, JSON.stringify(file));
  } catch {
    // «Nuevo» is a convenience only.
  }
}

function childPaths(games: readonly Game[]): Set<string> {
  const paths = new Set<string>();
  for (const g of games) for (const c of g.children) paths.add(c.rel_path);
  return paths;
}

/**
 * The `rel_path`s of `games`' children to tag «Nuevo» in the library at
 * `root`. Saves the current set as seen for the next open.
 */
export function newChildren(
  root: string,
  games: readonly Game[],
  storage: Storage | undefined = defaultStorage(),
): ReadonlySet<string> {
  const current = childPaths(games);
  const tooMany = current.size > MAX_SEEN_PATHS;
  if (!storage || tooMany) {
    session.set(root, null);
    if (storage && tooMany) {
      const file = readFile(storage);
      if (file && hasOwn(file.byRoot, root)) {
        delete file.byRoot[root];
        file.order = file.order.filter((r) => r !== root);
        writeFile(storage, file);
      }
    }
    return new Set();
  }

  const file = readFile(storage);
  if (!session.has(root)) {
    if (file === null) session.set(root, null);
    else
      session.set(
        root,
        // First open: today's children are the baseline, so nothing is
        // tagged now and a child that appears later in the session is.
        new Set(hasOwn(file.byRoot, root) ? file.byRoot[root] : current),
      );
  }
  const baseline = session.get(root) ?? null;
  if (baseline === null || file === null) return new Set();

  file.byRoot[root] = [...current];
  file.order = [...file.order.filter((r) => r !== root), root];
  writeFile(storage, file);

  const tagged = new Set<string>();
  for (const p of current) if (!baseline.has(p)) tagged.add(p);
  return tagged;
}
