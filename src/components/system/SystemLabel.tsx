import type { JSX } from 'react';

import { cn } from '@/lib/utils';

import { SystemIcon } from './SystemIcon';

export interface SystemLabelProps {
  /** Canonical id (`SystemView.system_id`); null draws the folder icon. */
  systemId: string | null;
  /** Visible text: the system name (or the folder, where that is all a
   *  surface carries). It is the label's whole accessible name. */
  name: string;
  /** Icon height in px; defaults to the text size (1em). */
  size?: number;
  className?: string;
  /** Icon after the name (spec §A.6 rule 2); it then carries the name as
   *  its title so the tooltip names the system. */
  trailing?: boolean;
}

/** A system's icon beside its name (spec §A.3). Leading by default, so a
 *  column of names lines its icons up on one edge; the name truncates and
 *  the icon never shrinks. */
export function SystemLabel({
  systemId,
  name,
  size,
  className,
  trailing = false,
}: SystemLabelProps): JSX.Element {
  const icon = (
    <SystemIcon
      systemId={systemId}
      {...(size === undefined ? {} : { size })}
      {...(trailing ? { title: name } : {})}
    />
  );
  return (
    <span className={cn('system-label', className)}>
      {!trailing && icon}
      <span className="system-label__name">{name}</span>
      {trailing && icon}
    </span>
  );
}
