import { useEffect, useRef, type ReactNode } from 'react';

import { trapTab } from '@/lib/focusTrap';

/** A modal dialog: focus moves in and back, Tab stays inside, Escape closes. */
export function Modal({
  labelledBy,
  onClose,
  className = '',
  children,
}: {
  labelledBy: string;
  onClose(): void;
  className?: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const opener =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    ref.current?.focus();
    return () => opener?.focus();
  }, []);
  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === 'Escape') onClose();
      else if (ev.key === 'Tab') trapTab(ev, ref.current);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  return (
    <div className="modal-backdrop">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        className={`modal ${className}`}
        ref={ref}
        tabIndex={-1}
      >
        {children}
      </div>
    </div>
  );
}
