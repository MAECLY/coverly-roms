import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { Tabs, TabsContent, TabsList, TabsTrigger } from './tabs';
import {
  SegmentedControl,
  SegmentedControlItem,
  ToggleGroup,
  ToggleGroupItem,
} from './toggle-group';
import { Toggle } from './toggle';

describe('Tabs', () => {
  it('roves with arrows and shows the matching panel', async () => {
    render(
      <Tabs defaultValue="general" orientation="vertical">
        <TabsList aria-label="Settings">
          <TabsTrigger value="general">General</TabsTrigger>
          <TabsTrigger value="library">Library</TabsTrigger>
        </TabsList>
        <TabsContent value="general">General panel</TabsContent>
        <TabsContent value="library">Library panel</TabsContent>
      </Tabs>,
    );
    expect(screen.getByRole('tablist')).toHaveAttribute(
      'aria-orientation',
      'vertical',
    );
    screen.getByRole('tab', { name: 'General' }).focus();
    await userEvent.keyboard('{ArrowDown}');
    const library = screen.getByRole('tab', { name: 'Library' });
    expect(library).toHaveFocus();
    expect(library).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tabpanel', { name: 'Library' })).toHaveTextContent(
      'Library panel',
    );
  });
});

describe('ToggleGroup and Toggle', () => {
  it('multiple toggles report aria-pressed', async () => {
    render(
      <ToggleGroup type="multiple" aria-label="Media">
        <ToggleGroupItem value="box">Box</ToggleGroupItem>
        <ToggleGroupItem value="snap">Snap</ToggleGroupItem>
      </ToggleGroup>,
    );
    const box = screen.getByRole('button', { name: 'Box' });
    await userEvent.click(box);
    expect(box).toHaveAttribute('aria-pressed', 'true');
  });

  it('Toggle flips aria-pressed', async () => {
    render(<Toggle aria-label="Filters">F</Toggle>);
    await userEvent.click(screen.getByRole('button', { name: 'Filters' }));
    expect(screen.getByRole('button', { name: 'Filters' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
  });
});

describe('SegmentedControl', () => {
  it('is a single-choice radio group that cannot be cleared', async () => {
    const onValueChange = vi.fn();
    render(
      <SegmentedControl
        aria-label="Theme"
        defaultValue="light"
        onValueChange={onValueChange}
      >
        <SegmentedControlItem value="light">Light</SegmentedControlItem>
        <SegmentedControlItem value="dark">Dark</SegmentedControlItem>
      </SegmentedControl>,
    );
    const light = screen.getByRole('radio', { name: 'Light' });
    expect(light).toBeChecked();
    await userEvent.click(light);
    expect(onValueChange).not.toHaveBeenCalled();
    expect(light).toBeChecked();
    light.focus();
    await userEvent.keyboard('{ArrowRight}');
    expect(screen.getByRole('radio', { name: 'Dark' })).toHaveFocus();
    await userEvent.keyboard(' ');
    expect(onValueChange).toHaveBeenLastCalledWith('dark');
  });
});
