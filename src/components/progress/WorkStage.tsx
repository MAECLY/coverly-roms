import type { ReactNode } from 'react';

import { SHELF_Y, VIEW } from './stageArt';
import './progress.css';

export interface WorkStageProps {
  /** What the drawing shows, read by assistive tech in place of the art. */
  label: string;
  children: ReactNode;
  /** HTML laid over the viewBox (the SystemIcon plate); never inside the svg. */
  overlay?: ReactNode;
}

/** The SVG frame every stage shares: shelf rule with its Buckram cap, and an
 * HTML overlay laid over the viewBox (SystemIcon never in foreignObject, D9). */
export function WorkStage({ label, children, overlay }: WorkStageProps) {
  return (
    <div
      role="img"
      aria-label={label}
      className="ws relative w-full max-w-[480px]"
      style={{ aspectRatio: `${VIEW.w} / ${VIEW.h}` }}
    >
      <svg
        aria-hidden="true"
        focusable="false"
        viewBox={`0 0 ${VIEW.w} ${VIEW.h}`}
        className="absolute inset-0 h-full w-full overflow-visible"
      >
        <line
          className="ws-shelf"
          x1="0"
          x2={VIEW.w}
          y1={SHELF_Y}
          y2={SHELF_Y}
        />
        <line
          className="ws-cap"
          x1="0"
          x2="48"
          y1={SHELF_Y + 1.5}
          y2={SHELF_Y + 1.5}
        />
        {children}
      </svg>
      {overlay}
    </div>
  );
}
