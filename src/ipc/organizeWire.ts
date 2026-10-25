/**
 * Fills in the organise fields Rust leaves out of its JSON (zero counts,
 * empty lists, fields older plans predate), so the app sees complete
 * values whatever produced them.
 */
import type {
  DedupeJobView,
  DedupePlanView,
  DedupePlanViewWire,
  OrganizeItem,
  OrganizeItemWire,
  OrganizeJobView,
  OrganizeJobViewWire,
  OrganizePlanView,
  OrganizePlanViewWire,
  OrganizeReport,
  OrganizeReportWire,
  OrganizeUndoReport,
  OrganizeUndoReportWire,
  OrganizeUndoView,
  OrganizeUndoViewWire,
} from './types';

/** An item without `transfer` moves as it is; without `disc`, no disc id. */
export function normalizeOrganizeItem(item: OrganizeItemWire): OrganizeItem {
  return {
    ...item,
    transfer: item.transfer ?? { kind: 'as-is' },
    disc: item.disc ?? null,
  } as OrganizeItem;
}

export function normalizeOrganizePlanView(
  view: OrganizePlanViewWire,
): OrganizePlanView {
  const { plan } = view;
  return {
    ...view,
    plan: {
      ...plan,
      items: plan.items.map(normalizeOrganizeItem),
      counts: {
        ...plan.counts,
        set_aside: plan.counts.set_aside ?? 0,
        in_library: plan.counts.in_library ?? 0,
      },
      duplicates: plan.duplicates ?? [],
      set_aside_trash: plan.set_aside_trash ?? false,
    },
  };
}

export function normalizeOrganizeReport(r: OrganizeReportWire): OrganizeReport {
  return { ...r, extracted: r.extracted ?? 0, set_aside: r.set_aside ?? 0 };
}

export function normalizeOrganizeJob(
  view: OrganizeJobViewWire,
): OrganizeJobView {
  return {
    ...view,
    report: view.report ? normalizeOrganizeReport(view.report) : null,
  };
}

export function normalizeOrganizeUndoReport(
  r: OrganizeUndoReportWire,
): OrganizeUndoReport {
  return { ...r, archive_lost: r.archive_lost ?? [] };
}

export function normalizeOrganizeUndo(
  view: OrganizeUndoViewWire,
): OrganizeUndoView {
  return { ...view, report: view.report.map(normalizeOrganizeUndoReport) };
}

/** A plan made for the Trash says so; Rust leaves `trash` out when false. */
export function normalizeDedupePlanView(
  view: DedupePlanViewWire,
): DedupePlanView {
  return { ...view, trash: view.trash ?? false };
}

/** Identity today: the job view's fields are always sent. */
export function normalizeDedupeJob(view: DedupeJobView): DedupeJobView {
  return view;
}
