import { ArrowLeft, ArrowRight } from 'lucide-react';
import type { JSX, RefObject } from 'react';

import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useT } from '@/i18n/useT';
import { COLUMN_DEFS, moveColumn, toggleColumn } from '@/lib/detailsColumns';
import {
  DETAILS_COLUMN_IDS,
  type DetailsColumn,
  type DetailsColumnId,
} from '@/lib/viewPrefs';

export interface ColumnPickerProps {
  /** Where the menu opens (viewport px), null when closed. */
  at: { x: number; y: number; column: DetailsColumnId } | null;
  columns: readonly DetailsColumn[];
  onChange(columns: DetailsColumn[]): void;
  onClose(): void;
  /** Focus goes back here when the menu closes (the header it came from). */
  returnFocus: RefObject<HTMLElement | null>;
}

/**
 * The Details header menu (spec §B.4.2): one tick per column of the
 * catalogue, the name always ticked, then Move left / Move right for the
 * column the menu was opened on. Opened by right-click, Shift+F10 or the
 * context-menu key on a header.
 */
export function ColumnPicker({
  at,
  columns,
  onChange,
  onClose,
  returnFocus,
}: ColumnPickerProps): JSX.Element {
  const t = useT();
  const shown = new Set(columns.map((c) => c.id));
  const index = at ? columns.findIndex((c) => c.id === at.column) : -1;
  return (
    <DropdownMenu
      open={at !== null}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      {/* A zero-size anchor at the pointer: the menu opens where it was asked. */}
      <DropdownMenuTrigger asChild>
        <span
          aria-hidden
          tabIndex={-1}
          className="pointer-events-none fixed size-0"
          style={{ left: at?.x ?? 0, top: at?.y ?? 0 }}
        />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        className="max-h-[70vh] overflow-y-auto"
        onCloseAutoFocus={(ev) => {
          ev.preventDefault();
          returnFocus.current?.focus();
        }}
      >
        <DropdownMenuLabel>{t('layouts.columnsPicker')}</DropdownMenuLabel>
        {DETAILS_COLUMN_IDS.map((id) => (
          <DropdownMenuCheckboxItem
            key={id}
            checked={shown.has(id)}
            disabled={id === 'name'}
            // Several columns are usually changed in one go.
            onSelect={(ev) => ev.preventDefault()}
            onCheckedChange={() => onChange(toggleColumn(columns, id))}
          >
            {t(COLUMN_DEFS[id].labelKey)}
          </DropdownMenuCheckboxItem>
        ))}
        {at && index >= 0 && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              disabled={index === 0}
              onSelect={() => onChange(moveColumn(columns, at.column, -1))}
            >
              <ArrowLeft aria-hidden className="size-4" />
              {t('layouts.columnMoveLeft')}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={index === columns.length - 1}
              onSelect={() => onChange(moveColumn(columns, at.column, 1))}
            >
              <ArrowRight aria-hidden className="size-4" />
              {t('layouts.columnMoveRight')}
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
