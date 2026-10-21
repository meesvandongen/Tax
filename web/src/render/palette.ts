// Colours for land uses and colour ramps for data overlays.

import { LandUse } from '../sim/protocol';

export type RGB = [number, number, number];

export const GROUND: Record<number, RGB> = {
  [LandUse.Water]: [0.16, 0.36, 0.52],
  [LandUse.Wild]: [0.33, 0.5, 0.25],
  [LandUse.Farm]: [0.7, 0.66, 0.36],
  [LandUse.Vacant]: [0.52, 0.47, 0.33],
  [LandUse.Road]: [0.27, 0.27, 0.29],
  [LandUse.Residential]: [0.55, 0.57, 0.48],
  [LandUse.Commercial]: [0.5, 0.52, 0.55],
  [LandUse.Industrial]: [0.45, 0.42, 0.38],
  [LandUse.Park]: [0.3, 0.6, 0.3],
  [LandUse.Civic]: [0.62, 0.62, 0.58],
  [LandUse.Construction]: [0.6, 0.5, 0.36],
};

export const BUILDING: Record<number, RGB> = {
  [LandUse.Residential]: [0.86, 0.72, 0.56],
  [LandUse.Commercial]: [0.5, 0.66, 0.85],
  [LandUse.Industrial]: [0.62, 0.56, 0.5],
  [LandUse.Civic]: [0.95, 0.94, 0.88],
  [LandUse.Construction]: [0.95, 0.72, 0.25],
};

export const DERELICT: RGB = [0.32, 0.27, 0.23];

export function mix(a: RGB, b: RGB, t: number): RGB {
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
}

// Perceptual ramps sampled at 5 stops.
const VIRIDIS: RGB[] = [
  [0.267, 0.005, 0.329],
  [0.229, 0.322, 0.546],
  [0.128, 0.567, 0.551],
  [0.369, 0.789, 0.383],
  [0.993, 0.906, 0.144],
];
const INFERNO: RGB[] = [
  [0.001, 0.0, 0.014],
  [0.341, 0.062, 0.429],
  [0.735, 0.216, 0.33],
  [0.978, 0.557, 0.035],
  [0.988, 0.998, 0.645],
];
const DIVERGING: RGB[] = [
  [0.23, 0.3, 0.75],
  [0.55, 0.69, 0.99],
  [0.87, 0.87, 0.87],
  [0.96, 0.6, 0.48],
  [0.71, 0.02, 0.15],
];

export const RAMPS = { viridis: VIRIDIS, inferno: INFERNO, diverging: DIVERGING };
export type RampName = keyof typeof RAMPS;

export function ramp(name: RampName, t: number): RGB {
  const stops = RAMPS[name];
  const x = Math.min(0.9999, Math.max(0, t)) * (stops.length - 1);
  const k = Math.floor(x);
  return mix(stops[k], stops[k + 1], x - k);
}

export function rampCss(name: RampName): string {
  const stops = RAMPS[name].map((c, i, a) => {
    const [r, g, b] = c.map((v) => Math.round(v * 255));
    return `rgb(${r},${g},${b}) ${(i / (a.length - 1)) * 100}%`;
  });
  return `linear-gradient(90deg, ${stops.join(',')})`;
}

export interface OverlayDef {
  key: string;
  label: string;
  ramp: RampName;
  /** Use a log scale for heavy-tailed values. */
  log?: boolean;
  unit?: string;
  help: string;
}

export const OVERLAY_DEFS: OverlayDef[] = [
  { key: 'land_value', label: 'Land value', ramp: 'viridis', log: true, help: 'Assessed land value per parcel (as if vacant).' },
  { key: 'rent', label: 'Rent', ramp: 'inferno', help: 'Asking rent per dwelling per month (market estimate on empty land).' },
  { key: 'tax', label: 'Property taxes', ramp: 'inferno', log: true, help: 'Property-type taxes levied on the parcel last month.' },
  { key: 'pollution', label: 'Pollution', ramp: 'inferno', help: 'Ambient pollution from industry.' },
  { key: 'amenity', label: 'Amenity', ramp: 'viridis', help: 'Value of nearby parks, water and nature (money / month).' },
  { key: 'service', label: 'Public services', ramp: 'viridis', help: 'Public service coverage (0–1).' },
  { key: 'commute', label: 'Job access', ramp: 'viridis', help: 'Effective distance to the city’s jobs (lower is better).' },
  { key: 'price', label: 'Last price', ramp: 'viridis', log: true, help: 'Last transaction price (or assessment).' },
  { key: 'occupancy', label: 'Occupancy', ramp: 'diverging', help: 'Share of units / job slots in use.' },
  { key: 'owner_wealth', label: 'Ownership', ramp: 'viridis', log: true, help: 'Wealth of the resident owner. Grey: absentee (outside) owner; white: public.' },
  { key: 'speculation', label: 'Idle holding', ramp: 'inferno', help: 'Months the owner has chosen to hold the parcel undeveloped.' },
];
