import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogTitle,
  AlertDialogTrigger,
} from './alert-dialog';
import { Button } from './button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
} from './dialog';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetTitle,
  SheetTrigger,
} from './sheet';

describe('Dialog', () => {
  it('is named and described, closes on Escape and returns focus', async () => {
    render(
      <Dialog>
        <DialogTrigger asChild>
          <Button>Open reports</Button>
        </DialogTrigger>
        <DialogContent>
          <DialogTitle>Reports</DialogTitle>
          <DialogDescription>Export the last run.</DialogDescription>
        </DialogContent>
      </Dialog>,
    );
    const opener = screen.getByRole('button', { name: 'Open reports' });
    await userEvent.click(opener);
    const dialog = screen.getByRole('dialog', { name: 'Reports' });
    expect(dialog).toHaveAccessibleDescription('Export the last run.');
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(opener).toHaveFocus();
  });

  it('labels the close button through i18n', async () => {
    render(
      <I18nProvider locale="es">
        <Dialog defaultOpen>
          <DialogContent>
            <DialogTitle>Informes</DialogTitle>
            <DialogDescription>-</DialogDescription>
          </DialogContent>
        </Dialog>
      </I18nProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Cerrar' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });
});

describe('AlertDialog', () => {
  it('is an alertdialog that starts on the safe action', async () => {
    const onConfirm = vi.fn();
    render(
      <AlertDialog>
        <AlertDialogTrigger asChild>
          <Button variant="destructive">Delete profile</Button>
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogTitle>Delete this profile?</AlertDialogTitle>
          <AlertDialogDescription>
            Your covers are not touched.
          </AlertDialogDescription>
          <AlertDialogCancel>Keep it</AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={onConfirm}>
            Delete
          </AlertDialogAction>
        </AlertDialogContent>
      </AlertDialog>,
    );
    await userEvent.click(
      screen.getByRole('button', { name: 'Delete profile' }),
    );
    expect(
      screen.getByRole('alertdialog', { name: 'Delete this profile?' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Keep it' })).toHaveFocus();
    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });
});

describe('Sheet', () => {
  it('opens as a modal dialog docked to the side', async () => {
    render(
      <Sheet>
        <SheetTrigger>Jobs</SheetTrigger>
        <SheetContent side="right">
          <SheetTitle>Jobs</SheetTitle>
          <SheetDescription>Running and past runs.</SheetDescription>
        </SheetContent>
      </Sheet>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Jobs' }));
    const sheet = screen.getByRole('dialog', { name: 'Jobs' });
    expect(sheet.className).toContain('slide-in-from-right');
    await userEvent.click(screen.getByRole('button', { name: 'Close' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });
});
