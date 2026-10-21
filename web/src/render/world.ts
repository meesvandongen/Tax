// Three.js view of the simulated city. Pure presentation: everything drawn
// is derived from the per-cell snapshot the worker sends.

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
  FLAG_WATER,
  type Frame,
  LandUse,
  type SimEvent,
} from '../sim/protocol';
import { BUILDING, DERELICT, GROUND, mix, OVERLAY_DEFS, ramp, type RGB } from './palette';

export const VSCALE = 0.045; // metres → world units (1 unit = one 40 m cell, ×1.8 exaggeration)
export const FLOOR_H = 0.14;

function hash(i: number, k: number): number {
  let h = (i * 374761393 + k * 668265263) | 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
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

export class WorldView {
  readonly renderer: THREE.WebGLRenderer;
  readonly scene = new THREE.Scene();
  readonly camera: THREE.PerspectiveCamera;
  readonly controls: MapControls;
  private w = 0;
  private h = 0;
  private heights: Float32Array = new Float32Array(0);
  private cellY: Float32Array = new Float32Array(0);
  private terrain?: THREE.Mesh;
  private groundTex?: THREE.DataTexture;
  private groundData: Uint8Array = new Uint8Array(0);
  private buildings?: THREE.InstancedMesh;
  private roofs?: THREE.InstancedMesh;
  private trees?: THREE.InstancedMesh;
  private bushes?: THREE.InstancedMesh;
  private tents?: THREE.InstancedMesh;
  private cranes?: THREE.InstancedMesh;
  private bridges?: THREE.InstancedMesh;
  private fences?: THREE.InstancedMesh;
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

    const hl = new THREE.EdgesGeometry(new THREE.BoxGeometry(1, 1, 1));
    this.highlight = new THREE.LineSegments(hl, new THREE.LineBasicMaterial({ color: 0xffffff }));
    this.highlight.visible = false;
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

  setWorld(w: number, h: number, heights: Float32Array, center?: [number, number]) {
    this.dispose();
    this.w = w;
    this.h = h;
    this.heights = heights;
    this.cellY = new Float32Array(w * h);
    for (let i = 0; i < w * h; i++) this.cellY[i] = Math.max(heights[i], 0.6) * VSCALE;

    this.buildTerrain();
    this.buildInstances();

    const [cx, cy] = center ?? [w / 2, h / 2];
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
      this.buildings,
      this.roofs,
      this.trees,
      this.bushes,
      this.tents,
      this.cranes,
      this.bridges,
      this.fences,
      this.traffic,
      this.movers,
      this.smog,
    ]) {
      if (!o) continue;
      this.scene.remove(o);
      (o as THREE.Mesh).geometry?.dispose();
    }
    for (const p of this.pulses) this.scene.remove(p.mesh);
    this.pulses = [];
    this.moverList = [];
  }

  private buildTerrain() {
    const { w, h } = this;
    const vw = w + 1;
    const vh = h + 1;
    const pos = new Float32Array(vw * vh * 3);
    const uv = new Float32Array(vw * vh * 2);
    for (let y = 0; y < vh; y++) {
      for (let x = 0; x < vw; x++) {
        let sum = 0;
        let n = 0;
        let water = false;
        for (const [dx, dy] of [
          [-1, -1],
          [0, -1],
          [-1, 0],
          [0, 0],
        ]) {
          const cx = x + dx;
          const cy = y + dy;
          if (cx < 0 || cy < 0 || cx >= w || cy >= h) continue;
          const hh = this.heights[cy * w + cx];
          if (hh <= 0.5) water = true;
          sum += hh;
          n++;
        }
        const k = y * vw + x;
        const height = water ? -0.25 : (sum / Math.max(n, 1)) * VSCALE;
        pos[k * 3] = x;
        pos[k * 3 + 1] = height;
        pos[k * 3 + 2] = y;
        uv[k * 2] = x / w;
        uv[k * 2 + 1] = y / h;
      }
    }
    const idx: number[] = [];
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const a = y * vw + x;
        const b = a + 1;
        const c = a + vw;
        const d = c + 1;
        idx.push(a, c, b, b, c, d);
      }
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
    geo.setAttribute('uv', new THREE.BufferAttribute(uv, 2));
    geo.setIndex(idx);
    geo.computeVertexNormals();

    this.groundData = new Uint8Array(w * h * 4);
    this.groundTex = new THREE.DataTexture(this.groundData, w, h, THREE.RGBAFormat);
    this.groundTex.magFilter = THREE.NearestFilter;
    this.groundTex.minFilter = THREE.LinearFilter;
    this.groundTex.colorSpace = THREE.SRGBColorSpace;
    this.groundTex.flipY = false;
    this.groundTex.needsUpdate = true;
    const mat = new THREE.MeshStandardMaterial({ map: this.groundTex, roughness: 0.95, metalness: 0 });
    this.terrain = new THREE.Mesh(geo, mat);
    this.terrain.receiveShadow = true;
    this.scene.add(this.terrain);

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
    // PlaneGeometry UVs run v=1 at -z; flip to match grid rows.
    const suv = smogGeo.getAttribute('uv') as THREE.BufferAttribute;
    for (let i = 0; i < suv.count; i++) suv.setY(i, 1 - suv.getY(i));
    this.smog = new THREE.Mesh(
      smogGeo,
      new THREE.MeshBasicMaterial({ map: this.smogTex, transparent: true, depthWrite: false }),
    );
    this.smog.position.set(w / 2, 4.2, h / 2);
    this.smog.renderOrder = 10;
    this.scene.add(this.smog);
  }

  private buildInstances() {
    const n = this.w * this.h;
    const std = (color: number, extra: Partial<THREE.MeshStandardMaterialParameters> = {}) =>
      new THREE.MeshStandardMaterial({ color, roughness: 0.8, metalness: 0.05, ...extra });

    const box = new THREE.BoxGeometry(1, 1, 1);
    box.translate(0, 0.5, 0);
    this.buildings = new THREE.InstancedMesh(box, std(0xffffff), n);
    this.buildings.castShadow = true;
    this.buildings.receiveShadow = true;

    const roof = new THREE.ConeGeometry(0.62, 0.35, 4, 1);
    roof.rotateY(Math.PI / 4);
    roof.translate(0, 0.175, 0);
    this.roofs = new THREE.InstancedMesh(roof, std(0xffffff), n);
    this.roofs.castShadow = true;

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

    const bridge = new THREE.BoxGeometry(1, 0.06, 0.7);
    this.bridges = new THREE.InstancedMesh(bridge, std(0x777777), 4000);

    const fenceMesh = new THREE.BoxGeometry(0.92, 0.06, 0.92);
    this.fences = new THREE.InstancedMesh(fenceMesh, std(0x9c7a4a, { transparent: true, opacity: 0.5 }), 8000);

    for (const m of [this.buildings, this.roofs, this.trees, this.bushes, this.tents, this.cranes, this.bridges, this.fences]) {
      m.count = 0;
      m.frustumCulled = false;
      this.scene.add(m);
    }

    const tGeo = new THREE.BufferGeometry();
    tGeo.setAttribute('position', new THREE.BufferAttribute(new Float32Array(3000 * 3), 3));
    this.traffic = new THREE.Points(
      tGeo,
      new THREE.PointsMaterial({ color: 0xffe9a0, size: 0.18, sizeAttenuation: true }),
    );
    this.traffic.frustumCulled = false;
    this.scene.add(this.traffic);

    const mGeo = new THREE.BufferGeometry();
    mGeo.setAttribute('position', new THREE.BufferAttribute(new Float32Array(600 * 3), 3));
    mGeo.setAttribute('color', new THREE.BufferAttribute(new Float32Array(600 * 3), 3));
    this.movers = new THREE.Points(
      mGeo,
      new THREE.PointsMaterial({ size: 0.45, sizeAttenuation: true, vertexColors: true }),
    );
    this.movers.frustumCulled = false;
    this.scene.add(this.movers);
  }

  // ------------------------------------------------------------- per frame

  update(frame: Frame, events: SimEvent[]) {
    this.lastFrame = frame;
    this.commuters = frame.commuters;
    this.paint();
    for (const e of events.slice(-250)) this.spawnEvent(e);
  }

  /** Repaint ground, buildings and vegetation from the last frame. */
  paint() {
    const frame = this.lastFrame;
    if (!frame || !this.buildings) return;
    const { w, h } = this;
    const n = w * h;
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

    const gd = this.groundData;
    const m4 = new THREE.Matrix4();
    const col = new THREE.Color();
    const q = new THREE.Quaternion();
    const pos = new THREE.Vector3();
    const scl = new THREE.Vector3();
    let nb = 0;
    let nr = 0;
    let nt = 0;
    let nbu = 0;
    let nte = 0;
    let ncr = 0;
    let nbr = 0;
    let nfe = 0;
    const smog = this.smogTex!.image.data as Uint8Array;
    const pollutionK = OVERLAY_DEFS.findIndex((d) => d.key === 'pollution');
    const pollution = frame.overlays.subarray(pollutionK * n, (pollutionK + 1) * n);

    for (let i = 0; i < n; i++) {
      const o = i * CELL_STRIDE;
      const use = cells[o + C_USE];
      const floors = cells[o + C_FLOORS];
      const cond = cells[o + C_CONDITION] / 255;
      const occ = cells[o + C_OCCUPANCY] / 255;
      const nature = cells[o + C_NATURE] / 255;
      const tents = cells[o + C_TENTS];
      const flags = cells[o + C_FLAGS];
      const x = i % w;
      const z = (i / w) | 0;
      const y0 = this.cellY[i];

      // ---- ground colour
      let g: RGB = GROUND[use] ?? [1, 0, 1];
      if (use === LandUse.Wild) g = mix([0.5, 0.6, 0.33], [0.2, 0.38, 0.18], nature);
      else if (use === LandUse.Farm) g = (x + z) % 2 === 0 ? [0.74, 0.68, 0.36] : [0.62, 0.62, 0.3];
      else if (use === LandUse.Vacant) g = mix([0.5, 0.44, 0.32], [0.45, 0.5, 0.28], nature);
      const shade = 0.9 + 0.2 * Math.min(1, this.heights[i] / 50);
      g = [g[0] * shade, g[1] * shade, g[2] * shade];
      const oc = use !== LandUse.Water && use !== LandUse.Road ? ovColor(i) : null;
      if (oc) g = mix(g, oc, 0.85);
      gd[i * 4] = Math.round(Math.min(1, g[0]) * 255);
      gd[i * 4 + 1] = Math.round(Math.min(1, g[1]) * 255);
      gd[i * 4 + 2] = Math.round(Math.min(1, g[2]) * 255);
      gd[i * 4 + 3] = 255;

      // ---- smog
      const p = this.showSmog ? Math.min(1, pollution[i] / 6) : 0;
      smog[i * 4] = 120;
      smog[i * 4 + 1] = 110;
      smog[i * 4 + 2] = 90;
      smog[i * 4 + 3] = Math.round(p * 150);

      // ---- bridges
      if (use === LandUse.Road && flags & FLAG_WATER && nbr < 4000) {
        pos.set(x + 0.5, 0.12, z + 0.5);
        m4.compose(pos, q.identity(), scl.set(1, 1, 1));
        this.bridges!.setMatrixAt(nbr++, m4);
      }

      // ---- buildings
      const isBuilding =
        use === LandUse.Residential || use === LandUse.Commercial || use === LandUse.Industrial || use === LandUse.Civic;
      if (isBuilding || use === LandUse.Construction) {
        const derelict = (flags & FLAG_DERELICT) !== 0;
        const fl = Math.max(1, floors);
        let base: RGB = BUILDING[use] ?? [1, 1, 1];
        if (use === LandUse.Construction) {
          // Scaffold grows with progress (unknown here): draw a low translucent block plus a crane.
          base = BUILDING[LandUse.Construction];
        }
        // Vary tint by hash; age/condition darkens; empty buildings look dim.
        const jitter = 0.9 + 0.2 * hash(i, 1);
        let c: RGB = [base[0] * jitter, base[1] * jitter, base[2] * jitter];
        c = mix(DERELICT, c, Math.min(1, 0.25 + cond));
        if (isBuilding && use !== LandUse.Civic) c = mix(c, [c[0] * 0.55, c[1] * 0.55, c[2] * 0.6], 1 - occ);
        if (derelict) c = mix(c, DERELICT, 0.6);
        const oc2 = ovColor(i);
        if (oc2) c = oc2;

        const footprint = use === LandUse.Industrial ? 0.86 : use === LandUse.Residential && fl <= 2 ? 0.62 : 0.78;
        const height = use === LandUse.Construction ? Math.max(0.25, fl * FLOOR_H * 0.35) : fl * FLOOR_H;
        const rot = use === LandUse.Residential && fl <= 3 ? hash(i, 2) * 0.3 - 0.15 : 0;
        q.setFromAxisAngle(new THREE.Vector3(0, 1, 0), rot);
        pos.set(x + 0.5, y0, z + 0.5);
        scl.set(footprint, height, footprint);
        m4.compose(pos, q, scl);
        this.buildings.setMatrixAt(nb, m4);
        this.buildings.setColorAt(nb, col.setRGB(c[0], c[1], c[2], THREE.SRGBColorSpace));
        nb++;

        if (use === LandUse.Residential && fl <= 3 && !oc2) {
          pos.set(x + 0.5, y0 + height, z + 0.5);
          scl.set(footprint, 1, footprint);
          m4.compose(pos, q, scl);
          this.roofs!.setMatrixAt(nr, m4);
          const rc: RGB = derelict ? [0.3, 0.25, 0.22] : hash(i, 3) > 0.5 ? [0.62, 0.27, 0.2] : [0.42, 0.3, 0.26];
          this.roofs!.setColorAt(nr, col.setRGB(rc[0], rc[1], rc[2], THREE.SRGBColorSpace));
          nr++;
        }
        if (use === LandUse.Construction && ncr < 4000) {
          const ch = Math.max(1.2, fl * FLOOR_H * 1.15);
          pos.set(x + 0.15, y0, z + 0.15);
          m4.compose(pos, q.identity(), scl.set(1, ch, 1));
          this.cranes!.setMatrixAt(ncr++, m4);
        }
      }

      // ---- vegetation
      let treesHere = 0;
      let bushesHere = 0;
      if (use === LandUse.Wild) treesHere = Math.round(nature * 4 - 0.6);
      else if (use === LandUse.Park) treesHere = 3;
      else if (use === LandUse.Residential && floors <= 2) treesHere = nature > 0.15 ? 1 : 0;
      if (use === LandUse.Vacant || (use === LandUse.Wild && flags & FLAG_SPECULATIVE)) bushesHere = Math.round(nature * 5);
      if (use === LandUse.Wild && nature < 0.45) bushesHere = 1;
      for (let k = 0; k < treesHere && nt < this.trees!.instanceMatrix.count; k++) {
        const tx = use === LandUse.Residential ? x + 0.12 + 0.1 * hash(i, 9) : x + 0.15 + 0.7 * hash(i, 10 + k);
        const tz = use === LandUse.Residential ? z + 0.85 : z + 0.15 + 0.7 * hash(i, 20 + k);
        const s = 0.7 + 0.6 * hash(i, 30 + k);
        pos.set(tx, y0, tz);
        m4.compose(pos, q.identity(), scl.set(s, s * (0.9 + nature * 0.5), s));
        this.trees!.setMatrixAt(nt, m4);
        const tc: RGB = mix([0.16, 0.34, 0.14], [0.27, 0.45, 0.2], hash(i, 40 + k));
        const dead = this.showSmog ? Math.min(0.7, pollution[i] / 10) : 0;
        const tc2 = mix(tc, [0.45, 0.4, 0.25], dead);
        this.trees!.setColorAt(nt, col.setRGB(tc2[0], tc2[1], tc2[2], THREE.SRGBColorSpace));
        nt++;
      }
      for (let k = 0; k < bushesHere && nbu < this.bushes!.instanceMatrix.count; k++) {
        pos.set(x + 0.1 + 0.8 * hash(i, 50 + k), y0, z + 0.1 + 0.8 * hash(i, 60 + k));
        const s = 0.6 + 0.8 * hash(i, 70 + k);
        m4.compose(pos, q.identity(), scl.set(s, s, s));
        this.bushes!.setMatrixAt(nbu, m4);
        const bc: RGB = mix([0.5, 0.52, 0.25], [0.35, 0.45, 0.2], hash(i, 80 + k));
        this.bushes!.setColorAt(nbu, col.setRGB(bc[0], bc[1], bc[2], THREE.SRGBColorSpace));
        nbu++;
      }
      // Fenced idle lots: land held empty inside the built-up area.
      if (flags & FLAG_SPECULATIVE && nfe < 8000) {
        pos.set(x + 0.5, y0 + 0.03, z + 0.5);
        m4.compose(pos, q.identity(), scl.set(1, 1, 1));
        this.fences!.setMatrixAt(nfe++, m4);
      }
      // ---- tents
      for (let k = 0; k < tents && nte < this.tents!.instanceMatrix.count; k++) {
        pos.set(x + 0.15 + 0.7 * hash(i, 90 + k), y0, z + 0.15 + 0.7 * hash(i, 100 + k));
        m4.compose(pos, q.identity(), scl.set(1, 1, 1));
        this.tents!.setMatrixAt(nte, m4);
        const tc: RGB = hash(i, 110 + k) > 0.5 ? [0.85, 0.45, 0.15] : [0.2, 0.4, 0.75];
        this.tents!.setColorAt(nte, col.setRGB(tc[0], tc[1], tc[2], THREE.SRGBColorSpace));
        nte++;
      }
    }

    const finish = (m: THREE.InstancedMesh, count: number) => {
      m.count = count;
      m.instanceMatrix.needsUpdate = true;
      if (m.instanceColor) m.instanceColor.needsUpdate = true;
    };
    finish(this.buildings, nb);
    finish(this.roofs!, nr);
    finish(this.trees!, nt);
    finish(this.bushes!, nbu);
    finish(this.tents!, nte);
    finish(this.cranes!, ncr);
    finish(this.bridges!, nbr);
    finish(this.fences!, nfe);
    this.groundTex!.needsUpdate = true;
    this.smogTex!.needsUpdate = true;
    this.smog!.visible = this.showSmog;
  }

  // ---------------------------------------------------------------- effects

  private cellPos(cell: number, lift = 0.2): THREE.Vector3 {
    const x = cell % this.w;
    const z = (cell / this.w) | 0;
    return new THREE.Vector3(x + 0.5, (this.cellY[cell] ?? 0) + lift, z + 0.5);
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

    // commuter traffic: L-shaped trips home → work, staggered phases
    if (this.traffic) {
      const posAttr = this.traffic.geometry.getAttribute('position') as THREE.BufferAttribute;
      const pairs = this.commuters.length / 2;
      const n = this.showTraffic ? Math.min(pairs, posAttr.count) : 0;
      const w = this.w;
      for (let k = 0; k < n; k++) {
        const home = this.commuters[2 * k];
        const job = this.commuters[2 * k + 1];
        const hx = (home % w) + 0.5;
        const hz = ((home / w) | 0) + 0.5;
        const jx = (job % w) + 0.5;
        const jz = ((job / w) | 0) + 0.5;
        const phase = (time * 0.12 + hash(k, 7)) % 1;
        // there and back again
        const t = phase < 0.5 ? phase * 2 : 2 - phase * 2;
        const dx = Math.abs(jx - hx);
        const dz = Math.abs(jz - hz);
        const total = dx + dz || 1;
        const d = t * total;
        let px: number;
        let pz: number;
        if (d <= dx) {
          px = hx + Math.sign(jx - hx) * d;
          pz = hz;
        } else {
          px = jx;
          pz = hz + Math.sign(jz - hz) * (d - dx);
        }
        const cell = Math.min(this.cellY.length - 1, Math.max(0, Math.floor(pz) * w + Math.floor(px)));
        posAttr.setXYZ(k, px, (this.cellY[cell] ?? 0) + 0.08, pz);
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
    if (this.buildings) targets.push(this.buildings);
    const hit = ray.intersectObjects(targets, false)[0];
    if (!hit) return null;
    const x = Math.floor(hit.point.x - (hit.object === this.buildings ? hit.face!.normal.x * 0.01 : 0));
    const z = Math.floor(hit.point.z - (hit.object === this.buildings ? hit.face!.normal.z * 0.01 : 0));
    if (x < 0 || z < 0 || x >= this.w || z >= this.h) return null;
    return z * this.w + x;
  }

  setHighlight(cell: number | null) {
    if (cell === null || !this.lastFrame) {
      this.highlight.visible = false;
      return;
    }
    const o = cell * CELL_STRIDE;
    const floors = this.lastFrame.cells[o + C_FLOORS];
    const hgt = Math.max(0.2, floors * FLOOR_H + 0.05);
    const p = this.cellPos(cell, 0);
    this.highlight.position.set(p.x, p.y + hgt / 2, p.z);
    this.highlight.scale.set(1, hgt, 1);
    this.highlight.visible = true;
  }

  focus(cell: number) {
    const p = this.cellPos(cell, 0);
    const delta = p.clone().sub(this.controls.target);
    this.controls.target.add(delta);
    this.camera.position.add(delta);
  }
}

/** 2nd–98th percentile range over non-water cells. */
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
