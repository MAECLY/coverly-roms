import { Component, type ErrorInfo, type ReactNode } from 'react';

import { useT } from '@/i18n/useT';

function Crashed() {
  const t = useT();
  return (
    <main className="crash">
      <p role="alert">{t('app.crashed')}</p>
      <button
        type="button"
        className="primary-button"
        onClick={() => window.location.reload()}
      >
        {t('app.reload')}
      </button>
    </main>
  );
}

/** Keeps a render error from unmounting the whole app into a blank window. */
export class ErrorBoundary extends Component<
  { children: ReactNode },
  { failed: boolean }
> {
  override state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  override componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error(error, info.componentStack);
  }

  override render() {
    return this.state.failed ? <Crashed /> : this.props.children;
  }
}
