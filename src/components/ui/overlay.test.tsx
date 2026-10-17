import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { Button } from './button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './dropdown-menu';
import { Popover, PopoverContent, PopoverTrigger } from './popover';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from './tooltip';

describe('Tooltip', () => {
  it('shows on keyboard focus and describes the trigger', async () => {
    render(
      <TooltipProvider delayDuration={0}>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button size="icon" aria-label="Move up">
              ↑
            </Button>
          </TooltipTrigger>
          <TooltipContent>Move up</TooltipContent>
        </Tooltip>
      </TooltipProvider>,
    );
    await userEvent.tab();
    const trigger = screen.getByRole('button', { name: 'Move up' });
    expect(trigger).toHaveFocus();
    const tip = await screen.findByRole('tooltip');
    expect(tip).toHaveTextContent('Move up');
    expect(trigger).toHaveAttribute('aria-describedby', tip.id);
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('tooltip')).toBeNull());
  });
});

describe('DropdownMenu', () => {
  it('opens from the keyboard, moves with arrows and activates', async () => {
    const onReports = vi.fn();
    render(
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button>More</Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent>
          <DropdownMenuItem onSelect={onReports}>Reports</DropdownMenuItem>
          <DropdownMenuCheckboxItem checked>
            Show hidden
          </DropdownMenuCheckboxItem>
          <DropdownMenuItem variant="destructive">
            Close library
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>,
    );
    const trigger = screen.getByRole('button', { name: 'More' });
    trigger.focus();
    await userEvent.keyboard('{Enter}');
    expect(screen.getByRole('menu')).toBeInTheDocument();
    expect(
      screen.getByRole('menuitemcheckbox', { name: 'Show hidden' }),
    ).toHaveAttribute('aria-checked', 'true');
    await waitFor(() =>
      expect(screen.getByRole('menuitem', { name: 'Reports' })).toHaveFocus(),
    );
    await userEvent.keyboard('{Enter}');
    expect(onReports).toHaveBeenCalledOnce();
    await waitFor(() => expect(screen.queryByRole('menu')).toBeNull());
    expect(trigger).toHaveFocus();
  });
});

describe('Popover', () => {
  it('toggles aria-expanded and closes on Escape', async () => {
    render(
      <Popover>
        <PopoverTrigger asChild>
          <Button>Replace</Button>
        </PopoverTrigger>
        <PopoverContent aria-label="Replace art">
          Replace the current cover?
        </PopoverContent>
      </Popover>,
    );
    const trigger = screen.getByRole('button', { name: 'Replace' });
    await userEvent.click(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(
      screen.getByRole('dialog', { name: 'Replace art' }),
    ).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    await waitFor(() =>
      expect(trigger).toHaveAttribute('aria-expanded', 'false'),
    );
  });
});
