/**
 * Fills in the organise fields Rust leaves out of its JSON (zero counts,
 * empty lists, fields older plans predate), so the app sees complete
 * values whatever produced them.
 */
import type {
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
  return {
    ...view,
    plan: { ...view.plan, items: view.plan.items.map(normalizeOrganizeItem) },
  };
}

export function normalizeOrganizeReport(r: OrganizeReportWire): OrganizeReport {
  return { ...r, extracted: r.extracted ?? 0 };
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
