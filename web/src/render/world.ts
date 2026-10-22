// Three.js view of the simulated city. Pure presentation: everything drawn
// is derived from the per-parcel snapshot the worker sends.
//
// Parcels are the quads of an irregular mesh (Townscaper's grid, see
// sim-core/src/mesh.rs). The ground is that mesh; a road parcel is paved
// with a tile chosen by which neighbours it connects to (dead end, straight,
// bend, T, crossing); a building is extruded from its parcel, built out to
// the edges it shares with attached neighbours, and a wave function
// collapse (wfc.ts) picks its roof module and facade colour so terraces
// share one roof line and neighbours differ in colour.

import * as THREE from 'three';
import { MapControls } from 'three/examples/jsm/controls/MapControls.js';
import {
  CELL_STRIDE,
  C_CONDITION,
  C_FLAGS,
  C_FLOORS,
  C_NATURE,
  C_OCCUPANCY,
  C_TENTS,
  C_USE,
  FLAG_DERELICT,
  FLAG_SPECULATIVE,
  type Frame,
  LandUse,
  type MeshData,
  NONE,
  type SimEvent,
} from '../sim/protocol';
import {
  BUILDING,
  BUILDING_ROOF,
  DERELICT,
  FACADES,
  GROUND,
  mix,
  OVERLAY_DEFS,
  ramp,
  ROAD_ASPHALT,
  type RGB,
} from './palette';
import { hash, ModuleSolver, UNSET } from './wfc';

export const VSCALE = 0.045; // metres → world units (1 unit = one 40 m cell, ×1.8 exaggeration)
export const FLOOR_H = 0.14;
const ROAD_LIFT = 0.035;
const BRIDGE_Y = 0.16;
const RIDGE_H = 0.17;

const WALL = 0;
const ROOF = 1;
const TOP = 2;

/** A growable triangle soup with per-vertex position, normal, colour and parcel. */
class Soup {
  pos = new Float32Array(0);
  nor = new Float32Array(0);
  col = new Float32Array(0);
  cell = new Int32Array(0);
  part = new Uint8Array(0);
  n = 0;
  readonly geometry = new THREE.BufferGeometry();
  private grown = true;

  constructor(cap: number) {
    this.alloc(cap);
  }

  private alloc(cap: number) {
    const grow = <T extends Float32Array | Int32Array | Uint8Array>(a: T, k: number, make: (n: number) => T): T => {
      const b = make(cap * k);
      b.set(a.subarray(0, Math.min(a.length, cap * k)));
      return b;
    };
    this.pos = grow(this.pos, 3, (n) => new Float32Array(n));
    this.nor = grow(this.nor, 3, (n) => new Float32Array(n));
    this.col = grow(this.col, 3, (n) => new Float32Array(n));
    this.cell = grow(this.cell, 1, (n) => new Int32Array(n));
    this.part = grow(this.part, 1, (n) => new Uint8Array(n));
    this.grown = true;
  }

  reset() {
    this.n = 0;
  }

  /**
   * Triangle over points a, b, c of the flat xyz array `p`, wound so its
   * normal points along (rx, ry, rz).
   */
  tri(p: Float32Array, a: number, b: number, c: number, rx: number, ry: number, rz: number, cell: number, part: number) {
    if (this.n + 3 > this.cell.length) this.alloc(Math.max(1024, this.cell.length * 2));
    const ax = p[3 * a], ay = p[3 * a + 1], az = p[3 * a + 2];
    let ux = p[3 * b] - ax, uy = p[3 * b + 1] - ay, uz = p[3 * b + 2] - az;
    let vx = p[3 * c] - ax, vy = p[3 * c + 1] - ay, vz = p[3 * c + 2] - az;
    let nx = uy * vz - uz * vy;
    let ny = uz * vx - ux * vz;
    let nz = ux * vy - uy * vx;
    if (nx * rx + ny * ry + nz * rz < 0) {
      [b, c] = [c, b];
      [ux, uy, uz, vx, vy, vz] = [vx, vy, vz, ux, uy, uz];
      nx = -nx;
      ny = -ny;
      nz = -nz;
    }
    const len = Math.hypot(nx, ny, nz) || 1;
    for (const k of [a, b, c]) {
      const o = 3 * this.n;
      this.pos[o] = p[3 * k];
      this.pos[o + 1] = p[3 * k + 1];
      this.pos[o + 2] = p[3 * k + 2];
      this.nor[o] = nx / len;
      this.nor[o + 1] = ny / len;
      this.nor[o + 2] = nz / len;
      this.cell[this.n] = cell;
      this.part[this.n] = part;
      this.n++;
    }
  }

  quad(p: Float32Array, a: number, b: number, c: number, d: number, rx: number, ry: number, rz: number, cell: number, part: number) {
    this.tri(p, a, b, c, rx, ry, rz, cell, part);
    this.tri(p, a, c, d, rx, ry, rz, cell, part);
  }

  /** Upload to the GPU (positions too only if `shape`). */
  commit(shape: boolean) {
    const g = this.geometry;
    if (this.grown) {
      g.setAttribute('position', new THREE.BufferAttribute(this.pos, 3));
      g.setAttribute('normal', new THREE.BufferAttribute(this.nor, 3));
      g.setAttribute('color', new THREE.BufferAttribute(this.col, 3));
      this.grown = false;
    } else {
      const names = shape ? ['position', 'normal', 'color'] : ['color'];
      for (const name of names) {
        const a = g.getAttribute(name) as THREE.BufferAttribute;
        a.clearUpdateRanges();
        a.addUpdateRange(0, this.n * 3);
        a.needsUpdate = true;
      }
    }
    g.setDrawRange(0, this.n);
  }
}

interface Pulse {
  mesh: THREE.Mesh;
  t: number;
  life: number;
}

interface Mover {
  from: THREE.Vector3;
  to: THREE.Vector3;
  t: number;
  life: number;
  color: THREE.Color;
}

export interface OverlayStats {
  min: number;
  max: number;
  log: boolean;
}

const isBuildingUse = (u: number) =>
  u === LandUse.Residential || u === LandUse.Commercial || u === LandUse.Industrial || u === LandUse.Civic;

/** Buildings that share walls: houses and shops form blocks, factories join factories. */
function attachable(a: number, b: number): boolean {
  const town = (u: number) => u === LandUse.Residential || u === LandUse.Commercial;
  return (town(a) && town(b)) || (a === LandUse.Industrial && b === LandUse.Industrial);
}

function pitchedRoof(use: number, floors: number): boolean {
  return (use === LandUse.Residential && floors <= 3) || (use === LandUse.Commercial && floors <= 2);
}

const linear = (c: RGB): RGB => {
  const t = new THREE.Color().setRGB(c[0], c[1], c[2], THREE.SRGBColorSpace);
  return [t.r, t.g, t.b];
};

export class WorldView {
  readonly renderer: THREE.WebGLRenderer;
  readonly scene = new THREE.Scene();
  readonly camera: THREE.PerspectiveCamera;
  readonly controls: MapControls;
  private w = 0;
  private h = 0;
  private n = 0;
  private mesh!: MeshData;
  private heights: Float32Array = new Float32Array(0);
  /** Parcel centroids (x, z). */
  private cx: Float32Array = new Float32Array(0);
  private cz: Float32Array = new Float32Array(0);
  /** Building base height per parcel, ground height per mesh corner. */
  private cellY: Float32Array = new Float32Array(0);
  private vertY: Float32Array = new Float32Array(0);
  private solver?: ModuleSolver;
  private terrain?: THREE.Mesh;
  private groundCol: Float32Array = new Float32Array(0);
  private gridLines?: THREE.LineSegments;
  private roads = new Soup(4096);
  private roadMesh?: THREE.Mesh;
  private roadSig = -1;
  private buildings = new Soup(65536);
  private buildingMesh?: THREE.Mesh;
  private buildingSig = -1;
  private fences = new Soup(1024);
  private fenceMesh?: THREE.Mesh;
  private trees?: THREE.InstancedMesh;
  private bushes?: THREE.InstancedMesh;
  private tents?: THREE.InstancedMesh;
  private cranes?: THREE.InstancedMesh;
  private piers?: THREE.InstancedMesh;
  private traffic?: THREE.Points;
  private movers?: THREE.Points;
  private moverList: Mover[] = [];
  private pulses: Pulse[] = [];
  private pulseGeo = new THREE.RingGeometry(0.35, 0.5, 24);
  private highlight: THREE.LineSegments;
  private smog?: THREE.Mesh;
  private smogTex?: THREE.DataTexture;
  private commuters: Uint32Array = new Uint32Array(0);
  private lastFrame?: Frame;
  private clock = new THREE.Clock();
  private sun: THREE.DirectionalLight;
  private pts = new Float32Array(3 * 32);
  overlay: string | null = null;
  overlayStats: OverlayStats | null = null;
  showTraffic = true;
  showSmog = true;
  onFrameRendered: () => void = () => {};

  constructor(private container: HTMLElement) {
    this.renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: 'high-performance' });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.05;
    container.appendChild(this.renderer.domElement);

    this.scene.background = new THREE.Color(0xa9c8e3);
    this.scene.fog = new THREE.Fog(0xa9c8e3, 120, 320);
    this.camera = new THREE.PerspectiveCamera(45, 1, 0.1, 1000);
    this.controls = new MapControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.08;
    this.controls.maxPolarAngle = Math.PI * 0.47;
    this.controls.minDistance = 4;
    this.controls.maxDistance = 260;

    this.scene.add(new THREE.HemisphereLight(0xdfefff, 0x5a5040, 1.1));
    this.sun = new THREE.DirectionalLight(0xfff1dc, 2.2);
    this.sun.castShadow = true;
    this.sun.shadow.mapSize.set(4096, 4096);
    this.sun.shadow.bias = -0.0004;
    this.sun.shadow.normalBias = 0.02;
    this.scene.add(this.sun);
    this.scene.add(this.sun.target);

    const hl = new THREE.BufferGeometry();
    hl.setAttribute('position', new THREE.BufferAttribute(new Float32Array(24 * 3), 3));
    this.highlight = new THREE.LineSegments(hl, new THREE.LineBasicMaterial({ color: 0xffffff }));
    this.highlight.visible = false;
    this.highlight.frustumCulled = false;
    this.scene.add(this.highlight);

    new ResizeObserver(() => this.resize()).observe(container);
    this.resize();
    this.renderer.setAnimationLoop(() => this.animate());
  }

  private resize() {
    const { clientWidth: cw, clientHeight: ch } = this.container;
    if (!cw || !ch) return;
    this.renderer.setSize(cw, ch);
    this.camera.aspect = cw / ch;
    this.camera.updateProjectionMatrix();
  }

  // ------------------------------------------------------------------ setup

  setWorld(w: number, h: number, heights: Float32Array, mesh: MeshData) {
    this.dispose();
    this.w = w;
    this.h = h;
    this.mesh = mesh;
    this.heights = heights;
    const n = (this.n = mesh.quads.length / 4);
    this.cx = new Float32Array(n);
    this.cz = new Float32Array(n);
    this.cellY = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      let sx = 0;
      let sz = 0;
      for (let k = 0; k < 4; k++) {
        const v = mesh.quads[4 * i + k];
        sx += mesh.verts[2 * v];
        sz += mesh.verts[2 * v + 1];
      }
      this.cx[i] = sx / 4;
      this.cz[i] = sz / 4;
      this.cellY[i] = Math.max(heights[i], 0.6) * VSCALE;
    }
    // Corners touching water sink below the water plane (river banks).
    const nv = mesh.verts.length / 2;
    const wet = new Uint8Array(nv);
    for (let i = 0; i < n; i++) {
      if (heights[i] > 0.5) continue;
      for (let k = 0; k < 4; k++) wet[mesh.quads[4 * i + k]] = 1;
    }
    this.vertY = new Float32Array(nv);
    for (let v = 0; v < nv; v++) this.vertY[v] = wet[v] ? -0.25 : mesh.vertHeights[v] * VSCALE;
    this.solver = new ModuleSolver(mesh.adj, n);
    this.roadSig = -1;
    this.buildingSig = -1;

    this.buildTerrain();
    this.buildInstances();

    const [cx, cy] = [w / 2, h / 2];
    this.controls.target.set(cx, 0, cy);
    this.camera.position.set(cx - 20, 38, cy + 46);
    this.controls.update();

    const s = Math.max(w, h);
    this.sun.position.set(w * 0.2, s * 0.9, h * 0.1);
    this.sun.target.position.set(w / 2, 0, h / 2);
    const cam = this.sun.shadow.camera;
    cam.left = -s * 0.75;
    cam.right = s * 0.75;
    cam.top = s * 0.75;
    cam.bottom = -s * 0.75;
    cam.near = 1;
    cam.far = s * 3;
    cam.updateProjectionMatrix();
  }

  private dispose() {
    for (const o of [
      this.terrain,
      this.gridLines,
      this.roadMesh,
      this.buildingMesh,
      this.fenceMesh,
      this.trees,
      this.bushes,
      this.tents,
      this.cranes,
      this.piers,
      this.traffic,
      this.movers,
      this.smog,
    ]) {
      if (!o) continue;
      this.scene.remove(o);
      if (o !== this.roadMesh && o !== this.buildingMesh && o !== this.fenceMesh) (o as THREE.Mesh).geometry?.dispose();
    }
    for (const p of this.pulses) this.scene.remove(p.mesh);
    this.pulses = [];
    this.moverList = [];
  }

  private buildTerrain() {
    const { w, h, n, mesh } = this;
    // Smooth normals from the shared corners, then one flat-coloured quad per parcel.
    const nv = mesh.verts.length / 2;
    const shared = new THREE.BufferGeometry();
    const vp = new Float32Array(nv * 3);
    for (let v = 0; v < nv; v++) {
      vp[3 * v] = mesh.verts[2 * v];
      vp[3 * v + 1] = this.vertY[v];
      vp[3 * v + 2] = mesh.verts[2 * v + 1];
    }
    const idx: number[] = [];
    for (let i = 0; i < n; i++) {
      const [a, b, c, d] = mesh.quads.subarray(4 * i, 4 * i + 4);
      // Counter-clockwise in (x, y) is clockwise seen from above (+y up, z = map y).
      idx.push(a, c, b, a, d, c);
    }
    shared.setAttribute('position', new THREE.BufferAttribute(vp, 3));
    shared.setIndex(idx);
    shared.computeVertexNormals();
    const geo = shared.toNonIndexed();
    shared.dispose();
    this.groundCol = new Float32Array(n * 6 * 3);
    geo.setAttribute('color', new THREE.BufferAttribute(this.groundCol, 3));
    const mat = new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.95, metalness: 0 });
    this.terrain = new THREE.Mesh(geo, mat);
    this.terrain.receiveShadow = true;
    this.scene.add(this.terrain);

    // Faint parcel outlines, as in Townscaper.
    const lines: number[] = [];
    for (let i = 0; i < n; i++) {
      if (this.heights[i] <= 0.5) continue;
      for (let k = 0; k < 4; k++) {
        const j = mesh.adj[4 * i + k];
        if (j !== NONE && j < i && this.heights[j] > 0.5) continue;
        for (const v of [mesh.quads[4 * i + k], mesh.quads[4 * i + ((k + 1) & 3)]]) {
          lines.push(mesh.verts[2 * v], this.vertY[v] + 0.012, mesh.verts[2 * v + 1]);
        }
      }
    }
    const lg = new THREE.BufferGeometry();
    lg.setAttribute('position', new THREE.BufferAttribute(new Float32Array(lines), 3));
    this.gridLines = new THREE.LineSegments(
      lg,
      new THREE.LineBasicMaterial({ color: 0x1d2a14, transparent: true, opacity: 0.14, depthWrite: false }),
    );
    this.scene.add(this.gridLines);

    const water = new THREE.Mesh(
      new THREE.PlaneGeometry(w * 3, h * 3),
      new THREE.MeshStandardMaterial({ color: 0x2b6d96, roughness: 0.25, metalness: 0.1, transparent: true, opacity: 0.88 }),
    );
    water.rotation.x = -Math.PI / 2;
    water.position.set(w / 2, 0.02, h / 2);
    water.receiveShadow = true;
    this.scene.add(water);

    // Smog layer: a translucent sheet tinted by the pollution overlay.
    this.smogTex = new THREE.DataTexture(new Uint8Array(w * h * 4), w, h, THREE.RGBAFormat);
    this.smogTex.magFilter = THREE.LinearFilter;
    this.smogTex.minFilter = THREE.LinearFilter;
    this.smogTex.flipY = false;
    const smogGeo = new THREE.PlaneGeometry(w, h);
    smogGeo.rotateX(-Math.PI / 2);
    // PlaneGeometry UVs run v=1 at -z; flip to match raster rows.
    const suv = smogGeo.getAttribute('uv') as THREE.BufferAttribute;
    for (let i = 0; i < suv.count; i++) suv.setY(i, 1 - suv.getY(i));
    this.smog = new THREE.Mesh(smogGeo, new THREE.MeshBasicMaterial({ map: this.smogTex, transparent: true, depthWrite: false }));
    this.smog.position.set(w / 2, 4.2, h / 2);
    this.smog.renderOrder = 10;
    this.scene.add(this.smog);
  }

  private buildInstances() {
    const n = this.n;
    const std = (color: number, extra: Partial<THREE.MeshStandardMaterialParameters> = {}) =>
      new THREE.MeshStandardMaterial({ color, roughness: 0.8, metalness: 0.05, ...extra });

    const big = new THREE.Sphere(new THREE.Vector3(this.w / 2, 0, this.h / 2), Math.hypot(this.w, this.h));
    const soupMesh = (soup: Soup, mat: THREE.Material, shadows: boolean) => {
      soup.reset();
      soup.commit(true);
      soup.geometry.boundingSphere = big;
      const m = new THREE.Mesh(soup.geometry, mat);
      m.castShadow = shadows;
      m.receiveShadow = true;
      m.frustumCulled = false;
      this.scene.add(m);
      return m;
    };
    this.roadMesh = soupMesh(this.roads, std(0xffffff, { vertexColors: true, roughness: 0.9 }), false);
    this.buildingMesh = soupMesh(this.buildings, std(0xffffff, { vertexColors: true }), true);
    this.fenceMesh = soupMesh(this.fences, std(0xffffff, { vertexColors: true, side: THREE.DoubleSide }), false);
    this.roadSig = -1;
    this.buildingSig = -1;

    const tree = new THREE.ConeGeometry(0.16, 0.55, 6);
    tree.translate(0, 0.275, 0);
    this.trees = new THREE.InstancedMesh(tree, std(0xffffff, { roughness: 1 }), n * 4);
    this.trees.castShadow = true;

    const bush = new THREE.IcosahedronGeometry(0.1, 0);
    bush.translate(0, 0.06, 0);
    this.bushes = new THREE.InstancedMesh(bush, std(0xffffff, { roughness: 1 }), n * 3);

    const tent = new THREE.ConeGeometry(0.11, 0.14, 4);
    tent.translate(0, 0.07, 0);
    this.tents = new THREE.InstancedMesh(tent, std(0xffffff), Math.min(n, 20000));

    const crane = new THREE.BoxGeometry(0.05, 1, 0.05);
    crane.translate(0, 0.5, 0);
    this.cranes = new THREE.InstancedMesh(crane, std(0xf2c230), 4000);

    const pier = new THREE.CylinderGeometry(0.07, 0.09, 1, 6);
    pier.translate(0, 0.5, 0);
    this.piers = new THREE.InstancedMesh(pier, std(0x8a8580), 4000);

    for (const m of [this.trees, this.bushes, this.tents, this.cranes, this.piers]) {
      m.count = 0;
      m.frustumCulled = false;
      this.scene.add(m);
    }

    const tGeo = new THREE.BufferGeometry();
    tGeo.setAttribute('position', new THREE.BufferAttribute(new Float32Array(3000 * 3), 3));
    this.traffic = new THREE.Points(tGeo, new THREE.PointsMaterial({ color: 0xffe9a0, size: 0.18, sizeAttenuation: true }));
    this.traffic.frustumCulled = false;
    this.scene.add(this.traffic);

    const mGeo = new THREE.BufferGeometry();
    mGeo.setAttribute('position', new THREE.BufferAttribute(new Float32Array(600 * 3), 3));
    mGeo.setAttribute('color', new THREE.BufferAttribute(new Float32Array(600 * 3), 3));
    this.movers = new THREE.Points(mGeo, new THREE.PointsMaterial({ size: 0.45, sizeAttenuation: true, vertexColors: true }));
    this.movers.frustumCulled = false;
    this.scene.add(this.movers);
  }

  // ----------------------------------------------------------- mesh helpers

  private corner(i: number, k: number): number {
    return this.mesh.quads[4 * i + (k & 3)];
  }

  /** A point inside parcel i at bilinear coordinates (u, v) of its corners. */
  private inside(i: number, u: number, v: number, out: THREE.Vector3): THREE.Vector3 {
    const m = this.mesh;
    const [a, b, c, d] = [0, 1, 2, 3].map((k) => this.corner(i, k));
    const wa = (1 - u) * (1 - v), wb = u * (1 - v), wc = u * v, wd = (1 - u) * v;
    out.set(
      wa * m.verts[2 * a] + wb * m.verts[2 * b] + wc * m.verts[2 * c] + wd * m.verts[2 * d],
      this.cellY[i],
      wa * m.verts[2 * a + 1] + wb * m.verts[2 * b + 1] + wc * m.verts[2 * c + 1] + wd * m.verts[2 * d + 1],
    );
    return out;
  }

  /** Mean ground height of parcel i's corners. */
  private groundY(i: number): number {
    let s = 0;
    for (let k = 0; k < 4; k++) s += this.vertY[this.corner(i, k)];
    return s / 4;
  }

  private cellAt(x: number, z: number): number {
    const px = Math.min(this.w - 1, Math.max(0, Math.floor(x)));
    const pz = Math.min(this.h - 1, Math.max(0, Math.floor(z)));
    return this.mesh.pixelCells[pz * this.w + px];
  }

  // ------------------------------------------------------------- per frame

  update(frame: Frame, events: SimEvent[]) {
    this.lastFrame = frame;
    this.commuters = frame.commuters;
    this.paint();
    for (const e of events.slice(-250)) this.spawnEvent(e);
  }

  /** Repaint ground, roads, buildings and vegetation from the last frame. */
  paint() {
    const frame = this.lastFrame;
    if (!frame || !this.terrain) return;
    const { n } = this;
    const cells = frame.cells;

    // Overlay values
    let ov: Float32Array | null = null;
    const def = OVERLAY_DEFS.find((d) => d.key === this.overlay);
    if (def) {
      const k = OVERLAY_DEFS.indexOf(def);
      ov = frame.overlays.subarray(k * n, (k + 1) * n);
      this.overlayStats = robustRange(ov, cells, !!def.log);
    } else {
      this.overlayStats = null;
    }
    const ovColor = (i: number): RGB | null => {
      if (!ov || !def || !this.overlayStats) return null;
      const v = ov[i];
      if (def.key === 'owner_wealth' && v < 0) return v < -1.5 ? [0.95, 0.95, 0.95] : [0.45, 0.45, 0.45];
      const { min, max, log } = this.overlayStats;
      const t = log ? (Math.log1p(Math.max(v, 0)) - min) / (max - min || 1) : (v - min) / (max - min || 1);
      return ramp(def.ramp, t);
    };

    this.paintGround(cells, ovColor);
    this.paintRoads(cells);
    this.paintBuildings(cells, ovColor);
    this.paintDetails(frame);
  }

  private paintGround(cells: Uint8Array, ovColor: (i: number) => RGB | null) {
    const gc = this.groundCol;
    const col = new THREE.Color();
    for (let i = 0; i < this.n; i++) {
      const o = i * CELL_STRIDE;
      const use = cells[o + C_USE];
      const nature = cells[o + C_NATURE] / 255;
      let g: RGB = GROUND[use] ?? [1, 0, 1];
      if (use === LandUse.Wild) g = mix([0.5, 0.6, 0.33], [0.2, 0.38, 0.18], nature);
      else if (use === LandUse.Farm) {
        // A patchwork of crops, one per field.
        const crop = hash(i, 5);
        g = crop < 0.4 ? [0.74, 0.68, 0.36] : crop < 0.75 ? [0.62, 0.62, 0.3] : [0.55, 0.6, 0.32];
      } else if (use === LandUse.Vacant) g = mix([0.5, 0.44, 0.32], [0.45, 0.5, 0.28], nature);
      const shade = 0.9 + 0.2 * Math.min(1, this.heights[i] / 50);
      g = [g[0] * shade, g[1] * shade, g[2] * shade];
      const oc = use !== LandUse.Water && use !== LandUse.Road ? ovColor(i) : null;
      if (oc) g = mix(g, oc, 0.85);
      col.setRGB(Math.min(1, g[0]), Math.min(1, g[1]), Math.min(1, g[2]), THREE.SRGBColorSpace);
      for (let v = 0; v < 6; v++) {
        const p = (6 * i + v) * 3;
        gc[p] = col.r;
        gc[p + 1] = col.g;
        gc[p + 2] = col.b;
      }
    }
    (this.terrain!.geometry.getAttribute('color') as THREE.BufferAttribute).needsUpdate = true;
  }

  /** Paved road tiles: arms from each road parcel's centre to the edges it shares with other roads. */
  private paintRoads(cells: Uint8Array) {
    const { n, mesh } = this;
    const isRoad = (i: number) => cells[i * CELL_STRIDE + C_USE] === LandUse.Road;
    let sig = 0;
    for (let i = 0; i < n; i++) if (isRoad(i)) sig = (Math.imul(sig, 31) + i + 1) | 0;
    if (sig === this.roadSig) return;
    this.roadSig = sig;

    const soup = this.roads;
    soup.reset();
    const p = this.pts;
    const roadY = (i: number) => (this.heights[i] <= 0.5 ? BRIDGE_Y : Math.max(this.groundY(i), 0.03) + ROAD_LIFT);
    const m4 = new THREE.Matrix4();
    let npier = 0;
    for (let i = 0; i < n; i++) {
      if (!isRoad(i)) continue;
      const cx = this.cx[i], cz = this.cz[i], cy = roadY(i);
      let arms = 0;
      for (let k = 0; k < 4; k++) {
        const j = mesh.adj[4 * i + k];
        if (j === NONE || !isRoad(j)) continue;
        arms++;
        const a = this.corner(i, k), b = this.corner(i, k + 1);
        const mx = (mesh.verts[2 * a] + mesh.verts[2 * b]) / 2;
        const mz = (mesh.verts[2 * a + 1] + mesh.verts[2 * b + 1]) / 2;
        const my = (cy + roadY(j)) / 2;
        // Half-width across the arm, along the shared edge.
        let ex = mesh.verts[2 * b] - mesh.verts[2 * a];
        let ez = mesh.verts[2 * b + 1] - mesh.verts[2 * a + 1];
        const el = Math.hypot(ex, ez) || 1;
        ex = (ex / el) * 0.2;
        ez = (ez / el) * 0.2;
        p.set([cx - ex, cy, cz - ez, cx + ex, cy, cz + ez, mx + ex, my, mz + ez, mx - ex, my, mz - ez]);
        soup.quad(p, 0, 1, 2, 3, 0, 1, 0, i, 0);
      }
      // The junction: a disc where arms meet, a turning circle at a dead end.
      const r = arms === 1 ? 0.3 : 0.21;
      const seg = 8;
      p[0] = cx;
      p[1] = cy + 0.001;
      p[2] = cz;
      for (let s = 0; s < seg; s++) {
        const t = (s / seg) * Math.PI * 2;
        p.set([cx + r * Math.cos(t), cy + 0.001, cz + r * Math.sin(t)], 3 * (s + 1));
      }
      for (let s = 0; s < seg; s++) soup.tri(p, 0, s + 1, ((s + 1) % seg) + 1, 0, 1, 0, i, 0);
      if (this.heights[i] <= 0.5 && npier < this.piers!.instanceMatrix.count) {
        m4.makeScale(1, BRIDGE_Y + 0.3, 1).setPosition(cx, -0.3, cz);
        this.piers!.setMatrixAt(npier++, m4);
      }
    }
    const [r, g, b] = linear(ROAD_ASPHALT);
    for (let v = 0; v < soup.n; v++) soup.col.set([r, g, b], 3 * v);
    soup.commit(true);
    this.piers!.count = npier;
    this.piers!.instanceMatrix.needsUpdate = true;
  }

  private paintBuildings(cells: Uint8Array, ovColor: (i: number) => RGB | null) {
    const { n, mesh } = this;
    const use = (i: number) => cells[i * CELL_STRIDE + C_USE];
    const floors = (i: number) => Math.max(1, cells[i * CELL_STRIDE + C_FLOORS]);

    // ---- rules for the wave function collapse
    const sig = new Int32Array(n).fill(-1);
    const link = new Uint8Array(n);
    const differ = new Uint8Array(n);
    const share = new Uint8Array(n);
    const pitched = new Uint8Array(n);
    const colours = new Uint8Array(n);
    let shapeSig = 0;
    for (let i = 0; i < n; i++) {
      const u = use(i);
      if (u === LandUse.Construction) shapeSig = (Math.imul(shapeSig, 31) + i * 7 + floors(i)) | 0;
      if (!isBuildingUse(u)) continue;
      const f = floors(i);
      const pit = pitchedRoof(u, f);
      for (let k = 0; k < 4; k++) {
        const j = mesh.adj[4 * i + k];
        if (j === NONE || !isBuildingUse(use(j)) || !attachable(u, use(j))) continue;
        share[i] |= 1 << k;
        if (use(j) === u) differ[i] |= 1 << k;
        if (pit && use(j) === u && floors(j) === f && pitchedRoof(use(j), floors(j))) link[i] |= 1 << k;
      }
      pitched[i] = pit ? 1 : 0;
      colours[i] = (FACADES[u] ?? [[1, 1, 1]]).length;
      sig[i] = u | (Math.min(f, 255) << 4) | (link[i] << 12) | (differ[i] << 16) | (share[i] << 20) | (pitched[i] << 24);
      shapeSig = (Math.imul(shapeSig, 31) + sig[i] + i) | 0;
    }
    const solver = this.solver!;
    const corner = (i: number, k: number) => this.corner(i, k);
    solver.solve({
      sig,
      link,
      differ,
      pitched,
      colours,
      prefer: (i, s) => {
        // Gables along the parcel's long axis; the odd L, T or hipped roof.
        const len = (k: number) => {
          const a = corner(i, k), b = corner(i, k + 1), c = corner(i, k + 2), d = corner(i, k + 3);
          return Math.hypot(
            (mesh.verts[2 * a] + mesh.verts[2 * b] - mesh.verts[2 * c] - mesh.verts[2 * d]) / 2,
            (mesh.verts[2 * a + 1] + mesh.verts[2 * b + 1] - mesh.verts[2 * c + 1] - mesh.verts[2 * d + 1]) / 2,
          );
        };
        switch (s) {
          case 0:
            return 1.2;
          case 5: // edges 0 and 2: the ridge runs between them
            return len(1) >= len(0) ? 5 : 1.5;
          case 10: // edges 1 and 3
            return len(0) > len(1) ? 5 : 1.5;
          case 1:
          case 2:
          case 4:
          case 8:
            return 0.5;
          case 3:
          case 6:
          case 12:
          case 9:
            return 0.9;
          case 15:
            return 0.25;
          default:
            return 0.35;
        }
      },
    });
    // Shape changes when any module, height or construction site changes.
    for (let i = 0; i < n; i++) if (sig[i] >= 0) shapeSig = (Math.imul(shapeSig, 17) + solver.roof[i] * 3 + solver.colour[i]) | 0;

    const soup = this.buildings;
    const top = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      const u = use(i);
      if (isBuildingUse(u)) top[i] = this.cellY[i] + floors(i) * FLOOR_H;
      else if (u === LandUse.Construction) top[i] = this.cellY[i] + Math.max(0.25, floors(i) * FLOOR_H * 0.35);
    }
    const reshape = shapeSig !== this.buildingSig;
    if (reshape) {
      this.buildingSig = shapeSig;
      soup.reset();
      for (let i = 0; i < n; i++) {
        const u = use(i);
        if (!isBuildingUse(u) && u !== LandUse.Construction) continue;
        const roofS = solver.roof[i] === UNSET ? 0 : solver.roof[i];
        this.addBuilding(i, u, floors(i), share[i], top, roofS, pitched[i] === 1 || u === LandUse.Civic);
      }
    }

    // ---- colours (every frame: occupancy, condition and overlays change)
    const wall = new Float32Array(3 * n);
    const roof = new Float32Array(3 * n);
    const flat = linear([0.45, 0.46, 0.48]);
    for (let i = 0; i < n; i++) {
      const u = use(i);
      if (!isBuildingUse(u) && u !== LandUse.Construction) continue;
      const o = i * CELL_STRIDE;
      const cond = cells[o + C_CONDITION] / 255;
      const occ = cells[o + C_OCCUPANCY] / 255;
      const derelict = (cells[o + C_FLAGS] & FLAG_DERELICT) !== 0;
      let c: RGB;
      let rc: RGB;
      if (u === LandUse.Construction) {
        c = BUILDING[LandUse.Construction];
        rc = c;
      } else {
        const ci = solver.colour[i] === UNSET ? 0 : solver.colour[i];
        const palette = FACADES[u] ?? [BUILDING[u]];
        c = palette[ci % palette.length];
        rc = (BUILDING_ROOF[u] ?? [[0.5, 0.3, 0.25]])[ci % (BUILDING_ROOF[u]?.length ?? 1)];
        const jitter = 0.94 + 0.1 * hash(i, 1);
        c = [c[0] * jitter, c[1] * jitter, c[2] * jitter];
        c = mix(DERELICT, c, Math.min(1, 0.25 + cond));
        if (u !== LandUse.Civic) c = mix(c, [c[0] * 0.55, c[1] * 0.55, c[2] * 0.6], 1 - occ);
        if (derelict) {
          c = mix(c, DERELICT, 0.6);
          rc = mix(rc, [0.3, 0.25, 0.22], 0.7);
        }
      }
      const oc = ovColor(i);
      if (oc) {
        c = oc;
        rc = oc;
      }
      wall.set(linear(c), 3 * i);
      roof.set(oc || u === LandUse.Construction || pitched[i] || u === LandUse.Civic ? linear(rc) : flat, 3 * i);
    }
    for (let v = 0; v < soup.n; v++) {
      const i = soup.cell[v];
      const src = soup.part[v] === WALL ? wall : roof;
      soup.col[3 * v] = src[3 * i];
      soup.col[3 * v + 1] = src[3 * i + 1];
      soup.col[3 * v + 2] = src[3 * i + 2];
    }
    soup.commit(reshape);
  }

  /**
   * Extrude parcel i: walls set back from its free edges and built out to
   * the edges it shares with attached neighbours, then a flat top or the
   * roof module `roofS` (the edges the ridge runs to).
   */
  private addBuilding(i: number, u: number, floors: number, share: number, top: Float32Array, roofS: number, pitched: boolean) {
    const { mesh } = this;
    const soup = this.buildings;
    const p = this.pts;
    const site = u === LandUse.Construction;
    const setback =
      u === LandUse.Industrial || u === LandUse.Commercial ? 0.07 : site ? 0.12 : floors <= 3 ? 0.14 : 0.1;
    const P: number[][] = [];
    for (let k = 0; k < 4; k++) {
      const v = this.corner(i, k);
      P.push([mesh.verts[2 * v], mesh.verts[2 * v + 1]]);
    }
    const d = [0, 1, 2, 3].map((k) => (!site && (share >> k) & 1 ? 0.004 : setback));
    const F: number[][] = [];
    for (let k = 0; k < 4; k++) {
      const k0 = (k + 3) & 3;
      // Inset lines of edge k0 (P[k0]→P[k]) and edge k (P[k]→P[k+1]); inward = left of the direction.
      const e0 = [P[k][0] - P[k0][0], P[k][1] - P[k0][1]];
      const e1 = [P[(k + 1) & 3][0] - P[k][0], P[(k + 1) & 3][1] - P[k][1]];
      const l0 = Math.hypot(e0[0], e0[1]) || 1;
      const l1 = Math.hypot(e1[0], e1[1]) || 1;
      const n0 = [-e0[1] / l0, e0[0] / l0];
      const n1 = [-e1[1] / l1, e1[0] / l1];
      const A = [P[k0][0] + n0[0] * d[k0], P[k0][1] + n0[1] * d[k0]];
      const B = [P[k][0] + n1[0] * d[k], P[k][1] + n1[1] * d[k]];
      const den = e0[0] * e1[1] - e0[1] * e1[0];
      if (Math.abs(den) < 1e-4 * l0 * l1) {
        F.push([P[k][0] + n0[0] * d[k0] + n1[0] * d[k], P[k][1] + n0[1] * d[k0] + n1[1] * d[k]]);
      } else {
        const s = ((B[0] - A[0]) * e1[1] - (B[1] - A[1]) * e1[0]) / den;
        F.push([A[0] + s * e0[0], A[1] + s * e0[1]]);
      }
    }
    let ground = Infinity;
    for (let k = 0; k < 4; k++) ground = Math.min(ground, this.vertY[this.corner(i, k)]);
    const yb = Math.min(ground, this.cellY[i]) - 0.05;
    const yt = top[i];
    // points: 0-3 footprint at base, 4-7 at eaves
    for (let k = 0; k < 4; k++) {
      p.set([F[k][0], yb, F[k][1]], 3 * k);
      p.set([F[k][0], yt, F[k][1]], 3 * (k + 4));
    }
    const fcx = (F[0][0] + F[1][0] + F[2][0] + F[3][0]) / 4;
    const fcz = (F[0][1] + F[1][1] + F[2][1] + F[3][1]) / 4;
    const part = site ? ROOF : WALL;
    for (let k = 0; k < 4; k++) {
      const j = mesh.adj[4 * i + k];
      // A wall against an attached neighbour at least as tall is never seen.
      if ((share >> k) & 1 && j !== NONE && top[j] >= yt - 1e-3) continue;
      const k1 = (k + 1) & 3;
      const ox = (F[k][0] + F[k1][0]) / 2 - fcx;
      const oz = (F[k][1] + F[k1][1]) / 2 - fcz;
      soup.quad(p, k, k1, k1 + 4, k + 4, ox, 0, oz, i, part);
    }
    if (!pitched) {
      soup.quad(p, 4, 5, 6, 7, 0, 1, 0, i, site ? ROOF : TOP);
      return;
    }
    // Pitched roof: apex over the middle, ridge out to the edges in the module.
    const ridge = u === LandUse.Civic ? RIDGE_H * 1.5 : RIDGE_H;
    p.set([fcx, yt + ridge, fcz], 3 * 8);
    for (let k = 0; k < 4; k++) {
      const k1 = (k + 1) & 3;
      const up = (roofS >> k) & 1 ? ridge : 0;
      p.set([(F[k][0] + F[k1][0]) / 2, yt + up, (F[k][1] + F[k1][1]) / 2], 3 * (9 + k));
    }
    for (let k = 0; k < 4; k++) {
      const k1 = (k + 1) & 3;
      soup.tri(p, k + 4, 9 + k, 8, 0, 1, 0, i, ROOF);
      soup.tri(p, 9 + k, k1 + 4, 8, 0, 1, 0, i, ROOF);
      if ((roofS >> k) & 1) {
        // Gable end.
        const ox = p[3 * (9 + k)] - fcx;
        const oz = p[3 * (9 + k) + 2] - fcz;
        soup.tri(p, k + 4, k1 + 4, 9 + k, ox, 0, oz, i, WALL);
      }
    }
  }

  /** Vegetation, fences, tents, cranes and smog. */
  private paintDetails(frame: Frame) {
    const { n, mesh } = this;
    const cells = frame.cells;
    const m4 = new THREE.Matrix4();
    const col = new THREE.Color();
    const q = new THREE.Quaternion();
    const pos = new THREE.Vector3();
    const scl = new THREE.Vector3();
    let nt = 0;
    let nbu = 0;
    let nte = 0;
    let ncr = 0;
    const smog = this.smogTex!.image.data as Uint8Array;
    const pollutionK = OVERLAY_DEFS.findIndex((d) => d.key === 'pollution');
    const pollution = frame.overlays.subarray(pollutionK * n, (pollutionK + 1) * n);
    const fences = this.fences;
    fences.reset();
    const p = this.pts;

    for (let i = 0; i < n; i++) {
      const o = i * CELL_STRIDE;
      const use = cells[o + C_USE];
      const floors = cells[o + C_FLOORS];
      const nature = cells[o + C_NATURE] / 255;
      const tents = cells[o + C_TENTS];
      const flags = cells[o + C_FLAGS];
      const y0 = this.cellY[i];

      let treesHere = 0;
      let bushesHere = 0;
      if (use === LandUse.Wild) treesHere = Math.round(nature * 4 - 0.6);
      else if (use === LandUse.Park) treesHere = 3;
      else if (use === LandUse.Residential && floors <= 2) treesHere = nature > 0.15 ? 1 : 0;
      if (use === LandUse.Vacant || (use === LandUse.Wild && flags & FLAG_SPECULATIVE)) bushesHere = Math.round(nature * 5);
      if (use === LandUse.Wild && nature < 0.45) bushesHere = 1;
      for (let k = 0; k < treesHere && nt < this.trees!.instanceMatrix.count; k++) {
        if (use === LandUse.Residential) {
          // In the front garden: just inside a free edge.
          const e = Math.floor(hash(i, 9) * 4);
          const a = this.corner(i, e), b = this.corner(i, e + 1);
          const t = 0.25 + 0.5 * hash(i, 10);
          const ex = mesh.verts[2 * a] + (mesh.verts[2 * b] - mesh.verts[2 * a]) * t;
          const ez = mesh.verts[2 * a + 1] + (mesh.verts[2 * b + 1] - mesh.verts[2 * a + 1]) * t;
          pos.set(ex + (this.cx[i] - ex) * 0.08, y0, ez + (this.cz[i] - ez) * 0.08);
        } else {
          this.inside(i, 0.15 + 0.7 * hash(i, 10 + k), 0.15 + 0.7 * hash(i, 20 + k), pos);
        }
        const s = (use === LandUse.Residential ? 0.55 : 0.7) + 0.6 * hash(i, 30 + k);
        m4.compose(pos, q.identity(), scl.set(s, s * (0.9 + nature * 0.5), s));
        this.trees!.setMatrixAt(nt, m4);
        const tc: RGB = mix([0.16, 0.34, 0.14], [0.27, 0.45, 0.2], hash(i, 40 + k));
        const dead = this.showSmog ? Math.min(0.7, pollution[i] / 10) : 0;
        const tc2 = mix(tc, [0.45, 0.4, 0.25], dead);
        this.trees!.setColorAt(nt, col.setRGB(tc2[0], tc2[1], tc2[2], THREE.SRGBColorSpace));
        nt++;
      }
      for (let k = 0; k < bushesHere && nbu < this.bushes!.instanceMatrix.count; k++) {
        this.inside(i, 0.1 + 0.8 * hash(i, 50 + k), 0.1 + 0.8 * hash(i, 60 + k), pos);
        const s = 0.6 + 0.8 * hash(i, 70 + k);
        m4.compose(pos, q.identity(), scl.set(s, s, s));
        this.bushes!.setMatrixAt(nbu, m4);
        const bc: RGB = mix([0.5, 0.52, 0.25], [0.35, 0.45, 0.2], hash(i, 80 + k));
        this.bushes!.setColorAt(nbu, col.setRGB(bc[0], bc[1], bc[2], THREE.SRGBColorSpace));
        nbu++;
      }
      // Fenced idle lots: land held empty inside the built-up area.
      if (flags & FLAG_SPECULATIVE) {
        for (let k = 0; k < 4; k++) {
          this.inside(i, k === 1 || k === 2 ? 0.94 : 0.06, k >= 2 ? 0.94 : 0.06, pos);
          p.set([pos.x, this.vertY[this.corner(i, k)] + 0.01, pos.z], 3 * k);
          p.set([pos.x, this.vertY[this.corner(i, k)] + 0.09, pos.z], 3 * (k + 4));
        }
        for (let k = 0; k < 4; k++) fences.quad(p, k, (k + 1) & 3, ((k + 1) & 3) + 4, k + 4, 0, 0, 1, i, 0);
      }
      for (let k = 0; k < tents && nte < this.tents!.instanceMatrix.count; k++) {
        this.inside(i, 0.15 + 0.7 * hash(i, 90 + k), 0.15 + 0.7 * hash(i, 100 + k), pos);
        m4.compose(pos, q.identity(), scl.set(1, 1, 1));
        this.tents!.setMatrixAt(nte, m4);
        const tc: RGB = hash(i, 110 + k) > 0.5 ? [0.85, 0.45, 0.15] : [0.2, 0.4, 0.75];
        this.tents!.setColorAt(nte, col.setRGB(tc[0], tc[1], tc[2], THREE.SRGBColorSpace));
        nte++;
      }
      if (use === LandUse.Construction && ncr < 4000) {
        const ch = Math.max(1.2, Math.max(1, floors) * FLOOR_H * 1.15);
        this.inside(i, 0.15, 0.15, pos);
        m4.compose(pos, q.identity(), scl.set(1, ch, 1));
        this.cranes!.setMatrixAt(ncr++, m4);
      }
    }
    // Smog over each raster pixel from the parcel under it.
    const px = mesh.pixelCells;
    for (let k = 0; k < px.length; k++) {
      const pl = this.showSmog ? Math.min(1, pollution[px[k]] / 6) : 0;
      smog[k * 4] = 120;
      smog[k * 4 + 1] = 110;
      smog[k * 4 + 2] = 90;
      smog[k * 4 + 3] = Math.round(pl * 150);
    }

    const [fr, fg, fb] = linear([0.61, 0.48, 0.29]);
    for (let v = 0; v < fences.n; v++) fences.col.set([fr, fg, fb], 3 * v);
    fences.commit(true);
    const finish = (m: THREE.InstancedMesh, count: number) => {
      m.count = count;
      m.instanceMatrix.needsUpdate = true;
      if (m.instanceColor) m.instanceColor.needsUpdate = true;
    };
    finish(this.trees!, nt);
    finish(this.bushes!, nbu);
    finish(this.tents!, nte);
    finish(this.cranes!, ncr);
    this.smogTex!.needsUpdate = true;
    this.smog!.visible = this.showSmog;
  }

  // ---------------------------------------------------------------- effects

  private cellPos(cell: number, lift = 0.2): THREE.Vector3 {
    return new THREE.Vector3(this.cx[cell] ?? 0, (this.cellY[cell] ?? 0) + lift, this.cz[cell] ?? 0);
  }

  private spawnPulse(cell: number, color: number, life = 1.6) {
    if (this.pulses.length > 220) return;
    const mesh = new THREE.Mesh(
      this.pulseGeo,
      new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.9, side: THREE.DoubleSide, depthWrite: false }),
    );
    mesh.rotation.x = -Math.PI / 2;
    mesh.position.copy(this.cellPos(cell, 0.08));
    this.scene.add(mesh);
    this.pulses.push({ mesh, t: 0, life });
  }

  private spawnMover(from: THREE.Vector3, to: THREE.Vector3, color: number) {
    if (this.moverList.length >= 600) return;
    this.moverList.push({ from, to, t: 0, life: 3 + Math.random() * 2, color: new THREE.Color(color) });
  }

  private spawnEvent(e: SimEvent) {
    switch (e.t) {
      case 'sale':
        this.spawnPulse(e.cell, e.forced ? 0xff4040 : 0xffd040);
        break;
      case 'harberger_buyout':
        this.spawnPulse(e.cell, 0xff40ff, 2.2);
        break;
      case 'build_start':
        this.spawnPulse(e.cell, 0xffa020);
        break;
      case 'build_done':
        this.spawnPulse(e.cell, 0x60ff90);
        break;
      case 'demolish':
        this.spawnPulse(e.cell, 0x905020);
        break;
      case 'firm_close':
        this.spawnPulse(e.cell, 0xff2020, 2);
        break;
      case 'firm_open':
        this.spawnPulse(e.cell, 0x40c0ff);
        break;
      case 'park':
        this.spawnPulse(e.cell, 0x30ff60, 2.5);
        break;
      case 'road':
        this.spawnPulse(e.cell, 0xd0d0d0, 1.2);
        break;
      case 'emigrate': {
        const from = this.cellPos(e.cell, 0.3);
        const edgeX = from.x < this.w / 2 ? -2 : this.w + 2;
        this.spawnMover(from, new THREE.Vector3(edgeX, 0.5, from.z), e.wealthy ? 0xffd700 : 0xff6040);
        break;
      }
      case 'immigrate': {
        const from = this.cellPos(e.cell, 0.3);
        const to = this.controls.target.clone().add(new THREE.Vector3((Math.random() - 0.5) * 16, 0.3, (Math.random() - 0.5) * 16));
        this.spawnMover(from, to, 0x60d0ff);
        break;
      }
    }
  }

  private animate() {
    const dt = Math.min(0.1, this.clock.getDelta());
    const time = this.clock.elapsedTime;
    this.controls.update();

    // pulses
    for (let k = this.pulses.length - 1; k >= 0; k--) {
      const p = this.pulses[k];
      p.t += dt;
      const f = p.t / p.life;
      if (f >= 1) {
        this.scene.remove(p.mesh);
        (p.mesh.material as THREE.Material).dispose();
        this.pulses.splice(k, 1);
        continue;
      }
      const s = 0.6 + f * 2.2;
      p.mesh.scale.set(s, s, s);
      (p.mesh.material as THREE.MeshBasicMaterial).opacity = 0.9 * (1 - f);
    }

    // movers (migration traffic)
    if (this.movers) {
      const posAttr = this.movers.geometry.getAttribute('position') as THREE.BufferAttribute;
      const colAttr = this.movers.geometry.getAttribute('color') as THREE.BufferAttribute;
      let n = 0;
      for (let k = this.moverList.length - 1; k >= 0; k--) {
        const m = this.moverList[k];
        m.t += dt / m.life;
        if (m.t >= 1) {
          this.moverList.splice(k, 1);
          continue;
        }
        const t = m.t;
        const x = m.from.x + (m.to.x - m.from.x) * t;
        const z = m.from.z + (m.to.z - m.from.z) * t;
        posAttr.setXYZ(n, x, 0.5 + Math.sin(t * Math.PI) * 1.5, z);
        colAttr.setXYZ(n, m.color.r, m.color.g, m.color.b);
        n++;
      }
      this.movers.geometry.setDrawRange(0, n);
      posAttr.needsUpdate = true;
      colAttr.needsUpdate = true;
    }

    // commuter traffic: trips home → work and back, staggered phases
    if (this.traffic && this.n) {
      const posAttr = this.traffic.geometry.getAttribute('position') as THREE.BufferAttribute;
      const pairs = this.commuters.length / 2;
      const n = this.showTraffic ? Math.min(pairs, posAttr.count) : 0;
      for (let k = 0; k < n; k++) {
        const home = this.commuters[2 * k];
        const job = this.commuters[2 * k + 1];
        const phase = (time * 0.12 + hash(k, 7)) % 1;
        const t = phase < 0.5 ? phase * 2 : 2 - phase * 2;
        const px = this.cx[home] + (this.cx[job] - this.cx[home]) * t;
        const pz = this.cz[home] + (this.cz[job] - this.cz[home]) * t;
        posAttr.setXYZ(k, px, (this.cellY[this.cellAt(px, pz)] ?? 0) + 0.08, pz);
      }
      this.traffic.geometry.setDrawRange(0, n);
      posAttr.needsUpdate = true;
    }

    this.renderer.render(this.scene, this.camera);
    this.onFrameRendered();
  }

  // ---------------------------------------------------------------- picking

  pick(clientX: number, clientY: number): number | null {
    if (!this.terrain) return null;
    const r = this.renderer.domElement.getBoundingClientRect();
    const ndc = new THREE.Vector2(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
    const ray = new THREE.Raycaster();
    ray.setFromCamera(ndc, this.camera);
    const targets: THREE.Object3D[] = [this.terrain];
    if (this.buildingMesh) targets.push(this.buildingMesh);
    const hit = ray.intersectObjects(targets, false)[0];
    if (!hit || !hit.face) return null;
    if (hit.object === this.buildingMesh) return this.buildings.cell[hit.face.a];
    return Math.floor(hit.face.a / 6);
  }

  setHighlight(cell: number | null) {
    if (cell === null || !this.lastFrame || cell >= this.n) {
      this.highlight.visible = false;
      return;
    }
    const use = this.lastFrame.cells[cell * CELL_STRIDE + C_USE];
    const floors = this.lastFrame.cells[cell * CELL_STRIDE + C_FLOORS];
    const top = this.cellY[cell] + (isBuildingUse(use) ? Math.max(1, floors) * FLOOR_H : 0) + 0.05;
    const attr = this.highlight.geometry.getAttribute('position') as THREE.BufferAttribute;
    let o = 0;
    const put = (v: number, y: number) => attr.setXYZ(o++, this.mesh.verts[2 * v], y, this.mesh.verts[2 * v + 1]);
    for (let k = 0; k < 4; k++) {
      const a = this.corner(cell, k), b = this.corner(cell, k + 1);
      put(a, this.vertY[a] + 0.03);
      put(b, this.vertY[b] + 0.03);
      put(a, top);
      put(b, top);
      put(a, this.vertY[a] + 0.03);
      put(a, top);
    }
    attr.needsUpdate = true;
    this.highlight.visible = true;
  }

  focus(cell: number) {
    const p = this.cellPos(cell, 0);
    const delta = p.clone().sub(this.controls.target);
    delta.y = 0;
    this.controls.target.add(delta);
    this.camera.position.add(delta);
  }

  /** Centre the view on the built-up area. */
  focusOnCity(cells: Uint8Array) {
    let sx = 0;
    let sz = 0;
    let k = 0;
    for (let i = 0; i < this.n; i++) {
      const u = cells[i * CELL_STRIDE + C_USE];
      if (u === LandUse.Residential || u === LandUse.Commercial || u === LandUse.Industrial) {
        sx += this.cx[i];
        sz += this.cz[i];
        k++;
      }
    }
    if (k) this.focus(this.cellAt(sx / k, sz / k));
  }
}

/** 2nd–98th percentile range over non-water parcels. */
function robustRange(v: Float32Array, cells: Uint8Array, log: boolean): OverlayStats {
  const sample: number[] = [];
  const step = Math.max(1, Math.floor(v.length / 6000));
  for (let i = 0; i < v.length; i += step) {
    const use = cells[i * CELL_STRIDE + C_USE];
    if (use === LandUse.Water || use === LandUse.Road) continue;
    const x = v[i];
    if (!Number.isFinite(x) || x < 0) continue;
    sample.push(log ? Math.log1p(x) : x);
  }
  if (!sample.length) return { min: 0, max: 1, log };
  sample.sort((a, b) => a - b);
  const lo = sample[Math.floor(sample.length * 0.02)];
  const hi = sample[Math.floor(sample.length * 0.98)];
  return { min: lo, max: hi > lo ? hi : lo + 1, log };
}
