import { FOLDER_PATH, FTAB_PATH, SHELF_Y, SLEEVE } from './stageArt';
import { WorkStage } from './WorkStage';

/** The reading line every folder passes under. */
const LINE_X = 180;
/** Folder glyphs at half the kit size (24 × 18), one pitch apart. The row
 * repeats every `cycle` folders; `lead` extra glyphs keep both sides of the
 * line filled whichever folder of the cycle is under it. */
const GLYPH = {
  scale: 0.5,
  w: 24,
  pitch: 24,
  y: 40,
  cycle: 5,
  lead: 2,
} as const;
const GLYPHS = GLYPH.cycle + 2 * GLYPH.lead;
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
 * Motion lives only in CSS (`.ws-slide`), so reduced motion renders the same
 * DOM and the slide just snaps.
 */
export function ScanStage({ done, roms, label }: ScanStageProps) {
  const slots = Math.min(
    SLOT_CAP,
    Math.ceil(Math.max(0, roms) / ROMS_PER_SLOT),
  );
  const step = Math.max(0, done) % GLYPH.cycle;
  return (
    <WorkStage label={label}>
      <g
        className="ws-slide"
        style={{ transform: `translateX(${-GLYPH.pitch * step}px)` }}
      >
        {Array.from({ length: GLYPHS }, (_, i) => (
          <g
            key={i}
            transform={`translate(${LINE_X - GLYPH.w / 2 + (i - GLYPH.lead) * GLYPH.pitch} ${GLYPH.y}) scale(${GLYPH.scale})`}
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
