/** Shared geometry of the progress stages, in viewBox units (360 × 112). */
export const VIEW = { w: 360, h: 112 } as const;
export const SHELF_Y = 100;
/** Landing kit.svg#folder and #ftab path data (viewBox 48 × 36), copied (D9). */
export const FOLDER_PATH =
  'M2 6h44a2 2 0 0 1 2 2v26a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z';
export const FTAB_PATH = 'M2 0h14l4 4v2H0V2a2 2 0 0 1 2-2z';
/** 72 × 54 at this scale; the front runs from y 55 to 100. */
export const FOLDER = { x: 262, y: 46, scale: 1.5 } as const;
export const BOX = { x: 16, y: 54, w: 68, h: 46 } as const;
export const ROUTE = 'M 86 50 Q 174 4 262 50';
/** 3:4, the Slot. */
export const SLEEVE = { w: 12, h: 16 } as const;
/** Label plate on the folder front (AP-2), as % of the frame for the HTML overlay. */
export const PLATE = {
  left: (274 / 360) * 100,
  top: (66 / 112) * 100,
  width: (48 / 360) * 100,
} as const;
export const pct = (v: number, of: number) => `${(v / of) * 100}%`;
