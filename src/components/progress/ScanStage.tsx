import { FOLDER_PATH, FTAB_PATH, SHELF_Y, SLEEVE } from './stageArt';
import { WorkStage } from './WorkStage';

/** The reading line every folder passes under. */
const LINE_X = 180;
/** Folder glyphs at half the kit size (24 × 18), one pitch apart. Glyph `k`
 * stands for folder `k` and keeps its place on the belt, so the belt only
 * ever slides forward; `behind` folders already read and `ahead` still to
 * read stay drawn around the one under the line. */
const GLYPH = {
  scale: 0.5,
  w: 24,
  pitch: 24,
  y: 40,
  behind: 4,
  ahead: 4,
} as const;
/** One solid slot per this many ROMs found, up to the cap. */
export const ROMS_PER_SLOT = 25;
export const SLOT_CAP = 12;
const SLOT_GAP = 4;
/** The row of slots is centred under the reading line. */
const ROW_X = LINE_X - (SLOT_CAP * SLEEVE.w + (SLOT_CAP - 1) * SLOT_GAP) / 2;

export interface ScanStageProps {
  /** Top-level folders read so far. */
  done: number;
  total: number;
  roms: number;
  label: string;
}

/**
 * The scan's stage (spec §6.5): small folders pass under a dashed reading
 * line, and every 25 ROMs found add one solid slot to a row on the shelf.
 * The belt's offset grows with every folder read (never wraps), so the CSS
 * slide never runs backwards. Motion lives only in CSS (`.ws-slide`), so
 * reduced motion renders the same DOM and the slide just snaps.
 */
export function ScanStage({ done, total, roms, label }: ScanStageProps) {
  const slots = Math.min(
    SLOT_CAP,
    Math.ceil(Math.max(0, roms) / ROMS_PER_SLOT),
  );
  const at = Math.max(0, Math.floor(done));
  const first = Math.max(0, at - GLYPH.behind);
  // An unknown total (0) leaves the belt open; a known one ends it.
  const last =
    total > 0 ? Math.min(at + GLYPH.ahead, total - 1) : at + GLYPH.ahead;
  const folders = Array.from(
    { length: Math.max(0, last - first + 1) },
    (_, i) => first + i,
  );
  return (
    <WorkStage label={label}>
      <g
        className="ws-slide"
        style={{ transform: `translateX(${-GLYPH.pitch * at}px)` }}
      >
        {folders.map((k) => (
          <g
            key={k}
            transform={`translate(${LINE_X - GLYPH.w / 2 + k * GLYPH.pitch} ${GLYPH.y}) scale(${GLYPH.scale})`}
          >
            <path className="ws-tab" d={FTAB_PATH} />
            <path className="ws-folder" d={FOLDER_PATH} />
          </g>
        ))}
      </g>
      <line
        className="ws-route"
        x1={LINE_X}
        x2={LINE_X}
        y1={GLYPH.y - 12}
        y2={GLYPH.y + 32}
      />
      {Array.from({ length: slots }, (_, i) => (
        <rect
          key={i}
          className="ws-sleeve is-solid"
          x={ROW_X + i * (SLEEVE.w + SLOT_GAP)}
          y={SHELF_Y - SLEEVE.h}
          width={SLEEVE.w}
          height={SLEEVE.h}
          rx="1"
        />
      ))}
    </WorkStage>
  );
}
