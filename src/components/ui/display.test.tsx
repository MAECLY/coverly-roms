import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';

import { Alert, AlertDescription, AlertTitle } from './alert';
import { Badge } from './badge';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from './collapsible';
import { EmptyState } from './empty-state';
import { Kbd } from './kbd';
import { Progress } from './progress';
import { ScrollArea } from './scroll-area';
import { Separator } from './separator';
import { SlotMark } from './slot-mark';
import { Spinner } from './spinner';
import { Toaster, toast } from './toaster';

describe('Progress', () => {
  it('is a progressbar with value and max', () => {
    render(<Progress aria-label="Scanning" value={30} max={120} />);
    const bar = screen.getByRole('progressbar', { name: 'Scanning' });
    expect(bar).toHaveAttribute('aria-valuenow', '30');
    expect(bar).toHaveAttribute('aria-valuemax', '120');
  });

  it('fills slots left to right', () => {
    render(<Progress aria-label="Applying" value={50} slots={4} />);
    const slots = screen.getByRole('progressbar').querySelectorAll('svg');
    expect([...slots].map((s) => s.dataset.fill)).toEqual([
      'full',
      'full',
      'empty',
      'empty',
    ]);
  });

  it('is indeterminate without a value', () => {
    render(<Progress aria-label="Waiting" />);
    expect(screen.getByRole('progressbar')).not.toHaveAttribute(
      'aria-valuenow',
    );
  });
});

describe('Spinner', () => {
  it('announces a translated label, or hides when decorative', () => {
    render(
      <I18nProvider locale="es">
        <Spinner />
        <Spinner decorative data-testid="d" />
      </I18nProvider>,
    );
    expect(screen.getByRole('status')).toHaveTextContent('Cargando');
    expect(screen.getByTestId('d')).toHaveAttribute('aria-hidden', 'true');
  });
});

describe('SlotMark and EmptyState', () => {
  it('names a slot only when given a label; photo-corners from 24px', () => {
    const { container } = render(
      <>
        <SlotMark state="missing" label="No cover" size={24} />
        <SlotMark state="filled" />
      </>,
    );
    const img = screen.getByRole('img', { name: 'No cover' });
    expect(img.querySelectorAll('path')).toHaveLength(4);
    expect(container.querySelectorAll('svg[aria-hidden="true"]')).toHaveLength(
      1,
    );
  });

  it('EmptyState renders a heading and its actions', () => {
    render(
      <EmptyState
        title="No missing art."
        description="This shelf is complete."
        actions={<button type="button">Open another folder</button>}
      />,
    );
    expect(
      screen.getByRole('heading', { level: 2, name: 'No missing art.' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Open another folder' }),
    ).toBeInTheDocument();
  });
});

describe('Alert', () => {
  it('danger is role=alert; others are status; dismiss is translated', async () => {
    const onDismiss = vi.fn();
    render(
      <>
        <Alert variant="danger">
          <AlertTitle>Couldn't reach ScreenScraper.</AlertTitle>
          <AlertDescription>
            Nothing was changed on your SD card.
          </AlertDescription>
        </Alert>
        <Alert variant="warning" onDismiss={onDismiss}>
          Paused
        </Alert>
      </>,
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Nothing was changed');
    expect(screen.getByRole('status')).toHaveTextContent('Paused');
    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});

describe('Badge, Kbd, Separator', () => {
  it('render the right elements', () => {
    render(
      <>
        <Badge variant="tag" mono>
          SGDB
        </Badge>
        <Kbd>/</Kbd>
        <Separator decorative={false} orientation="vertical" />
        <Separator data-testid="deco" />
      </>,
    );
    expect(screen.getByText('SGDB').className).toContain('bg-cr-manila');
    expect(screen.getByText('/').tagName).toBe('KBD');
    expect(screen.getByRole('separator')).toHaveAttribute(
      'aria-orientation',
      'vertical',
    );
    expect(screen.getByTestId('deco')).toHaveAttribute('role', 'none');
  });
});

describe('Collapsible', () => {
  it('toggles aria-expanded and the content', async () => {
    render(
      <Collapsible>
        <CollapsibleTrigger>Release notes</CollapsibleTrigger>
        <CollapsibleContent>Notes body</CollapsibleContent>
      </Collapsible>,
    );
    const trigger = screen.getByRole('button', { name: 'Release notes' });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByText('Notes body')).toBeNull();
    trigger.focus();
    await userEvent.keyboard('{Enter}');
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('Notes body')).toBeVisible();
  });
});

describe('ScrollArea', () => {
  it('exposes a focusable named region when labelled', () => {
    render(<ScrollArea aria-label="Release notes">text</ScrollArea>);
    const region = screen.getByRole('region', { name: 'Release notes' });
    expect(region).toHaveAttribute('tabindex', '0');
  });
});

describe('Toaster', () => {
  it('renders a toast inside a translated notifications region', async () => {
    render(
      <I18nProvider locale="es">
        <Toaster />
      </I18nProvider>,
    );
    act(() => {
      toast.success('41 portadas aplicadas.');
    });
    expect(
      await screen.findByText('41 portadas aplicadas.'),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(/Notificaciones/)).toBeInTheDocument();
  });
});
