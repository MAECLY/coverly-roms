/** The groups list of the duplicates screen; see `groupNav.ts`. */
import { useRef, useState, type KeyboardEvent, type ReactNode } from 'react';

import {
  GroupNavContext,
  dialogOpen,
  stepGroup,
  type GroupRegistry,
} from '@/components/dedupe/groupNav';
import { isTyping, letterOf } from '@/lib/keyboard';

/** The list of system sections: J next game, K previous game. */
export function GroupNavList({
  disabled = false,
  className,
  children,
}: {
  /** While applying, nothing moves. */
  disabled?: boolean;
  className?: string;
  children: ReactNode;
}) {
  const [registry] = useState<GroupRegistry>(() => new Map());
  const ref = useRef<HTMLDivElement>(null);

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (disabled || e.defaultPrevented) return;
    if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    if (isTyping(e.target) || dialogOpen()) return;
    const letter = letterOf(e);
    const step = letter === 'j' ? 1 : letter === 'k' ? -1 : 0;
    if (!step || !ref.current) return;
    if (stepGroup(ref.current, registry, step)) e.preventDefault();
  }

  return (
    <GroupNavContext.Provider value={registry}>
      {/* The list only delegates J/K from the controls inside it. */}
      {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions */}
      <div ref={ref} className={className} onKeyDown={onKeyDown}>
        {children}
      </div>
    </GroupNavContext.Provider>
  );
}
