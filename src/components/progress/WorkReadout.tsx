import type { ReactNode } from 'react';

import { Progress } from '@/components/ui/progress';
import { useT } from '@/i18n/useT';

export interface WorkReadoutProps {
  bar: {
    value: number | null;
    max: number;
    slots?: number;
    tone?: 'primary' | 'warning';
  };
  /** The bar's accessible name. */
  label: string;
  /** «312,4 MB de 591,9 MB» or «Carpeta 12 de 40». */
  amount?: ReactNode;
  /** Plain text of `amount`, used as the bar's value text. */
  amountText?: string;
  rate?: ReactNode;
  /** undefined: not applicable; null: still measuring; else the wording. */
  eta?: ReactNode | null;
  counters?: ReactNode[];
  action?: ReactNode;
}

/**
 * The progress readout shared by every long job (spec §6.4): a slot bar,
 * then «amount · rate · time left» and the counters on one row that wraps,
 * then the action. The row is not a live region; the sentence above it is.
 */
export function WorkReadout({
  bar,
  label,
  amount,
  amountText,
  rate,
  eta,
  counters = [],
  action,
}: WorkReadoutProps) {
  const t = useT();
  const left = [
    amount,
    rate,
    eta === null ? t('organize.progress.measuring') : eta,
  ].filter((x) => x !== undefined && x !== null);
  return (
    <div className="grid w-full gap-2">
      <Progress
        {...bar}
        aria-label={label}
        {...(amountText ? { getValueLabel: () => amountText } : {})}
      />
      <div
        aria-live="off"
        className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 text-dense tabular-nums text-cr-text-secondary"
      >
        <p className="m-0 flex flex-wrap gap-x-2">
          {left.map((x, i) => (
            <span key={i}>
              {i > 0 && <span aria-hidden> · </span>}
              {x}
            </span>
          ))}
        </p>
        {counters.length > 0 && (
          <p className="m-0 flex flex-wrap gap-x-2">
            {counters.map((c, i) => (
              <span key={i}>
                {i > 0 && <span aria-hidden> · </span>}
                {c}
              </span>
            ))}
          </p>
        )}
      </div>
      {action && <div>{action}</div>}
    </div>
  );
}
