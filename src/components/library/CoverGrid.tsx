// Kept so existing imports work: the cover grid is now the Covers view.
export {
  CoversView as CoverGrid,
  GAP,
  TILE_H,
  TILE_W,
} from './views/CoversView';
// eslint-disable-next-line react-refresh/only-export-components -- compatibility re-export, no component state to refresh
export { entryIdOf, type EntryListProps } from './views/types';
