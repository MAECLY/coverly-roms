import type { ElementType, HTMLAttributes } from 'react';

import { Logo } from '@/components/brand/Logo';
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
 * name ("COVERLY"), so the name is pinned with aria-label. The Lean-To mark
 * (`Logo`) leads the lockup and stays decorative.
 */
export function Wordmark({
  as: Tag = 'span',
  size = 'lg',
  className,
  ...rest
}: WordmarkProps) {
  return (
    <Tag
      className={cn(
        size === 'lg' ? 'wordmark' : 'wordmark-small',
        'inline-flex items-center gap-[0.35em]',
        className,
      )}
      aria-label="Coverly ROMs"
      {...rest}
    >
      <span aria-hidden>
        <Logo showWordmark={false} size="0.85em" />
      </span>
      <span>
        Coverly <b className="wordmark-tag">ROMs</b>
      </span>
    </Tag>
  );
}
