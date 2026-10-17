import type { ElementType, HTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

interface WordmarkProps extends HTMLAttributes<HTMLElement> {
  /** The element to render (h1 on the start screen, h2 in About). */
  as?: ElementType;
  size?: 'lg' | 'sm';
}

/**
 * The wordmark (brand §3): `COVERLY` in Archivo wdth 75 / 800 followed by
 * `ROMs` inside a filled Manila tag. The same markup as the landing
 * (`.wordmark` + `<b>`), so both surfaces draw it identically. The product
 * name is not translated. `text-transform` leaks into Chromium's accessible
 * name ("COVERLY"), so the name is pinned with aria-label.
 */
export function Wordmark({
  as: Tag = 'span',
  size = 'lg',
  className,
  ...rest
}: WordmarkProps) {
  return (
    <Tag
      className={cn(size === 'lg' ? 'wordmark' : 'wordmark-small', className)}
      aria-label="Coverly ROMs"
      {...rest}
    >
      Coverly <b className="wordmark-tag">ROMs</b>
    </Tag>
  );
}
