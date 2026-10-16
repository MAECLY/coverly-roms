export interface FolderNode {
  /** `/`-separated path from the library root (`''` for the root). */
  path: string;
  name: string;
  /** Entries in this folder and below. */
  count: number;
  children: FolderNode[];
}

const collator = new Intl.Collator(undefined, {
  sensitivity: 'base',
  numeric: true,
});

function sortTree(node: FolderNode): void {
  node.children.sort((a, b) => collator.compare(a.name, b.name));
  node.children.forEach(sortTree);
}

/** The folder tree of the entries' `dir`s, with recursive counts. */
export function buildFolderTree(
  entries: readonly { dir: string }[],
): FolderNode {
  const root: FolderNode = { path: '', name: '', count: 0, children: [] };
  const index = new Map<string, FolderNode>([['', root]]);
  for (const e of entries) {
    root.count += 1;
    if (!e.dir) continue;
    let parent = root;
    let path = '';
    for (const part of e.dir.split('/')) {
      path = path ? `${path}/${part}` : part;
      let node = index.get(path);
      if (!node) {
        node = { path, name: part, count: 0, children: [] };
        index.set(path, node);
        parent.children.push(node);
      }
      node.count += 1;
      parent = node;
    }
  }
  sortTree(root);
  return root;
}
