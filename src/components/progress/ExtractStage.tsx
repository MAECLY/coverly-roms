import { useEffect, useReducer, type CSSProperties } from 'react';

import { SystemIcon } from '@/components/system/SystemIcon';
import { useReducedMotion } from '@/lib/useReducedMotion';

import {
  BOX,
  FOLDER,
  FOLDER_PATH,
  FTAB_PATH,
  PLATE,
  ROUTE,
  SLEEVE,
} from './stageArt';
import { WorkStage } from './WorkStage';

/** At most this many sleeves in flight (D10); older ones land at once. */
export const FLY_CAP = 3;
/** AP-4: a sleeve whose `animationend` never arrives still lands. */
export const SAFETY_MS = 1500;

export interface Flight {
  /** Entry index (0-based) the sleeve stands for. */
  id: number;
  /** Stagger slot for `--g`. */
  g: number;
}
export interface StageState {
  item: number | null;
  /** Entries already accounted for (landed or flying). */
  seen: number;
  landed: number;
  flying: Flight[];
}
export type StageAction =
  | {
      type: 'event';
      item: number;
      done: number;
      instant: boolean;
      cap?: number;
    }
  | { type: 'landed'; id: number }
  | { type: 'drain' };

export const initialStage: StageState = {
  item: null,
  seen: 0,
  landed: 0,
  flying: [],
};

/** D10: data first; at most `cap` sleeves in flight, the rest land at once.
 * The first event of an item is its baseline: nothing already done replays. */
export function stageReducer(s: StageState, a: StageAction): StageState {
  switch (a.type) {
    case 'event': {
      if (a.item !== s.item)
        return { item: a.item, seen: a.done, landed: 0, flying: [] };
      if (a.done <= s.seen) return s;
      const fresh = Array.from(
        { length: a.done - s.seen },
        (_, i) => s.seen + i,
      );
      if (a.instant)
        return {
          ...s,
          seen: a.done,
          landed: s.landed + s.flying.length + fresh.length,
          flying: [],
        };
      const room = Math.max(0, (a.cap ?? FLY_CAP) - s.flying.length);
      const fly = room > 0 ? fresh.slice(-room) : [];
      return {
        ...s,
        seen: a.done,
        landed: s.landed + fresh.length - fly.length,
        // Same array when nothing new flies, so the safety timer keeps running.
        flying:
          fly.length === 0
            ? s.flying
            : [...s.flying, ...fly.map((id, i) => ({ id, g: i }))],
      };
    }
    case 'landed':
      return s.flying.some((f) => f.id === a.id)
        ? {
            ...s,
            landed: s.landed + 1,
            flying: s.flying.filter((f) => f.id !== a.id),
          }
        : s;
    case 'drain':
      return s.flying.length === 0
        ? s
        : { ...s, landed: s.landed + s.flying.length, flying: [] };
  }
}

export interface ExtractStageProps {
  /** Accessible name of the drawing (`organize.progress.stageLabel`). */
  label: string;
  itemId: number;
  systemId: string | null;
  /** null: no per-entry data (older wire, single entry). */
  entries: { done: number; total: number } | null;
  phase: 'decoding' | 'checking';
  cancelling: boolean;
  /** 'move': a stack of solid sleeves, one per `done` change (§6.2 Moves). */
  variant?: 'extract' | 'move';
}

/** Where a sleeve waits at the box mouth before it leaves. */
const MOUTH = { x: BOX.x + BOX.w - 18, y: BOX.y - 14 } as const;
/** Where landed sleeves stand: behind the folder front (y 55), clear of
 * the tab (it ends at x 292), packed like spines on a shelf. */
const SHELVED = { x: FOLDER.x + 32, y: FOLDER.y - 8, step: 5, max: 6 } as const;
/** Flights aim at the middle of the stack, then settle into their slot. */
const LANDING_X = SHELVED.x + 2 * SHELVED.step;
/** Peak of the arc relative to the mouth, so the sleeve rides the route. */
const LIFT = -24;

export function ExtractStage({
  label,
  itemId,
  systemId,
  entries,
  phase,
  cancelling,
  variant = 'extract',
}: ExtractStageProps) {
  const reduced = useReducedMotion();
  const [s, dispatch] = useReducer(stageReducer, initialStage);
  const done = entries?.done ?? 0;

  useEffect(() => {
    const instant = reduced || cancelling || document.hidden;
    dispatch({
      type: 'event',
      item: itemId,
      done,
      instant,
      cap: variant === 'move' ? 1 : FLY_CAP,
    });
  }, [itemId, done, reduced, cancelling, variant]);

  useEffect(() => {
    if (cancelling || reduced) dispatch({ type: 'drain' });
  }, [cancelling, reduced]);

  useEffect(() => {
    const onHide = () => {
      if (document.hidden) dispatch({ type: 'drain' });
    };
    document.addEventListener('visibilitychange', onHide);
    return () => document.removeEventListener('visibilitychange', onHide);
  }, []);

  // Keyed on the oldest flight alone: later sleeves taking off must not
  // restart its timer, or a steady stream of events would keep it aloft.
  const oldestId = s.flying[0]?.id;
  useEffect(() => {
    if (oldestId === undefined) return;
    const id = window.setTimeout(
      () => dispatch({ type: 'landed', id: oldestId }),
      SAFETY_MS,
    );
    return () => window.clearTimeout(id);
  }, [oldestId]);

  const open = phase === 'decoding' && !cancelling;
  const waiting = entries
    ? Math.min(5, Math.max(0, entries.total - entries.done - 1))
    : 3;
  const marching =
    entries !== null &&
    phase === 'decoding' &&
    !cancelling &&
    entries.done < entries.total;

  return (
    <WorkStage
      label={label}
      overlay={
        <span
          className="ws-plate absolute flex items-center justify-center gap-1 whitespace-nowrap rounded-sm px-1 text-[11px] leading-4"
          style={{
            left: `${PLATE.left}%`,
            top: `${PLATE.top}%`,
            width: `${PLATE.width}%`,
          }}
        >
          {variant === 'extract' && (
            <SystemIcon systemId={systemId} size={14} />
          )}
          {entries && (
            <span className="mono tabular-nums">
              {entries.done}/{entries.total}
            </span>
          )}
        </span>
      }
    >
      <path className="ws-route" d={ROUTE} />
      {/* The box (or, for moves, a stack of three solid sleeves). */}
      <g key={itemId} className="ws-box-in">
        {variant === 'extract' ? (
          <>
            {Array.from({ length: waiting }, (_, i) => (
              <rect
                key={i}
                className="ws-sleeve"
                x={BOX.x + 8 + i * 10}
                y={BOX.y - 9}
                width={SLEEVE.w}
                height={SLEEVE.h}
                rx="1.5"
              />
            ))}
            <rect
              className="ws-box"
              x={BOX.x}
              y={BOX.y}
              width={BOX.w}
              height={BOX.h}
              rx="2"
            />
            <rect
              className={`ws-lid${open ? ' is-open' : ''}`}
              x={BOX.x - 2}
              y={BOX.y - 6}
              width={BOX.w + 4}
              height="7"
              rx="1.5"
            />
            {marching && (
              <rect
                className="ws-sleeve ws-march"
                x={MOUTH.x}
                y={MOUTH.y}
                width={SLEEVE.w}
                height={SLEEVE.h}
                rx="1.5"
              />
            )}
          </>
        ) : (
          [0, 1, 2].map((i) => (
            <rect
              key={i}
              className="ws-sleeve is-solid"
              x={BOX.x + 20 + i * 4}
              y={BOX.y + 20 - i * 4}
              width={SLEEVE.w}
              height={SLEEVE.h}
              rx="1.5"
            />
          ))
        )}
      </g>
      {/* Landed sleeves: spines standing behind the folder front. */}
      {Array.from({ length: Math.min(s.landed, SHELVED.max) }, (_, i) => (
        <g key={i}>
          <rect
            className="ws-sleeve is-solid"
            x={SHELVED.x + i * SHELVED.step}
            y={SHELVED.y}
            width={SLEEVE.w}
            height={SLEEVE.h}
            rx="1.5"
          />
          <rect
            className="ws-spine"
            x={SHELVED.x + i * SHELVED.step}
            y={SHELVED.y}
            width="3"
            height={SLEEVE.h}
          />
        </g>
      ))}
      {s.flying.map((f) => (
        <g
          key={f.id}
          className="ws-fly-x"
          style={
            {
              '--g': f.g,
              '--dx': `${LANDING_X - MOUTH.x}px`,
            } as CSSProperties
          }
          onAnimationEnd={(e) => {
            if (e.animationName === 'ws-fly-x')
              dispatch({ type: 'landed', id: f.id });
          }}
        >
          <g
            className="ws-fly-y"
            style={
              {
                '--g': f.g,
                '--lift': `${LIFT}px`,
                '--drop': `${SHELVED.y - MOUTH.y}px`,
              } as CSSProperties
            }
          >
            <rect
              className="ws-sleeve is-solid"
              x={MOUTH.x}
              y={MOUTH.y}
              width={SLEEVE.w}
              height={SLEEVE.h}
              rx="1.5"
            />
          </g>
        </g>
      ))}
      <g
        transform={`translate(${FOLDER.x} ${FOLDER.y}) scale(${FOLDER.scale})`}
      >
        <path
          key={s.landed}
          className={`ws-tab${s.landed > 0 ? ' ws-flash' : ''}`}
          d={FTAB_PATH}
        />
        <path className="ws-folder" d={FOLDER_PATH} />
      </g>
      {phase === 'checking' && (
        <path
          className="ws-check"
          d={`M ${FOLDER.x + 29} ${FOLDER.y + 44} l 4 4 l 9 -9`}
        />
      )}
    </WorkStage>
  );
}
