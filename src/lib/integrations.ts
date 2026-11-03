/**
 * Typed loader of `integrations/integrations.json` (spec D2, D3): the one
 * registry of every service Coverly talks to on the user's behalf. It
 * feeds the «Integraciones» tab, the settings search and the notices; the
 * Rust side (`coverly_core::integrations`) reads the same file and owns the
 * coverage test and the card state of credentialed services.
 */
import raw from '../../integrations/integrations.json';
import {
  CREDENTIAL_KEYS,
  type CatalogStatus,
  type CredentialKey,
  type FieldInput,
  type IntegrationState,
  type SourceId,
} from '@/ipc/types';

export type { FieldInput, IntegrationState };

export type IntegrationId =
  | 'steamgriddb'
  | 'screenscraper'
  | 'switch-catalog'
  | 'updates'
  // Planned entries and future ones: the registry is the list.
  | (string & {});

export type IntegrationKind = 'api-key' | 'account' | 'dataset' | 'link';
export type IntegrationGroup =
  'art' | 'names' | 'achievements' | 'catalogs' | 'updates';
export type IntegrationStatus = 'working' | 'planned';
export type IntegrationTest = 'steamgriddb' | 'screenscraper';

export interface IntegrationField {
  key: CredentialKey;
  input: FieldInput;
  required: boolean;
}

export interface Integration {
  id: IntegrationId;
  /** Display name; for `switch-catalog` and `updates` only the Spanish
   * fallback (the UI reads `integrations.service.<id>.name`). */
  name: string;
  kind: IntegrationKind;
  group: IntegrationGroup;
  status: IntegrationStatus;
  fields: IntegrationField[];
  test: IntegrationTest | null;
  /** The service's official account or API page. */
  help_url: string | null;
  /** Ids in `credits/credits.json`. */
  credits: string[];
  /** The art source this integration unlocks. */
  source: SourceId | null;
  /** Settings path of its place elsewhere (Fuentes order, General). */
  setting: string | null;
  /** Features it powers (`fill`, `organize-identify`, …). */
  powers: string[];
  /** Anchor of its roadmap section (planned entries). */
  roadmap: string | null;
  /** What a planned service will ask for (display only). */
  needs: FieldInput[];
}

/** The keys of a record that must name every member of `T`: the
 * compiler then flags an allow-list that misses a new member. */
function allOf<T extends string>(r: Record<T, true>): readonly string[] {
  return Object.keys(r);
}

const KINDS = allOf<IntegrationKind>({
  'api-key': true,
  account: true,
  dataset: true,
  link: true,
});
const GROUPS = allOf<IntegrationGroup>({
  art: true,
  names: true,
  achievements: true,
  catalogs: true,
  updates: true,
});
const STATUSES = allOf<IntegrationStatus>({ working: true, planned: true });
const INPUTS = allOf<FieldInput>({
  user: true,
  password: true,
  'api-key': true,
  'client-id': true,
  secret: true,
});
const TESTS = allOf<IntegrationTest>({
  steamgriddb: true,
  screenscraper: true,
});
const SOURCES = allOf<SourceId>({
  libretro: true,
  steamgriddb: true,
  screenscraper: true,
  embedded: true,
});

function optString(v: unknown): boolean {
  return v === undefined || v === null || typeof v === 'string';
}

function optOneOf(v: unknown, of: readonly string[]): boolean {
  return v === undefined || v === null || of.includes(v as string);
}

function optList(v: unknown, ok: (x: unknown) => boolean): boolean {
  return v === undefined || (Array.isArray(v) && v.every(ok));
}

function isField(v: unknown): boolean {
  const f = v as Partial<IntegrationField> | null;
  return (
    typeof f === 'object' &&
    f !== null &&
    (CREDENTIAL_KEYS as readonly string[]).includes(f.key as string) &&
    INPUTS.includes(f.input as string) &&
    typeof f.required === 'boolean'
  );
}

/** Checks the shape of a parsed registry and fills in absent arrays and
 * nulls; throws naming the entry. The Rust side checks the rest. */
export function toIntegrations(file: unknown): Integration[] {
  const entries = (file as { entries?: unknown } | null)?.entries;
  if (!Array.isArray(entries)) throw new Error('integrations: no entries');
  return entries.map((e: unknown, i) => {
    const r = (e ?? {}) as Record<string, unknown>;
    const ok =
      typeof r.id === 'string' &&
      typeof r.name === 'string' &&
      KINDS.includes(r.kind as string) &&
      GROUPS.includes(r.group as string) &&
      STATUSES.includes(r.status as string) &&
      optList(r.fields, isField) &&
      optOneOf(r.test, TESTS) &&
      optString(r.help_url) &&
      optList(r.credits, (x) => typeof x === 'string') &&
      optOneOf(r.source, SOURCES) &&
      optString(r.setting) &&
      optList(r.powers, (x) => typeof x === 'string') &&
      optString(r.roadmap) &&
      optList(r.needs, (x) => INPUTS.includes(x as string));
    if (!ok)
      throw new Error(`integrations: entry ${String(r.id ?? i)} is malformed`);
    return {
      id: r.id as string,
      name: r.name as string,
      kind: r.kind as IntegrationKind,
      group: r.group as IntegrationGroup,
      status: r.status as IntegrationStatus,
      fields: (r.fields as IntegrationField[] | undefined) ?? [],
      test: (r.test as IntegrationTest | undefined) ?? null,
      help_url: (r.help_url as string | undefined) ?? null,
      credits: (r.credits as string[] | undefined) ?? [],
      source: (r.source as SourceId | undefined) ?? null,
      setting: (r.setting as string | undefined) ?? null,
      powers: (r.powers as string[] | undefined) ?? [],
      roadmap: (r.roadmap as string | undefined) ?? null,
      needs: (r.needs as FieldInput[] | undefined) ?? [],
    };
  });
}

/** Every entry of the registry, in page order. */
export const INTEGRATIONS: readonly Integration[] = toIntegrations(raw);

/** The cards of the tab, in page order. */
export const WORKING: readonly Integration[] = INTEGRATIONS.filter(
  (e) => e.status === 'working',
);

/** The «Próximamente» list. */
export const PLANNED: readonly Integration[] = INTEGRATIONS.filter(
  (e) => e.status === 'planned',
);

/** The entry with this id. */
export function integration(id: string): Integration | undefined {
  return INTEGRATIONS.find((e) => e.id === id);
}

/** The working integration that unlocks the art source `source`. */
export function integrationForSource(
  source: SourceId,
): Integration | undefined {
  return WORKING.find((e) => e.source === source);
}

/** The DOM id of a card: deep links and the search land on it. */
export function integrationDomId(id: string): string {
  return `integration-${id}`;
}

/** The Switch catalogue card's state (spec D23, plan IP-1): mapped here
 * from the existing `CatalogStatus` rather than computed in Rust. */
export interface CardState {
  state: IntegrationState;
  /** A word that replaces the generic tag: the catalogue's («Pendiente»,
   * «Descargando», «Listo») or the updater's («Al abrir», «Manual»). */
  label?: 'pending' | 'downloading' | 'ready' | 'on-launch' | 'manual';
  progress?: { done: number; total: number | null };
  /** The failed catalogue state; its existing copy says why and what to do. */
  reason?: 'failed-old' | 'failed-none' | 'rejected' | 'remove-failed';
}

export function catalogCardState(s: CatalogStatus | null): CardState {
  if (s === null) return { state: 'saved' };
  switch (s.state) {
    case 'off':
      return { state: 'off' };
    case 'pending':
      return { state: 'saved', label: 'pending' };
    case 'downloading':
      return {
        state: 'saved',
        label: 'downloading',
        progress: { done: s.done, total: s.total },
      };
    case 'ready':
      return { state: 'connected', label: 'ready' };
    case 'failed-old':
    case 'failed-none':
    case 'rejected':
    case 'remove-failed':
      return { state: 'error', reason: s.state };
  }
}

/** The name shown for an entry: its catalogue copy when it has one
 * (`integrations.service.<id>.name`: «Catálogo de Switch»), else the
 * registry's name (a service name is the same in every language). */
export function integrationName(
  entry: Integration,
  t: (key: string) => string,
): string {
  const key = `integrations.service.${entry.id}.name`;
  const text = t(key);
  return text === key ? entry.name : text;
}

/** The Actualizaciones card's tag: it has nothing to connect to or store,
 * so it says whether Coverly looks for a new version on launch (D24). */
export function updatesCardState(checkOnLaunch: boolean): CardState {
  return checkOnLaunch
    ? { state: 'connected', label: 'on-launch' }
    : { state: 'off', label: 'manual' };
}
