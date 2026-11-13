import type { GameDetailsView } from '@/ipc/types';
import type { Game } from '@/lib/games';

/** The game has something to say about versions (§8.8 2). */
export function hasVersionFacts(
  game: Game,
  details: GameDetailsView | null,
): boolean {
  if (game.isDemo || game.orphan !== null) return false;
  return (
    game.updates.length > 0 ||
    game.installed !== null ||
    game.entry.version !== undefined ||
    game.missing !== null ||
    details?.latest !== undefined
  );
}
