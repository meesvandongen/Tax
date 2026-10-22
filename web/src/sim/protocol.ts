// Shared types between the simulation worker and the main thread.
// Mirrors the Rust side: sim-core/src/{types,snapshot,tax,config,metrics}.rs

export const enum LandUse {
  Water = 0,
  Wild = 1,
  Farm = 2,
  Vacant = 3,
  Road = 4,
  Residential = 5,
  Commercial = 6,
  Industrial = 7,
  Park = 8,
  Civic = 9,
  Construction = 10,
}

export const LAND_USE_NAMES = [
  'Water', 'Wild', 'Farm', 'Vacant', 'Road', 'Residential', 'Commercial', 'Industrial', 'Park', 'Civic', 'Construction',
];

// Per-cell byte layout (snapshot.rs CELL_STRIDE)
export const CELL_STRIDE = 8;
export const C_USE = 0;
export const C_FLOORS = 1;
export const C_CONDITION = 2;
export const C_OCCUPANCY = 3;
export const C_NATURE = 4;
export const C_TENTS = 5;
export const C_OWNER = 6;
export const C_FLAGS = 7;

export const FLAG_ROAD_ACCESS = 1;
export const FLAG_SPECULATIVE = 2;
export const FLAG_DERELICT = 4;
export const FLAG_WATER = 8;
export const FLAG_CON_RES = 16;
export const FLAG_CON_COM = 32;
export const FLAG_CON_IND = 64;
export const FLAG_RECENT_SALE = 128;

export type TaxSpec =
  | { kind: 'land_value'; rate: number }
  | { kind: 'property'; land_rate: number; building_rate: number }
  | { kind: 'income'; brackets: { threshold: number; rate: number }[]; include_capital: boolean }
  | { kind: 'wealth'; threshold: number; rate: number }
  | { kind: 'sales'; rate: number }
  | { kind: 'corporate'; rate: number }
  | { kind: 'payroll'; rate: number }
  | { kind: 'transfer'; rate: number }
  | { kind: 'harberger'; rate: number }
  | { kind: 'pollution'; per_unit: number }
  | { kind: 'vacancy'; rate: number }
  | { kind: 'estate'; threshold: number; rate: number }
  | { kind: 'head'; amount: number };

export type TaxKind = TaxSpec['kind'];

export interface SpendingPolicy {
  services: number;
  infrastructure: number;
  parks: number;
  dividend: number;
  welfare: number;
  max_debt_to_revenue: number;
  purchase_premium: number;
}

export interface Policy {
  taxes: TaxSpec[];
  spending: SpendingPolicy;
  zoning: boolean;
  height_limit: number;
  collective_sale_threshold: number;
}

export interface Preset {
  name: string;
  description: string;
  policy: Policy;
}

/** A subset of sim-core Metrics; all numeric fields are kept. */
export interface Metrics {
  tick: number;
  year: number;
  households: number;
  unhoused: number;
  immigrants: number;
  emigrants: number;
  wealthy_emigrants: number;
  mean_utility: number;
  employed: number;
  unemployment: number;
  mean_wage: number;
  median_income: number;
  gdp: number;
  exports: number;
  firms: number;
  gini_income: number;
  gini_wealth: number;
  top10_wealth_share: number;
  top1_wealth_share: number;
  poverty_rate: number;
  absentee_land_share: number;
  owner_occupier_share: number;
  flat_purchases: number;
  flat_sales: number;
  median_rent: number;
  rent_burden: number;
  housing_units: number;
  housing_vacancy: number;
  land_value_total: number;
  land_price_index: number;
  avg_floors: number;
  max_floors: number;
  urban_cells: number;
  idle_urban_lots: number;
  speculative_holds: number;
  derelict: number;
  sales: number;
  forced_sales: number;
  harberger_buyouts: number;
  buildings_started: number;
  green_cover: number;
  forest_cells: number;
  pollution_exposure: number;
  emissions: number;
  avg_commute: number;
  co2_index: number;
  parks: number;
  revenue: number;
  revenue_by_tax: [string, number][];
  spend_services: number;
  spend_infrastructure: number;
  spend_parks: number;
  spend_dividend: number;
  spend_welfare: number;
  spend_rebate: number;
  gov_cash: number;
  dividend_per_hh: number;
  deterred_workers: number;
  blocked_sales: number;
  floors_built: number;
  floors_no_tax: number;
  deferred_maintenance: number;
  capital_flight: number;
  land_rent_annual: number;
  public_goods_annual: number;
  [key: string]: unknown;
}

export type SimEvent =
  | { t: 'immigrate'; cell: number }
  | { t: 'emigrate'; cell: number; wealthy: boolean }
  | { t: 'sale'; cell: number; forced: boolean }
  | { t: 'harberger_buyout'; cell: number }
  | { t: 'foreclosure'; cell: number }
  | { t: 'build_start'; cell: number }
  | { t: 'build_done'; cell: number }
  | { t: 'demolish'; cell: number }
  | { t: 'firm_open'; cell: number }
  | { t: 'firm_close'; cell: number }
  | { t: 'park'; cell: number }
  | { t: 'road'; cell: number }
  | { t: 'death'; cell: number };

/** The irregular parcel mesh (sim-core/src/mesh.rs). Parcel i is quad i. */
export interface MeshData {
  /** Corner positions, (x, y) pairs in cells. */
  verts: Float32Array;
  /** Four corners per parcel, counter-clockwise in (x, y). */
  quads: Uint32Array;
  /** The parcel across each edge (edge k runs from corner k to k + 1), NONE at the border. */
  adj: Uint32Array;
  /** Terrain height (m) at each corner. */
  vertHeights: Float32Array;
  /** Parcel under each pixel of the width × height raster. */
  pixelCells: Uint32Array;
}

export const NONE = 0xffffffff;

export interface Frame {
  tick: number;
  cells: Uint8Array;
  overlays: Float32Array;
  commuters: Uint32Array;
}

// ---- messages main → worker
export type ToWorker =
  | { type: 'init'; wasmUrl: string; config: unknown; role: 'main' | 'shadow' }
  | { type: 'reset'; config: unknown }
  | { type: 'setPolicy'; policy: Policy }
  | { type: 'run'; monthsPerSecond: number }
  | { type: 'pause' }
  | { type: 'step'; months: number }
  | { type: 'inspect'; cell: number; id: number };

// ---- messages worker → main
export type FromWorker =
  | {
      type: 'ready';
      width: number;
      height: number;
      heights: Float32Array;
      mesh: MeshData;
      overlays: string[];
      presets: Preset[];
      config: { seed: number; policy: Policy; [k: string]: unknown };
    }
  | { type: 'metrics'; metrics: Metrics[] }
  | { type: 'frame'; frame: Frame; events: SimEvent[] }
  | { type: 'inspect'; id: number; data: unknown }
  | { type: 'error'; message: string }
  | { type: 'perf'; msPerMonth: number };
