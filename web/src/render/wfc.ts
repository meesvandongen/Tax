// Wave function collapse over the parcel mesh: every building picks a roof
// module and a facade colour so that neighbours fit together, the way
// Townscaper picks its tiles.
//
// A roof module is the set of the parcel's four edges its ridge runs to
// (16 modules: none = a hipped or flat roof, two opposite edges = a plain
// gable, two adjacent = an L, three = a T, four = a cross). Constraints
// across shared walls:
//   * between attached houses of the same height, a ridge that reaches the
//     wall continues on the other side (so terraces get one roof line);
//   * attached buildings of the same use differ in colour.
// Every parcel has at most four neighbours and the ridge rule only fixes
// one bit per edge, so the constraints can always be met: collapsing never
// hits a contradiction. Parcels whose surroundings have not changed keep
// their modules, so the town does not flicker as it grows.

export const NONE = 0xffffffff;
export const UNSET = 255;

/** HAS[k]: bitmask over the 16 roof modules of those whose ridge reaches edge k. */
const HAS: number[] = [0, 1, 2, 3].map((k) => {
  let m = 0;
  for (let s = 0; s < 16; s++) if ((s >> k) & 1) m |= 1 << s;
  return m;
});
const ALL_ROOFS = 0xffff;

function popcount(x: number): number {
  let c = 0;
  while (x) {
    x &= x - 1;
    c++;
  }
  return c;
}

export function hash(i: number, k: number): number {
  let h = (i * 374761393 + k * 668265263) | 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

export interface CellRules {
  /** Signature of everything the modules depend on; -1 = not a building. */
  sig: Int32Array;
  /** Edges across which the ridge must continue. */
  link: Uint8Array;
  /** Edges across which the colour must differ. */
  differ: Uint8Array;
  /** 1 if the roof may be pitched (otherwise only module 0). */
  pitched: Uint8Array;
  /** Number of facade colours to choose from. */
  colours: Uint8Array;
  /** Relative preference for roof module `s` on parcel `i`. */
  prefer(i: number, s: number): number;
}

export class ModuleSolver {
  readonly roof: Uint8Array;
  readonly colour: Uint8Array;
  private sig: Int32Array;
  private dom: Uint16Array;
  private dirty: Uint8Array;

  constructor(
    private adj: Uint32Array,
    n: number,
  ) {
    this.roof = new Uint8Array(n).fill(UNSET);
    this.colour = new Uint8Array(n).fill(UNSET);
    this.sig = new Int32Array(n).fill(-1);
    this.dom = new Uint16Array(n);
    this.dirty = new Uint8Array(n);
  }

  private edgeTo(j: number, i: number): number {
    for (let k = 0; k < 4; k++) if (this.adj[4 * j + k] === i) return k;
    return 0;
  }

  /** Re-solve the parcels whose rules changed. Returns whether anything did. */
  solve(r: CellRules): boolean {
    const n = r.sig.length;
    const todo: number[] = [];
    this.dirty.fill(0);
    for (let i = 0; i < n; i++) {
      if (r.sig[i] === this.sig[i]) continue;
      this.sig[i] = r.sig[i];
      this.roof[i] = UNSET;
      this.colour[i] = UNSET;
      if (r.sig[i] >= 0) {
        todo.push(i);
        this.dirty[i] = 1;
      }
    }
    if (!todo.length) return false;

    // Initial domains: every module, less what solved neighbours rule out.
    for (const i of todo) {
      let d = r.pitched[i] ? ALL_ROOFS : 1;
      for (let k = 0; k < 4; k++) {
        if (!((r.link[i] >> k) & 1)) continue;
        const j = this.adj[4 * i + k];
        if (j === NONE || this.roof[j] === UNSET) continue;
        const bit = (this.roof[j] >> this.edgeTo(j, i)) & 1;
        d &= bit ? HAS[k] : ~HAS[k] & ALL_ROOFS;
      }
      this.dom[i] = d || (r.pitched[i] ? ALL_ROOFS : 1);
    }

    // Collapse the most constrained parcel first (bucket queue by entropy).
    const buckets: number[][] = Array.from({ length: 17 }, () => []);
    for (const i of todo) buckets[popcount(this.dom[i])].push(i);
    for (;;) {
      let i = -1;
      for (let b = 1; b <= 16 && i < 0; b++) {
        const q = buckets[b];
        while (q.length) {
          const c = q.pop()!;
          if (this.roof[c] === UNSET && popcount(this.dom[c]) === b) {
            i = c;
            break;
          }
        }
      }
      if (i < 0) break;
      const s = this.pick(i, this.dom[i], r);
      this.roof[i] = s;
      // Propagate: unsolved neighbours across linked edges must match.
      for (let k = 0; k < 4; k++) {
        if (!((r.link[i] >> k) & 1)) continue;
        const j = this.adj[4 * i + k];
        if (j === NONE || !this.dirty[j] || this.roof[j] !== UNSET) continue;
        const kj = this.edgeTo(j, i);
        const d = this.dom[j] & ((s >> k) & 1 ? HAS[kj] : ~HAS[kj] & ALL_ROOFS);
        if (d && d !== this.dom[j]) {
          this.dom[j] = d;
          buckets[popcount(d)].push(j);
        }
      }
    }

    // Colours: each attached neighbour of the same use already coloured is ruled out.
    for (const i of todo) {
      const nc = Math.max(1, r.colours[i]);
      let d = (1 << nc) - 1;
      for (let k = 0; k < 4; k++) {
        if (!((r.differ[i] >> k) & 1)) continue;
        const j = this.adj[4 * i + k];
        if (j !== NONE && this.colour[j] !== UNSET) d &= ~(1 << this.colour[j]);
      }
      if (!d) d = (1 << nc) - 1;
      const options: number[] = [];
      for (let c = 0; c < nc; c++) if ((d >> c) & 1) options.push(c);
      this.colour[i] = options[Math.floor(hash(i, r.sig[i] + 17) * options.length)];
    }
    return true;
  }

  private pick(i: number, d: number, r: CellRules): number {
    let total = 0;
    for (let s = 0; s < 16; s++) if ((d >> s) & 1) total += r.prefer(i, s);
    let x = hash(i, r.sig[i]) * total;
    let last = 0;
    for (let s = 0; s < 16; s++) {
      if (!((d >> s) & 1)) continue;
      last = s;
      x -= r.prefer(i, s);
      if (x <= 0) return s;
    }
    return last;
  }
}
