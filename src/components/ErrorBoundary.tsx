import { Component, type ErrorInfo, type ReactNode } from 'react';

import { RotateCw } from 'lucide-react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { useT } from '@/i18n/useT';

function Crashed() {
  const t = useT();
  return (
    <main className="grid min-h-screen place-content-center justify-items-center gap-4 bg-cr-bg p-4 text-center">
      <Alert variant="danger" className="w-full max-w-md text-left">
        <p className="m-0">{t('app.crashed')}</p>
      </Alert>
      <Button
        variant="primary"
        size="lg"
        onClick={() => window.location.reload()}
      >
        <RotateCw aria-hidden />
        {t('app.reload')}
      </Button>
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
