/** Shared Tailwind classes of the organise screen (brand tokens only). */

/** A section heading (a shelf label), like the settings groups. */
export const sectionTitle = 'm-0 text-dense font-semibold text-cr-text';

/** Secondary text under a row, a control or a group. */
export const hintText = 'm-0 text-caption text-cr-text-secondary';

/** A bordered panel: one system folder, one review bucket, the offer. */
export const panel = 'rounded-lg border border-cr-border bg-cr-surface';

/** A file list: rows split by hairlines. */
export const fileList = 'm-0 list-none divide-y divide-cr-border p-0';

/** One file in a list. */
export const fileRow = 'grid gap-1 px-3 py-2 text-ui text-cr-text';

/** File paths and names: the label-maker readout. */
export const pathText = 'font-mono text-caption [overflow-wrap:anywhere]';

/** Trigger of a collapsible group: a ghost row with a rotating chevron. */
export const groupTrigger =
  'group h-auto min-h-control w-full justify-start gap-2 whitespace-normal rounded-lg px-3 py-2 text-left';

export const chevron =
  'transition-transform duration-fast ease-standard group-data-[state=open]:rotate-90 motion-reduce:transition-none';

/** Lists inside a group scroll on their own past this height. */
export const listViewport = 'max-h-[28rem]';
