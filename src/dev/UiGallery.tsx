/**
 * Dev-only specimen sheet for src/components/ui (`pnpm dev`, then open
 * `/?ui=gallery`). main.tsx imports it behind `import.meta.env.DEV`, so it is
 * not in the production bundle. Copy here is sample content, not UI strings.
 */
import {
  ChevronRight,
  LayoutGrid,
  List,
  MoreHorizontal,
  Search,
  Settings,
} from 'lucide-react';
import { useEffect, useState, type ReactNode } from 'react';

import { DupGroup } from '@/components/dedupe/DupGroup';

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { EmptyState } from '@/components/ui/empty-state';
import {
  Field,
  FieldControl,
  FieldDescription,
  FieldError,
  FieldLabel,
} from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { Kbd, KbdGroup } from '@/components/ui/kbd';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { Progress } from '@/components/ui/progress';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  NativeSelect,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from '@/components/ui/sheet';
import { Slider } from '@/components/ui/slider';
import { SlotMark } from '@/components/ui/slot-mark';
import { Spinner } from '@/components/ui/spinner';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';
import { Toaster, toast } from '@/components/ui/toaster';
import {
  SegmentedControl,
  SegmentedControlItem,
  ToggleGroup,
  ToggleGroupItem,
} from '@/components/ui/toggle-group';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { I18nProvider } from '@/i18n/I18nProvider';
import type { Locale } from '@/i18n/core';
import type { DupGroupView } from '@/ipc/dedupeTypes';
import { dedupeFixture } from '@/ipc/mock/dedupeFixture';
import { applyTheme } from '@/lib/theme';

function Specimen({
  name,
  children,
  wide = false,
}: {
  name: string;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <section
      aria-labelledby={`sp-${name}`}
      className={`flex flex-col gap-4 rounded-lg border border-cr-border bg-card p-5 ${wide ? 'lg:col-span-2' : ''}`}
    >
      <div className="flex flex-col gap-1.5">
        <h2
          id={`sp-${name}`}
          className="m-0 font-mono text-caption font-medium text-cr-text-secondary"
        >
          {name}
        </h2>
        <Separator shelf />
      </div>
      <div className="flex flex-col gap-4">{children}</div>
    </section>
  );
}

const DEDUPE = dedupeFixture();
const DEDUPE_SHOWCASE: DupGroupView[] = [
  ...['Ecco the Dolphin', 'Crash Bash', 'Tetris'].map((title) =>
    DEDUPE.groups.find((g) => g.title === title)!,
  ),
  {
    ...DEDUPE.groups.find((g) => g.title === 'GB Game 01')!,
    id: 9_999_001,
    title: 'Pokemon Blue',
    reason: { kind: 'has-save', path: 'GB/Pokemon Blue.sav' },
    members: DEDUPE.groups
      .find((g) => g.title === 'GB Game 01')!
      .members.map((m, i) => ({
        ...m,
        unit: {
          ...m.unit,
          primary: i === 0 ? 'GB/Pokemon Blue.gb' : 'GB/Pokemon Blue (1).gb',
        },
        dest: i === 0 ? null : `${DEDUPE.quarantine}/GB/Pokemon Blue (1).gb`,
        travels:
          i === 0
            ? [{ from: 'GB/Pokemon Blue.sav', kind: { kind: 'save' as const } }]
            : [
                {
                  from: 'GB/Imgs/Pokemon Blue (1).png',
                  kind: { kind: 'art' as const, media: 'cover' as const },
                },
              ],
        ...(i === 1 ? { art_leaves: { media: ['cover' as const] } } : {}),
      })),
  },
];

const Row = ({ children }: { children: ReactNode }) => (
  <div className="flex flex-wrap items-center gap-2">{children}</div>
);

export function UiGallery() {
  const [theme, setTheme] = useState<'light' | 'dark'>(() =>
    window.matchMedia?.('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light',
  );
  const [locale, setLocale] = useState<Locale>('en');
  const [progress, setProgress] = useState(62);
  const [checked, setChecked] = useState<boolean | 'indeterminate'>(
    'indeterminate',
  );
  useEffect(() => applyTheme(theme), [theme]);

  return (
    <I18nProvider locale={locale}>
      <TooltipProvider>
        <div className="min-h-full bg-background text-foreground">
          <header className="sticky top-0 z-10 border-b border-cr-border bg-background/90 backdrop-blur">
            <div className="mx-auto flex max-w-[1160px] flex-wrap items-center gap-4 px-6 py-3">
              <h1 className="m-0 flex items-center gap-2 font-display text-heading font-extrabold [font-stretch:75%]">
                COVERLY
                <span className="rounded-sm bg-cr-manila px-[0.3em] text-[0.6em] font-semibold text-cr-manila-fg [font-stretch:100%]">
                  ROMs
                </span>
                <span className="font-body text-ui font-normal text-cr-text-secondary">
                  primitives
                </span>
              </h1>
              <div className="ml-auto flex items-center gap-3">
                <SegmentedControl
                  aria-label="Theme"
                  value={theme}
                  onValueChange={(v) => setTheme(v as 'light' | 'dark')}
                  size="sm"
                >
                  <SegmentedControlItem value="light">
                    Light
                  </SegmentedControlItem>
                  <SegmentedControlItem value="dark">Dark</SegmentedControlItem>
                </SegmentedControl>
                <SegmentedControl
                  aria-label="Locale"
                  value={locale}
                  onValueChange={(v) => setLocale(v as Locale)}
                  size="sm"
                >
                  <SegmentedControlItem value="en">EN</SegmentedControlItem>
                  <SegmentedControlItem value="es">ES</SegmentedControlItem>
                </SegmentedControl>
              </div>
            </div>
          </header>

          <main className="mx-auto grid max-w-[1160px] gap-4 px-6 py-6 lg:grid-cols-2">
            <Specimen name="Duplicates: DupGroup" wide>
              <div className="rounded-lg border border-cr-border bg-cr-surface px-4">
                {DEDUPE_SHOWCASE.map((g) => (
                  <DupGroup
                    key={g.id}
                    group={g}
                    quarantine={DEDUPE.quarantine}
                    profileName="Anbernic (stock)"
                  />
                ))}
              </div>
            </Specimen>

            <Specimen name="Button" wide>
              <Row>
                <Button variant="primary">Apply 41 covers</Button>
                <Button variant="secondary">Simulate</Button>
                <Button variant="outline">Open folder</Button>
                <Button variant="ghost">Skip</Button>
                <Button variant="destructive">Delete profile</Button>
                <Button variant="link">Release notes</Button>
              </Row>
              <Row>
                <Button variant="primary" size="sm">
                  Small
                </Button>
                <Button variant="primary" size="md">
                  Medium
                </Button>
                <Button variant="primary" size="lg">
                  Large
                </Button>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button size="icon" aria-label="Settings">
                      <Settings aria-hidden />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Settings</TooltipContent>
                </Tooltip>
                <Button size="icon-sm" variant="ghost" aria-label="More">
                  <MoreHorizontal aria-hidden />
                </Button>
                <Button variant="primary" loading>
                  Saving
                </Button>
                <Button variant="primary" disabled>
                  Disabled
                </Button>
                <Button variant="outline" disabled>
                  Disabled
                </Button>
              </Row>
            </Specimen>

            <Specimen name="Field · Input · Textarea">
              <Field>
                <FieldLabel>Backup folder</FieldLabel>
                <FieldControl>
                  <Input mono defaultValue="/Volumes/SD/.coverly-backup" />
                </FieldControl>
                <FieldDescription>
                  Originals are copied here before anything is replaced.
                </FieldDescription>
              </Field>
              <Field>
                <FieldLabel>Proxy URL</FieldLabel>
                <FieldControl>
                  <Input
                    placeholder="socks5://127.0.0.1:1080"
                    defaultValue="htp:/proxy"
                  />
                </FieldControl>
                <FieldError>
                  Use http://, https:// or socks5://. Nothing was saved.
                </FieldError>
              </Field>
              <div className="relative">
                <Search
                  aria-hidden
                  className="pointer-events-none absolute left-2 top-1/2 size-4 -translate-y-1/2 text-cr-text-secondary"
                />
                <Input
                  type="search"
                  aria-label="Search the library"
                  placeholder="Search"
                  className="pl-8 pr-9"
                />
                <Kbd className="absolute right-2 top-1/2 -translate-y-1/2">
                  /
                </Kbd>
              </div>
              <Field>
                <FieldLabel>Ignored folders</FieldLabel>
                <FieldControl>
                  <Textarea mono defaultValue={'roms/bios\nroms/ports'} />
                </FieldControl>
              </Field>
            </Specimen>

            <Specimen name="Checkbox · Switch · RadioGroup">
              <Field orientation="horizontal">
                <FieldControl>
                  <Checkbox checked={checked} onCheckedChange={setChecked} />
                </FieldControl>
                <FieldLabel>All systems</FieldLabel>
                <FieldDescription>
                  Mixed: some systems are selected.
                </FieldDescription>
              </Field>
              <Row>
                <Field orientation="horizontal">
                  <FieldControl>
                    <Checkbox defaultChecked />
                  </FieldControl>
                  <FieldLabel>Box art</FieldLabel>
                </Field>
                <Field orientation="horizontal">
                  <FieldControl>
                    <Checkbox />
                  </FieldControl>
                  <FieldLabel>Screenshot</FieldLabel>
                </Field>
                <Field orientation="horizontal" disabled>
                  <FieldControl>
                    <Checkbox />
                  </FieldControl>
                  <FieldLabel>Video</FieldLabel>
                </Field>
              </Row>
              <Separator />
              <Field orientation="horizontal">
                <FieldControl>
                  <Switch defaultChecked />
                </FieldControl>
                <FieldLabel>Check for updates on launch</FieldLabel>
              </Field>
              <Field orientation="horizontal">
                <FieldControl>
                  <Switch />
                </FieldControl>
                <FieldLabel>Back up before replacing</FieldLabel>
              </Field>
              <Separator />
              <RadioGroup defaultValue="missing" aria-label="Overwrite policy">
                {[
                  ['missing', 'Only fill empty slots'],
                  ['older', 'Replace art older than the source'],
                  ['always', 'Always replace'],
                ].map(([v, l]) => (
                  <div key={v} className="flex items-center gap-2">
                    <RadioGroupItem value={v!} id={`r-${v}`} />
                    <label htmlFor={`r-${v}`} className="text-ui">
                      {l}
                    </label>
                  </div>
                ))}
              </RadioGroup>
            </Specimen>

            <Specimen name="Select · NativeSelect · Slider">
              <Field>
                <FieldLabel>Log level</FieldLabel>
                <FieldControl>
                  <NativeSelect defaultValue="info">
                    <option value="error">error</option>
                    <option value="info">info</option>
                    <option value="debug">debug</option>
                  </NativeSelect>
                </FieldControl>
              </Field>
              <Select defaultValue="es">
                <SelectTrigger aria-label="Language" className="w-48">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="auto">System</SelectItem>
                  <SelectItem value="en">English</SelectItem>
                  <SelectItem value="es">Español</SelectItem>
                </SelectContent>
              </Select>
              <Slider
                defaultValue={[70]}
                max={100}
                step={5}
                thumbLabel="Minimum match"
              />
              <Slider
                defaultValue={[20, 80]}
                max={100}
                thumbLabel={['From', 'To']}
              />
            </Specimen>

            <Specimen name="Tabs · ToggleGroup · SegmentedControl">
              <Tabs defaultValue="art">
                <TabsList aria-label="Drawer">
                  <TabsTrigger value="art">Art</TabsTrigger>
                  <TabsTrigger value="match">Match</TabsTrigger>
                  <TabsTrigger value="file">File</TabsTrigger>
                  <TabsTrigger value="off" disabled>
                    History
                  </TabsTrigger>
                </TabsList>
                <TabsContent
                  value="art"
                  className="text-ui text-cr-text-secondary"
                >
                  Box art, screenshot and marquee for this game.
                </TabsContent>
                <TabsContent
                  value="match"
                  className="text-ui text-cr-text-secondary"
                >
                  How the game was identified.
                </TabsContent>
                <TabsContent
                  value="file"
                  className="text-ui text-cr-text-secondary"
                >
                  Path and size on the card.
                </TabsContent>
              </Tabs>
              <Tabs
                defaultValue="general"
                orientation="vertical"
                className="gap-4"
              >
                <TabsList aria-label="Settings" className="w-36">
                  <TabsTrigger value="general">General</TabsTrigger>
                  <TabsTrigger value="library">Library</TabsTrigger>
                  <TabsTrigger value="sources">Sources</TabsTrigger>
                </TabsList>
                <TabsContent
                  value="general"
                  className="text-ui text-cr-text-secondary"
                >
                  Theme and language.
                </TabsContent>
                <TabsContent
                  value="library"
                  className="text-ui text-cr-text-secondary"
                >
                  Folders and scanning.
                </TabsContent>
                <TabsContent
                  value="sources"
                  className="text-ui text-cr-text-secondary"
                >
                  ScreenScraper, SteamGridDB, libretro.
                </TabsContent>
              </Tabs>
              <Row>
                <SegmentedControl aria-label="View" defaultValue="grid">
                  <SegmentedControlItem value="grid" aria-label="Grid">
                    <LayoutGrid aria-hidden />
                  </SegmentedControlItem>
                  <SegmentedControlItem value="list" aria-label="List">
                    <List aria-hidden />
                  </SegmentedControlItem>
                </SegmentedControl>
                <ToggleGroup
                  type="multiple"
                  variant="outline"
                  aria-label="Media"
                  defaultValue={['box']}
                >
                  <ToggleGroupItem value="box">Box</ToggleGroupItem>
                  <ToggleGroupItem value="snap">Snap</ToggleGroupItem>
                  <ToggleGroupItem value="logo">Logo</ToggleGroupItem>
                </ToggleGroup>
              </Row>
            </Specimen>

            <Specimen name="Dialog · AlertDialog · Sheet · Popover · DropdownMenu">
              <Row>
                <Dialog>
                  <DialogTrigger asChild>
                    <Button variant="primary">Fill missing art</Button>
                  </DialogTrigger>
                  <DialogContent>
                    <DialogHeader>
                      <DialogTitle>Fill missing art</DialogTitle>
                      <DialogDescription>
                        Here is what would change. Nothing has changed yet.
                      </DialogDescription>
                    </DialogHeader>
                    <DialogBody>
                      <p className="m-0">312 games. 41 covers missing.</p>
                    </DialogBody>
                    <DialogFooter>
                      <Button>Back</Button>
                      <Button variant="primary">Apply 41 covers</Button>
                    </DialogFooter>
                  </DialogContent>
                </Dialog>
                <AlertDialog>
                  <AlertDialogTrigger asChild>
                    <Button variant="destructive">Reset settings</Button>
                  </AlertDialogTrigger>
                  <AlertDialogContent>
                    <AlertDialogHeader>
                      <AlertDialogTitle>Reset all settings?</AlertDialogTitle>
                      <AlertDialogDescription>
                        Your covers and backups are not touched.
                      </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                      <AlertDialogCancel>Keep settings</AlertDialogCancel>
                      <AlertDialogAction variant="destructive">
                        Reset
                      </AlertDialogAction>
                    </AlertDialogFooter>
                  </AlertDialogContent>
                </AlertDialog>
                <Sheet>
                  <SheetTrigger asChild>
                    <Button>Jobs</Button>
                  </SheetTrigger>
                  <SheetContent side="right">
                    <SheetHeader>
                      <SheetTitle>Jobs</SheetTitle>
                      <SheetDescription>
                        Running and finished runs.
                      </SheetDescription>
                    </SheetHeader>
                    <Progress
                      aria-label="Applying"
                      value={progress}
                      slots={12}
                    />
                  </SheetContent>
                </Sheet>
                <Popover>
                  <PopoverTrigger asChild>
                    <Button>Replace</Button>
                  </PopoverTrigger>
                  <PopoverContent
                    aria-label="Replace cover"
                    className="grid gap-3"
                  >
                    <p className="m-0">
                      Replace the current box art? The original goes to the
                      backup folder.
                    </p>
                    <div className="flex justify-end gap-2">
                      <Button size="sm">Keep</Button>
                      <Button size="sm" variant="primary">
                        Replace
                      </Button>
                    </div>
                  </PopoverContent>
                </Popover>
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button size="icon" aria-label="More actions">
                      <MoreHorizontal aria-hidden />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuLabel>Library</DropdownMenuLabel>
                    <DropdownMenuItem>
                      Reports <DropdownMenuShortcut>⌘R</DropdownMenuShortcut>
                    </DropdownMenuItem>
                    <DropdownMenuItem>
                      Settings <DropdownMenuShortcut>⌘,</DropdownMenuShortcut>
                    </DropdownMenuItem>
                    <DropdownMenuCheckboxItem checked>
                      Show hidden files
                    </DropdownMenuCheckboxItem>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem variant="destructive">
                      Close library
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </Row>
              <Row>
                <Button
                  onClick={() =>
                    toast('41 covers applied.', {
                      action: { label: 'Undo', onClick: () => undefined },
                    })
                  }
                >
                  Show toast
                </Button>
                <Button
                  onClick={() =>
                    toast.error("Couldn't reach ScreenScraper.", {
                      description: 'Nothing was changed on your SD card.',
                    })
                  }
                >
                  Error toast
                </Button>
              </Row>
            </Specimen>

            <Specimen name="Progress · Spinner">
              <Progress aria-label="Scanning" value={progress} />
              <Progress aria-label="Waiting" size="sm" />
              <Progress aria-label="Paused" value={40} tone="warning" />
              <Progress aria-label="Applying" value={progress} slots={16} />
              <Progress aria-label="Queued" slots={8} />
              <Row>
                <Button
                  size="sm"
                  onClick={() => setProgress((p) => Math.max(0, p - 13))}
                >
                  −13
                </Button>
                <Button
                  size="sm"
                  onClick={() => setProgress((p) => Math.min(100, p + 13))}
                >
                  +13
                </Button>
                <span className="font-mono text-caption tabular-nums text-cr-text-secondary">
                  {progress}%
                </span>
                <Spinner size="sm" />
                <Spinner />
                <Spinner size="lg" className="text-primary" />
              </Row>
            </Specimen>

            <Specimen name="Badge · Kbd · SlotMark">
              <Row>
                <Badge>built-in</Badge>
                <Badge variant="outline">312 ROMs</Badge>
                <Badge variant="primary">in use</Badge>
                <Badge variant="tag" mono>
                  SGDB
                </Badge>
                <Badge variant="tag" mono>
                  libretro
                </Badge>
                <Badge variant="success">complete</Badge>
                <Badge variant="warning">paused</Badge>
                <Badge variant="danger">failed</Badge>
                <Badge variant="info">update</Badge>
              </Row>
              <Row>
                <KbdGroup>
                  <Kbd>⌘</Kbd>
                  <Kbd>K</Kbd>
                </KbdGroup>
                {['j', 'k', 's', 'a'].map((k) => (
                  <Kbd key={k}>{k}</Kbd>
                ))}
              </Row>
              <Row>
                {(
                  [
                    'missing',
                    'filled',
                    'applied',
                    'applied-run',
                    'unsupported',
                  ] as const
                ).map((s) => (
                  <span
                    key={s}
                    className="flex items-center gap-2 pr-2 text-caption text-cr-text-secondary"
                  >
                    <SlotMark state={s} size={12} />
                    <SlotMark state={s} size={16} />
                    <SlotMark state={s} size={32} label={s} />
                    {s}
                  </span>
                ))}
              </Row>
            </Specimen>

            <Specimen name="Alert">
              <Alert variant="danger">
                <AlertTitle>Couldn't write to /Volumes/SD/roms/gba.</AlertTitle>
                <AlertDescription>
                  The card may be read-only. Nothing was changed.
                </AlertDescription>
              </Alert>
              <Alert
                variant="warning"
                action={<Button size="sm">Resume</Button>}
              >
                <AlertTitle>Run paused.</AlertTitle>
                <AlertDescription>18 of 41 covers filed.</AlertDescription>
              </Alert>
              <Alert variant="info" onDismiss={() => undefined}>
                Coverly 0.2 is available.
              </Alert>
              <Alert variant="success">
                Filed 41 covers. Undo is one click away.
              </Alert>
            </Specimen>

            <Specimen name="Collapsible · ScrollArea">
              <Collapsible defaultOpen className="grid gap-2">
                <CollapsibleTrigger asChild>
                  <Button variant="ghost" size="sm" className="group w-fit">
                    <ChevronRight
                      aria-hidden
                      className="transition-transform duration-fast group-data-[state=open]:rotate-90"
                    />
                    Release notes
                  </Button>
                </CollapsibleTrigger>
                <CollapsibleContent>
                  <ScrollArea
                    aria-label="Release notes"
                    className="h-28 rounded-md border border-cr-border bg-cr-surface-sunken"
                  >
                    <ul className="m-0 grid gap-1 p-3 text-ui">
                      {Array.from({ length: 12 }, (_, i) => (
                        <li key={i}>
                          Fix {i + 1}: a calmer, more careful shelf.
                        </li>
                      ))}
                    </ul>
                  </ScrollArea>
                </CollapsibleContent>
              </Collapsible>
            </Specimen>

            <Specimen name="EmptyState" wide>
              <div className="grid gap-4 md:grid-cols-[2fr_1fr]">
                <EmptyState
                  className="rounded-lg bg-cr-surface-sunken"
                  title="No missing art."
                  description="This shelf is complete. Open another folder or check a different system."
                  actions={
                    <>
                      <Button variant="primary">Open folder</Button>
                      <Button>Show all games</Button>
                    </>
                  }
                />
                <EmptyState
                  size="sm"
                  titleAs="h3"
                  title="No recent libraries"
                  description="Folders you open show up here."
                  className="rounded-lg border border-cr-border"
                />
              </div>
            </Specimen>
          </main>
        </div>
        <Toaster />
      </TooltipProvider>
    </I18nProvider>
  );
}
