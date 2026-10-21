# Technology research: browser-playable 3D agent-based city / tax simulator

*Research date: 2026-10-21. Status: recommendation for review.*

## TL;DR

**Recommended stack:**

- **Simulation core:** one Rust workspace. A pure `sim-core` crate (no JS, no I/O) compiles to
  - `wasm32-unknown-unknown`, which runs in a **Web Worker** in the browser, and
  - native, for a headless CLI used for parameter sweeps and CI.
  - The browser build is **single-threaded at first**. That means stable Rust, no `SharedArrayBuffer`, and no COOP/COEP header requirement.
- **Parallelism:** comes from running many independent simulations at once (native, `rayon`), not from threads inside one tick.
- **Rendering and UI:** **TypeScript + Vite + Three.js `WebGPURenderer`**, which falls back to WebGL2 automatically. The tax-comparison UI and charts are ordinary DOM.
- **Bevy:** not for v1. Raw WebGPU: no.
- **GPU work:** the GPU is for *visualisation* (instancing, field heatmaps, terrain). It does **not** run the authoritative simulation, so the simulation stays bit-reproducible.
- **Data flow:** simulation → render via **transferable `ArrayBuffer`s in a buffer pool**. `SharedArrayBuffer` is an optional later upgrade.
- **Determinism:** seeded named PRNG (PCG / xorshift / ChaCha8, never `StdRng`), with golden-hash CI that compares the native build against the wasm build.
- **Tax policies:** a batch-oriented `TaxPolicy` trait (trait objects plus a serde-tagged config enum as the factory).
- **Target budget (mid laptop):**
  - 100k agents on a 512×512 parcel grid at **≥10 ticks/s normal speed**, using ≤30% of one core.
  - **≥30 ticks/s fast-forward.**
  - **60 fps rendering** of ≤50k visible buildings on an integrated GPU.
  - **≥300 headless 50-year runs/hour** on an 8-core machine.

**Benchmark run for this report** (section 1.2; sources in `docs/research/bench-wasm-vs-js/`):

- Rust→wasm ran at **1.2–1.4× native time**.
- Carefully written TypeScript-style typed-array JavaScript ran at **2–3× native time on V8** and **1.5–3× on JavaScriptCore**.
- All five builds produced **bit-identical state hashes**: native, native with `target-cpu`, wasm, wasm+SIMD, and JS. That directly supports the "one crate, two targets, reproducible" plan.

---

## 0. What the workload actually costs

Before choosing a language, it helps to size the problem:

| Item | Rough size |
|---|---|
| 100k agents × ~200 B of SoA state | ~20 MB |
| 512×512 parcels × ~64 B | ~16 MB |
| Land-value / pollution fields, 512² × f32 × a few layers | ~1 MB each |
| Building instances to draw | 10k–100k |

**Memory is a non-issue.** It is far below the 4 GB wasm32 limit, so memory64 is irrelevant (§1.5).

**Cost is dominated by algorithms, not by language:**

- market matching (households ↔ dwellings, workers ↔ jobs, land auctions),
- commute/accessibility computation,
- how often each agent re-decides.

A 5-point diffusion stencil over 262k cells costs about 1 ms on one CPU core.

**Lessons from shipped games:**

- Cities: Skylines II's well-publicised performance problems came from **rendering**: missing LODs, no occlusion culling, and a shadow pass making 72% of draw calls. Its DOTS simulation measured only about 1.5 ms of compute. ([Paavo Huhtala analysis](https://blog.paavo.me/cities-skylines-2-performance/))
- **Implication:** budget engineering effort for render LOD/culling and for the economic algorithms at least as much as for raw language speed.

---

## 1. Simulation core language

### 1.1 Options

| Option | Speed (relative) | Same code headless? | Threads in browser | Ergonomics / risk |
|---|---|---|---|---|
| **TypeScript, SoA typed arrays** | 2–3× slower than wasm on V8 for this kind of kernel (my benchmark); JIT deopt and GC-pause risk once code gets polymorphic | Yes (Node/Bun/Deno) | Workers + SAB, manual | Fastest iteration; one language; no SIMD in JS; perf cliffs are invisible |
| **Rust → wasm** | ~1.2–1.4× native (my benchmark); 2019 SPEC study found 1.45–1.55× on large programs | **Yes: same crate native** | Requires **nightly + `-Zbuild-std`** + COOP/COEP | Strong types for complex economic logic, `cargo test`, `rayon` natively; slower compiles; wasm-bindgen governance change |
| **C++ → Emscripten** | Similar to Rust→wasm | Yes | **Mature pthreads on stable toolchain** | No memory safety; weaker package story; choose only with existing C++ expertise |
| **Native desktop only** | Fastest (threads, AVX) | n/a | n/a | Loses "browser-playable"; could later be added *as well* via Tauri wrapping the same web UI |

### 1.2 WASM vs JS vs native: evidence

**Literature:**

- **Jangda et al., "Not So Fast" (USENIX ATC 2019).** On SPEC CPU, wasm ran on average **45% (Firefox) to 55% (Chrome) slower than native**, peaking at 2.08–2.5×. Earlier "within 10%" claims came from roughly 100-line scientific kernels. Engines have improved since, but this remains the most rigorous large-program study. ([USENIX](https://usenix.org/conference/atc19/presentation/jangda), [arXiv](https://arxiv.org/abs/1901.09056))
- **Recent blog benchmarks** report wasm 2–10× faster than plain JS on numeric work, but only about 4× faster than JS that already uses typed arrays. Boundary-crossing overhead can erase the win when calls are chatty. These are directional only and the methodologies are unpublished. ([Anurag Nandi 2026](https://www.anuragnandi.com/blog/replaced-slowest-javascript-with-webassembly-2026), [Rust+wasm SIMD benchmark](https://dev.to/bence_rcz_fe471c168707c1/rust-webassembly-performance-javascript-vs-wasm-bindgen-vs-raw-wasm-with-simd-4pco))
- **SoA vs object layout** in JS ECS: claimed speedups of 14–24× over OOP style ([Ankur Sethi](https://ankursethi.com/blog/data-locality-sometimes-beats-algorithmic-complexity/)). Data layout matters more than language.

**My own benchmark (this report):**

- **Kernel:** an ABM-like toy representative of the planned simulation. Each tick:
  1. 5-point land-value diffusion on a W×W grid with occupancy pressure;
  2. every agent samples 4 random candidate parcels (random memory access), evaluates a utility, relocates on >5% gain, then pays income tax plus land-value tax and rent.
- **Implementation:** identical algorithm in Rust (SoA `Vec`s, raw `extern "C"` exports) and in hand-optimised JS (SoA typed arrays, monomorphic).
- **Hardware:** a 2.1 GHz cloud Xeon vCPU, which is slower per core than a current laptop.
- **Engines:** V8 = Node 22 (V8 12.4); JavaScriptCore = Bun 1.4.2. SpiderMonkey was not tested.

ms per tick:

| Agents / grid | native | native `target-cpu=native` | wasm (V8) | wasm+simd128 (V8) | JS SoA (V8) | wasm (JSC) | JS SoA (JSC) |
|---|---|---|---|---|---|---|---|
| 50k / 256² | 2.46 | 2.33 | 2.79 | 2.85 | 7.46 | 2.01 | 3.79 |
| 200k / 512² | 11.9 | 11.4 | 14.6 | 15.5 | 32.2 | 16.2 | 24.5 |
| 1M / 1024² | 71.0 | 64.1 | 91.1 | 83.1 | 198.8 | 93.1 | 200.1 |

**Observations:**

1. **Wasm is about 1.2–1.3× native** on this memory-bound, random-access workload, and about 2.2× faster than the best-case JS on V8.
   - Real-world JS (closures, objects, polymorphic call sites as the model grows) usually does *worse* than this best case.
   - Rust gets *better* with `target-cpu=native` and threads, which only apply natively.
2. **Autovectorised `simd128` gave nothing** for this scattered-access loop. It helped slightly on the 1M case, where the stencil is a bigger share of the work. SIMD pays off only for dense field operations (diffusion, convolution, bulk tax arithmetic), and those are a small share of the total.
3. **Bit-identical results everywhere:**
   - All builds produced the same FNV hash of the full state (`14449067` for the 200k case), including the JS port.
   - This holds because the kernel uses only IEEE `+ − × ÷` on `f64`, integer xorshift, and fixed iteration order.
   - Determinism across native and wasm is achievable, provided the rules in §3.5 are followed.
4. **The stripped wasm module was 22 KB (9.4 KB gzipped).** Sim-core size is not a concern.

### 1.3 WebAssembly feature status (late 2026)

From the official feature table data ([webassembly.org/features](https://webassembly.org/features/), raw [`features.json`](https://raw.githubusercontent.com/WebAssembly/website/main/features.json)):

| Feature | Chrome | Firefox | Safari | Relevance |
|---|---|---|---|---|
| Fixed-width SIMD (128-bit) | 91 | 89 | 16.4 | Usable everywhere; enable `+simd128` (no runtime feature detection in wasm, see [core::arch::wasm32](https://doc.rust-lang.org/core/arch/wasm32/)) |
| Threads + atomics | 74 | 79 | 14.1 | Needs cross-origin isolation (§1.4) |
| Relaxed SIMD | 114 | 145 | flag only | **Avoid**: not in Safari, and *non-deterministic across CPUs by design* ([WebAssembly nondeterminism](https://www.webassembly.org/docs/nondeterminism/)) |
| Memory64 | 133 | 134 | flag (STP) | Avoid (§1.5) |
| GC (WasmGC) | 119 | 120 | 18.2 | Irrelevant for Rust/C++ (§1.6) |
| Exception handling (final) | 137 | 131 | 18.4 | Minor |
| Tail calls | 112 | 121 | 18.2 | Minor |
| JSPI | 137 | 153 | 27 | Not needed (sim is synchronous) |

WebAssembly 3.0 (memory64, GC, exceptions, and more) was finalised in September 2025 ([announcement](https://webassembly.org/news/2025-09-17-wasm-3.0)).

### 1.4 WASM threads, SharedArrayBuffer, and hosting

**Browser requirements:**

- `SharedArrayBuffer`, and so wasm shared memory, needs the page to be **cross-origin isolated**: `Cross-Origin-Opener-Policy: same-origin` plus `Cross-Origin-Embedder-Policy: require-corp` (or `credentialless`).
- With `require-corp`, every cross-origin subresource (CDN fonts, analytics, embeds) must opt in, or it is blocked. ([MDN COEP](https://developer.mozilla.org/docs/Web/HTTP/Reference/Headers/Cross-Origin-Embedder-Policy))
- `credentialless` is supported in Chromium and in Firefox ≥119 (desktop). It appears **not** to be implemented in Safari: the [WebKit bug](https://bugs.webkit.org/show_bug.cgi?id=230550) was still open in 2025. So `require-corp` remains the portable choice.

**GitHub Pages cannot set headers.** It is a long-standing feature request with no ETA ([community discussion #13309](https://github.com/orgs/community/discussions/13309)). The workaround is [**coi-serviceworker**](https://github.com/gzuidhof/coi-serviceworker), a service worker that re-serves responses with COOP/COEP. Costs:

- a forced reload on first visit,
- conflicts with any other service worker,
- it must not be bundled,
- general fragility. ([dev.to write-up](https://dev.to/stefnotch/enabling-coop-coep-without-touching-the-server-2d3n))

Cloudflare Pages and Netlify support a plain `_headers` file, so isolation there is trivial ([example](https://app.cinevva.com/tools/cross-origin-isolation-checker)).

**Rust-specific cost of threads in the browser (still true in late 2026):**

- Rust's prebuilt `std` for `wasm32-unknown-unknown` has no atomics.
- Threads need **nightly**, `-C target-feature=+atomics,+bulk-memory`, `--shared-memory`/`--import-memory` link args, and `-Zbuild-std=panic_abort,std`.
- [wasm-bindgen-rayon](https://github.com/RReverser/wasm-bindgen-rayon) is tested with a pinned `nightly-2025-11-15`. wasm-bindgen 0.2.122 additionally requires `-Clink-arg=--export=__heap_base` on nightlies after 2026-05-06 ([wasm-bindgen releases](https://github.com/wasm-bindgen/wasm-bindgen/releases)).
- `build-std` is **not** slated for stabilisation in 2026. The 2026 project goal covers RFC acceptance and initial implementation only ([Rust 2026 goal: build-std](https://rust-lang.github.io/goals/2026/build-std.html)).
- The main thread can never block, so pool threads must run inside workers.

C++/Emscripten pthreads work on a stable toolchain. This is Emscripten's one concrete advantage.

**Conclusion:** design the browser simulation to be **single-threaded inside one worker** for v1. This alone gives:

- stable Rust;
- no COOP/COEP requirement, so it is host-agnostic (GitHub Pages works);
- easy determinism.

Parallel speed-up is not needed for a 100k-agent target (§5.3). If it later is, either:

- (a) adopt nightly + wasm-bindgen-rayon behind a feature flag with a single-threaded fallback build, or
- (b) shard spatially across N workers, each with its own wasm instance, exchanging boundary data. This is more complex but works on stable.

### 1.5 Memory64

- Shipped in Chrome 133 and Firefox 134; Safari has it only behind a flag.
- 64-bit memories cannot use the guard-page trick, so **every access is explicitly bounds-checked**. SpiderMonkey measured **10% to over 100% slowdowns**, and the JS API caps memories at 16 GB anyway.
- SpiderMonkey's advice: use memory64 only if you truly need more than 4 GB ([SpiderMonkey blog](https://spidermonkey.dev/blog/2025/01/15/is-memory64-actually-worth-using.html)).
- **Verdict:** we need well under 1 GB, so use wasm32.

### 1.6 WasmGC

WasmGC (shipped Chrome 119, Firefox 120, Safari 18.2) lets GC languages (Kotlin, Dart, Java, OCaml) compile to wasm without shipping their own GC. Rust and C++ use linear memory and gain nothing from it, so it does not affect this decision. If anything it makes Kotlin/Wasm or Dart viable, but neither has a strong ABM or 3D ecosystem.

### 1.7 One Rust crate for wasm and native: validated, with caveats

The lean toward one crate is sound. It is how A/B Street is structured: a `sim` crate, a `headless` binary, a `game` GUI, and an `abstio` crate abstracting file I/O for native and web ([A/B Street dev docs](https://a-b-street.github.io/docs/tech/dev/index.html)). My benchmark shows bit-identical results from the same source on both targets.

Rules that make it work:

1. **`sim-core` has zero platform dependencies.**
   - No `wasm-bindgen`, `web-sys`, `std::time`, `std::fs`, threads, or `HashMap` iteration in logic.
   - Inputs and outputs are plain data (commands in; snapshots, metrics, and render buffers out).
   - Thin adapter crates (`sim-wasm`, `sim-cli`) do the I/O.
2. **Parallelism lives outside the core:**
   - natively, `rayon` runs *independent runs* of a parameter sweep in parallel (embarrassingly parallel, and deterministic per run);
   - inside a run, keep logic sequential, or use deterministic data-parallel patterns (§3.5) that behave identically with one thread.
3. **Floating-point discipline** (§3.5): no transcendental `std` functions in state-affecting code, no `mul_add`, no relaxed SIMD, no `f32`/`f64` from `usize`.
4. **CI runs a golden scenario natively and in Node with the wasm build**, then asserts equal state hashes at checkpoints.

**Challenges to watch:**

- `usize` is 32-bit on wasm32 and 64-bit natively. Never let `usize` reach serialized state or RNG sampling; use `u32` explicitly. The [rand reproducibility notes](https://rust-random.github.io/book/crate-reprod.html) say the same.
- Compile times grow with generics-heavy code. Keep `sim-core` lean and avoid pulling in Bevy.
- wasm-bindgen moved to a new `wasm-bindgen` GitHub org after the rustwasm WG sunset (July–September 2025). Other repos (gloo, twiggy, walrus) were archived or handed to maintainers ([Rust blog](https://blog.rust-lang.org/inside-rust/2025/07/21/sunsetting-the-rustwasm-github-org)). wasm-pack now lives at [wasm-bindgen/wasm-pack](https://github.com/wasm-bindgen/wasm-pack) (v0.15.0, 2026), and wasm-bindgen is active (0.2.129, September 2026).
  - The risk is low but real. Keep the JS boundary thin so you could drop to raw `extern "C"` exports, as my benchmark does with zero glue.

### 1.8 Verdict for §1

Rust → wasm for the browser and Rust native for headless runs, from one `sim-core` crate.

TypeScript-only is a **legitimate fallback** if the team is TS-only and the target stays ≤50k agents. It would cost roughly 2–3× the CPU time and lose native-speed sweeps and `rayon`. (TS can also run headless in Node, so the one-codebase property holds there too.)

---

## 2. Rendering

### 2.1 WebGPU availability (October 2026)

From the [gpuweb implementation-status wiki](https://github.com/gpuweb/gpuweb/wiki/implementation-status), plus [web.dev](https://web.dev/blog/webgpu-supported-major-browsers):

| Browser | Windows | macOS | Linux | Android / iOS |
|---|---|---|---|---|
| Chrome / Edge | 113+ (ARM64 behind flag) | 113+ | 144+ Intel Gen12+, 147+ NVIDIA/Wayland; **others behind flags** | Android 121+ (Qualcomm/ARM), 139+ Imagination |
| Firefox | 141+ | 145+ (macOS 26), 147+ Apple Silicon | **Nightly only**; stable "expected 2026" | behind flag |
| Safari | n/a | 26+ | n/a | iOS/iPadOS 26+ |

Other milestones:

- WebGPU became a W3C Candidate Recommendation Draft in March 2026 ([Cinevva roundup](https://app.cinevva.com/signals/2026-03-27-browser-rendering-best-month)).
- Chrome 146 added an opt-in **compatibility mode**, `featureLevel: "compatibility"`, which runs WebGPU on OpenGL ES 3.1-class GPUs ([Chrome blog](https://developer.chrome.com/blog/new-in-webgpu-146)).

**Implication:** a non-trivial share of desktop users still lack WebGPU: Linux Firefox, many Linux Chrome configurations, and older GPUs. My rough estimate is around 10–20%; I found no authoritative telemetry figure. **A WebGL2 fallback path is mandatory**, and the simulation must not depend on WebGPU compute.

### 2.2 Engine comparison

| | Three.js r186 (`0.186.1`) | Babylon.js 9.31 | PlayCanvas 2.23 | Bevy 0.20 (Rust) | Raw WebGPU |
|---|---|---|---|---|---|
| WebGPU | `WebGPURenderer` + TSL; **auto-fallback to WebGL2** | Mature WebGPU + WebGL2 engines (9.0, March 2026) | WebGPU maturing; WebGL2 default | wgpu: WebGPU or WebGL2 (separate builds) | n/a |
| GPU compute | TSL compute (`instancedArray`, `renderer.compute`); WebGL2 fallback *emulates* compute via transform feedback (confirmed in `WebGLBackend.js`), so it is limited | Compute shaders (WebGPU only) | Compute (WebGPU only) | Full wgpu compute (WebGPU only on web) | Full |
| Size (gzipped, before tree-shaking) | measured: WebGL build ≈ **194 KB**, WebGPU build ≈ **309 KB** | larger (~MBs for full core; tree-shakeable ES modules) | ~mid-hundreds of KB | typically **multi-MB**; one reported full-feature game ~22 MB raw / ~7 MB gz / ~5 MB brotli ([HackMD](https://hackmd.io/r15ulD6yR2G6kchYH0RQPw)) | minimal |
| UI for charts/controls | DOM (any framework) | DOM or Babylon GUI | DOM | Bevy UI / egui inside canvas | DOM |
| Ecosystem / docs | Largest; monthly releases with API churn | Strong, Microsoft-backed, stable API | Editor-centric | Growing; breaking release every ~4–6 months | n/a |
| Threads on web | n/a (JS) | n/a | n/a | **single-threaded on wasm** (same nightly constraints as §1.4) | n/a |

Notes and sources:

- **Three.js:**
  - The official manual describes automatic WebGL2 fallback and TSL→WGSL/GLSL transpilation ([manual](https://threejs.org/manual/en/webgpurenderer)).
  - Releases r184–r186 (through September 2026) keep adding compute features (`compileComputeAsync`, compute `updateBefore`/`updateAfter`) ([releases](https://github.com/mrdoob/three.js/releases)).
  - Practitioners consider it production-usable since about r171, with `await renderer.init()` required ([Utsubo guide](https://www.utsubo.com/blog/webgpu-threejs-migration-guide)). They also report Firefox-Linux black-canvas pitfalls ([buildmvpfast](https://www.buildmvpfast.com/blog/threejs-webgl-to-webgpu-renderer-migration-2026)).
  - Raw `ShaderMaterial` GLSL does not run on the WebGPU backend; use TSL node materials.
- **Babylon.js:** [9.0 (March 2026)](https://app.cinevva.com/signals/2026-03-27-browser-rendering-best-month) added clustered lighting and more, and the 9.2x releases keep fixing WebGPU issues ([gamedev.net](https://gamedev.net/news/6140/)).
- **PlayCanvas:** [v2.19 forum post](https://forum.playcanvas.com/t/engine-v2-19-0/42321), [comparison](https://app.cinevva.com/blog/2026-06-09-web-game-engines-2026-comparison).
- **Bevy:**
  - Releases: 0.17 (September 2025), 0.18 (January 2026), 0.19 (June 2026), 0.20 (8 October 2026) ([bevy.org/news](https://bevy.org/news/)).
  - The 0.20 notes contain little web-specific work, and mesh shaders are unsupported on web ([0.20 notes](https://bevy.org/news/bevy-0-20/)).

**Assessment:**

- **Three.js** best fits this project: smallest bundle, biggest community, automatic WebGL2 fallback, and the UI stays in DOM/TypeScript.
  - That matters a lot, because a tax-comparison tool is mostly charts, tables, sliders, and side-by-side scenarios.
- **Babylon.js** is the credible alternative if you prefer a batteries-included engine (GUI, inspector, physics) with stronger API stability. Pick it if three.js's monthly churn becomes painful.
- **Bevy** gives an all-Rust codebase and an ECS, but it is the wrong trade for v1:
  - multi-MB downloads;
  - single-threaded on web anyway;
  - slow full rebuilds;
  - frequent breaking releases;
  - in-canvas UI that is far weaker than the DOM for data-heavy screens;
  - and simulation and render would share one wasm instance on the main thread unless you engineer around it.
  - Revisit Bevy only if a native desktop app becomes the primary product.
- **Raw WebGPU** is a renderer-building project in itself, with no WebGL fallback. No.

### 2.3 Rendering many buildings

- **Instancing:**
  - Use one `InstancedMesh` per building archetype (geometry × material), with per-instance matrix, colour, and height/age attributes. This works when there are few geometries and many instances.
  - `BatchedMesh` handles many distinct geometries in one draw call and has per-instance `setVisibleAt` ([three.js forum](https://discourse.threejs.org/t/how-to-choose-between-instancedmesh-and-batchedmesh/81221)).
  - [three.ez batched-mesh-extensions](https://github.com/agargaro/batched-mesh-extensions/) adds BVH frustum culling and LOD.
- **Procedural buildings:** prefer parametric archetypes (footprint × floors × style) generated in a vertex shader (TSL) from per-instance attributes. Then a parcel update is "write 4 floats", not "rebuild a mesh".
- **Chunking:** split the city into chunks of, say, 32×32 parcels, each with its own instanced batches. Rebuild only dirty chunks, and frustum-cull per chunk. For terrain, use a chunked heightmap with LOD; nature (trees) is instanced impostors at distance.
- **Cities: Skylines II lessons:**
  - LOD everything, including tiny props;
  - limit shadow casters by size and distance (its shadow pass was 72% of draw calls);
  - consider occlusion culling at street level.
  - ([analysis](https://blog.paavo.me/cities-skylines-2-performance/))
- **Agents:** don't render all of them. Cities: Skylines 1 separated 1,048,576 abstract *citizens* from at most 65,536 instantiated *citizen instances* and 16,384 active vehicles ([Steam limit-tool output](https://steamcommunity.com/app/255710/discussions/0/152390648082792963)). Render a sampled, cosmetic subset of moving people and cars, interpolated on the GPU.

### 2.4 GPU compute for the simulation itself?

**Recommendation: no, for authoritative state. Yes, for visualisation.**

- **Determinism:** GPU floating point (fast-math, FMA, reduction order, vendor drivers) is not bit-reproducible across devices. It would also diverge from the native headless runs, which defeats reproducible tax-regime comparisons.
- **Availability:** compute is WebGPU-only in practice; the three.js WebGL2 path only emulates it via transform feedback. That is not acceptable for the simulation itself on the non-WebGPU share of clients.
- **Cost-benefit:** at 512², diffusion is about 1 ms on CPU (§1.2), and GPU readback (`mapAsync`) adds a frame or more of latency plus synchronisation complexity.
- **Where the GPU is worth it:**
  - smoothing and contouring land-value or pollution fields for display;
  - heatmap overlays;
  - animating cosmetic traffic particles;
  - instanced building shaders;
  - terrain.
- The simulation sends the raw field (f32, or a quantised u8/u16 texture) at 1–2 Hz, and TSL does the pretty part. TSL compute with `instancedArray` is the idiomatic path ([example repo](https://github.com/Makio64/advanced-threejs-tsl-webgpu-rendering)).

---

## 3. Architecture patterns

### 3.1 Process layout

```
Main thread (TS)                                 Sim Worker (TS shell + Rust wasm)
┌───────────────────────────────┐   commands    ┌──────────────────────────────────┐
│ UI (DOM framework, charts)    │ ────────────▶ │ command queue (tick-stamped)     │
│ Input → commands              │               │ fixed-step loop: sim.step()      │
│ Three.js WebGPURenderer       │ ◀──────────── │ writes render buffers            │
│ interpolation @ display rate  │  transferable │ metrics/time-series ring buffer  │
│ buffer pool (returns buffers) │  ArrayBuffers │ snapshots / replay log           │
└───────────────────────────────┘               └──────────────────────────────────┘
          optional later: OffscreenCanvas renderer worker; SAB triple-buffer
```

- The simulation runs in a dedicated module worker. It never blocks the UI, and its long ticks can't cause jank.
- Rendering stays on the main thread. Three.js supports `OffscreenCanvas` in a worker ([web.dev](https://web.dev/articles/offscreen-canvas)), but DOM-driven picking and UI integration are simpler on the main thread. Move it only if profiling shows main-thread contention.

### 3.2 Moving state: transferables vs SharedArrayBuffer

| | `postMessage` with transferables | SharedArrayBuffer |
|---|---|---|
| Copy cost | Zero-copy *transfer*; the worker does one memcpy out of wasm memory (≈0.3 ms for 2.4 MB) | Zero-copy reads; needs seqlock/atomics or a triple buffer |
| Requirements | None | COOP/COEP everywhere (§1.4) |
| Wasm memory | Rust wasm memory is a *non-shared* ArrayBuffer unless built with threads (nightly) | To share wasm memory directly you need the threads build |
| Complexity | Low (buffer pool ping-pong) | Medium (sync protocol, tearing) |

([MDN SharedArrayBuffer](https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer), [Mozilla Hacks cartoon intro](https://hacks.mozilla.org/2017/06/a-cartoon-intro-to-arraybuffers-and-sharedarraybuffers/), [Surma's Atomics demo](https://surma.dev/lab/mandelbrot/))

**Recommendation:** use transferables with a pool of 2–3 buffers per stream.

- Per tick, send **deltas**: changed parcels/buildings as compact `(index, archetype, height, flags)` records.
- At 1–2 Hz, send **fields** (land value, pollution) as `Float32Array` or quantised arrays.
- Send per-tick **metrics**: tax revenue by instrument, Gini, rents, density.
- The main thread returns buffers to the worker after uploading them to the GPU.
- In wasm, expose `ptr()`/`len()` for the SoA arrays and build typed-array views on `memory.buffer`. **Re-create views after any memory growth**, because growth detaches old views.
- SAB is an upgrade path only, and only if profiling shows the memcpy matters, which is unlikely at these sizes.

### 3.3 Time model: fixed timestep, decoupled from rendering

- The simulation advances in **fixed economic ticks**, for example 1 tick = 1 week. 52 ticks = 1 year, and a 50-year scenario is 2,600 ticks.
- Rendering runs at display rate and interpolates cosmetic motion. This is the classic accumulator pattern from Glenn Fiedler's ["Fix Your Timestep!"](https://gafferongames.com/post/fix_your_timestep/).
- Speed control means changing ticks per second (pause, 1×, 5×, max). The worker loop runs ticks in **≤8 ms slices** and then yields (`setTimeout(0)` or a `MessageChannel`), so commands are processed promptly.
- **Stagger decisions; don't recompute every agent every tick:**
  - Each agent re-evaluates its location or job on a schedule, e.g. `(agent_id + tick) % 26 == 0`, or by event (rent shock, job loss).
  - A/B Street abandoned fixed 0.1 s timesteps for a **discrete-event priority queue** because fixed-step updates were "lots of busy work" re-checking unchanged conditions ([A/B Street discrete-event design](https://a-b-street.github.io/docs/tech/trafficsim/discrete_event/index.html)).
  - For an economic simulation, a hybrid works well: fixed weekly ticks for markets, accounting and taxes, plus an event queue or staggered schedule for individual decisions.

### 3.4 ECS (bevy_ecs, hecs, bitECS) vs plain SoA

- The agent types are few and stable (persons/households, firms, parcels/buildings, government). Fields are known up front, and queries are "for all households" or "for all parcels".
- **Plain SoA structs** (`Households { income: Vec<f64>, wealth: Vec<f64>, parcel: Vec<u32>, … }`) with **generational indices** for handles (e.g. the `slotmap` pattern, which Kay/Citybound also used) are the best fit. They are fastest, simplest to serialise and snapshot, deterministic by construction, and easy to expose to JS as typed-array views.
- A full ECS earns its keep when entities change composition dynamically, or for heterogeneous gameplay objects. Specifics:
  - [`bevy_ecs`](https://crates.io/crates/bevy_ecs) 0.20 is excellent but brings a scheduler, change detection and a large compile surface, and its parallel scheduler doesn't apply on wasm.
  - [`hecs`](https://crates.io/crates/hecs) 0.11 is a minimal archetype ECS and a fine choice if needed later.
  - [`bitECS`](https://github.com/NateTheGreatt/bitECS) 0.4 is the TS equivalent, relevant only on the TS path.
- **Verdict:** plain SoA in `sim-core`. If the render/UI side wants an entity model, keep it in TS.

### 3.5 Determinism and seeded RNG

Required for fair tax-regime A/B comparisons (common random numbers), for replay, and for native↔wasm parity.

- **RNG:**
  - Use a *named* algorithm with a version-pinned crate: `rand_pcg` 0.10 (Pcg64Mcg / Lcg128Xsl64) or `rand_chacha` 0.10 (ChaCha8Rng).
  - **Never `StdRng` or `thread_rng`.** rand explicitly marks `StdRng` as non-portable across platforms and versions ([rand book](https://rust-random.github.io/book/crate-reprod.html)).
  - Sample `u32`, not `usize`.
- **Stream splitting:** derive per-agent or per-subsystem streams from `(master_seed, stream_id)`. ChaCha has native stream/word-position support; alternatively, hash `(seed, agent_id, tick)` with a counter-based generator. Then:
  - results don't depend on iteration order or thread count;
  - two tax regimes can share the *same* random draws (common random numbers), which sharply reduces the variance of the comparison.
- **Floating point:**
  - IEEE `+ − × ÷ sqrt` are bit-identical across wasm and native.
  - `sin/exp/ln/powf` come from platform libm and can differ in the last ulp. Use the `libm` crate (pure Rust, identical everywhere) or your own polynomials in state-affecting code ([alice-det-math notes](https://docs.rs/alice-det-math)).
  - Avoid `mul_add` (FMA vs non-FMA rounding).
  - Avoid relaxed SIMD.
  - Never branch on NaN payloads; wasm NaN bits are non-deterministic ([spec](https://www.webassembly.org/docs/nondeterminism/), [Wasmtime notes](https://docs.wasmtime.dev/examples-deterministic-wasm-execution.html)).
  - Rust does not contract to FMA implicitly. My benchmark confirms bit-identical results across native x86-64 (including `target-cpu=native`), wasm on V8, and wasm on JSC.
- **Collections:** no `HashMap`/`HashSet` iteration in logic (random SipHash keys). Use `BTreeMap`, `IndexMap` with a fixed hasher, or sorted `Vec`s.
- **Money:** consider `i64` cents (fixed point) for ledgers, which gives exact accounting identities and makes "tax revenue = sum of liabilities" checkable. Utilities and prices can stay in `f64`.
- **Parallel reductions (native, if ever used within a tick):** reduce in fixed chunk order, never in "whichever thread finishes first" order.

### 3.6 Tax policies as plugins

Batch-oriented trait objects, so dynamic dispatch happens once per policy per tick and never per agent:

```rust
/// Read-only view of the world passed to policies each tick.
pub struct WorldView<'a> { pub parcels: &'a Parcels, pub households: &'a Households,
                           pub firms: &'a Firms, pub tick: u32, /* … */ }

pub trait TaxPolicy: Send + Sync {
    fn id(&self) -> &str;
    /// Price signals agents see in their decisions (hot path, vectorisable):
    /// e.g. effective annual tax per parcel for a given use; marginal income-tax rate.
    fn parcel_tax_signal(&self, w: &WorldView, out: &mut [f32]) {}
    fn household_marginal_rate(&self, w: &WorldView, out: &mut [f32]) {}
    /// Event hooks (transfer taxes, stamp duty, capital gains on sale…)
    fn on_land_transfer(&self, w: &WorldView, tx: &LandTransfer) -> Cents { 0 }
    /// Periodic assessment → liabilities written to the ledger.
    fn assess(&self, w: &WorldView, period: Period, ledger: &mut TaxLedger);
}

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
pub enum PolicyConfig {
    LandValueTax { rate: f64, assessment_lag_years: u32 },
    PropertyTax  { rate: f64, improvements_weight: f64 },
    IncomeTax    { brackets: Vec<(Cents, f64)> },
    TransferTax  { rate: f64 },
    // add a variant + impl to add a tax
}
impl PolicyConfig { pub fn build(&self) -> Box<dyn TaxPolicy> { /* match */ } }

pub struct TaxRegime { pub policies: Vec<Box<dyn TaxPolicy>>, pub revenue_use: Box<dyn RevenueUse> }
```

- **Regimes are data** (JSON/RON) that the browser UI edits and the headless CLI reads unchanged. A scenario file plus a seed fully specifies a run.
- The **agent decision code only sees aggregated signals** (`parcel_tax_signal` and similar). Adding a new tax doesn't touch the agent code unless it introduces a genuinely new kind of signal.
- **Revenue recycling** (citizen's dividend, infrastructure, tax cuts) is a separate `RevenueUse` trait. Revenue-neutral comparisons are a core use case.
- **Optional optimisation:** if profiling ever shows dispatch cost, switch `Box<dyn>` to `enum_dispatch`. It is a mechanical change.
- **No runtime-loaded wasm plugins** for v1: version, determinism and security headaches for little benefit.

### 3.7 Snapshots and replay

- **Replay** = `(sim_version, scenario config, seed, command log [(tick, Command)])`. Because the simulation is deterministic, that reproduces the entire run on either target. Tiny, so put it in shareable URLs or files.
- **Snapshots:**
  - `serde` + [`postcard`](https://crates.io/crates/postcard) 1.1 (compact, stable format) of the SoA world, with a version header.
  - Use [`rkyv`](https://crates.io/crates/rkyv) 0.8 only if zero-copy loading of large snapshots becomes necessary.
  - Snapshot every N years for scrubbing and branching ("fork this city and apply regime B").
- **State hash every K ticks** (FNV/xxhash over the SoA bytes, as in my benchmark). This catches desyncs and regressions, and powers the native↔wasm parity test in CI.

### 3.8 How existing projects handle agents and scale

- **Citybound** (Rust):
  - Built on [Kay](https://github.com/aeplay/kay), its own actor system inspired by Erlang and data-oriented design. Actor state and message queues live in contiguous chunks per actor type, with a slot map for stable IDs, aiming at "millions of entities".
  - Every household and business is an actor exchanging messages.
  - Split into `cb_simulation`, `cb_server`, and a `cb_browser_ui` (wasm) client ([repo](https://github.com/citybound/citybound)).
  - Development has largely stalled (last commits around 2023; Kay last released 2020).
  - **Lesson:** actor message passing buys distribution and multiplayer, but makes determinism and reasoning harder. Not worth it for a single-player policy lab.
- **A/B Street** (Rust):
  - Separate `sim` and `headless` crates, a custom `widgetry` GUI (glium/winit) that also targets the web, and **discrete-event** traffic simulation.
  - Savestates restore exact state.
  - Scale handled by contraction-hierarchy pathfinding precomputed at import ([dev docs](https://a-b-street.github.io/docs/tech/dev/index.html), [discrete-event](https://a-b-street.github.io/docs/tech/trafficsim/discrete_event/index.html)).
  - **Lessons:** event-driven or staggered updates beat fixed per-agent ticking; precompute accessibility.
- **Cities: Skylines (1):**
  - Hard caps: 65,536 citizen instances, 16,384 active vehicles, but about 1M abstract citizens.
  - When a cap is hit, agents silently stop being simulated ([Steam](https://steamcommunity.com/app/255710/discussions/0/152390648082792963)).
- **Cities: Skylines II:**
  - Unity DOTS (ECS + Jobs + Burst), no hard agent cap ([80.lv](https://80.lv/articles/cities-skylines-2-doesn-t-have-limit-for-people-it-can-track), [developer comment](https://twt.omada.cafe/r/CitiesSkylines/comments/17ekhtw/comment/k64zldb/?context=3)).
  - Its problems were in rendering (§0).
  - **Lesson:** separate abstract agents (economic state) from instantiated visible agents.
- **SimCity (2013), GlassBox:**
  - Everything reduces to *resources in bins*, *units* (buildings), *maps* (spatial fields such as pollution and land value), *globals*, and *rules* that move resources. *Agents* are deliberately dumb carriers with a destination, so that 10,000+ can run ([Game Developer GDC 2012](https://gamedeveloper.com/design/gdc-2012-breaking-down-em-simcity-em-s-glassbox-engine), [PC Gamer](https://pcgamer.com/simcity-inside-the-glassbox-engine)).
  - The well-known failure: Sims had no persistent home or job and went to the *nearest* vacancy each day. That was visibly wrong and undermined trust in the simulation ([BGR](https://www.bgr.com/general/simcity-pr-nightmare-analysis-372284/), [Human Transit](https://humantransit.org/2013/03/sim-city-will-continue-to-mislead-on-transit.html)).
  - **Lesson for a tax lab:** agents must have **persistent identity, tenure, balance sheets and contracts** (lease, mortgage, employment), or tax incidence results are meaningless. This favours fewer, richer agents (households and firms, ~10⁵) over many shallow ones.
- **ABM frameworks:**
  - [krABMaga](https://docs.rs/crate/krabmaga/latest) is a Rust MASON-style ABM framework with Bevy visualisation ([JASSS 2024](https://www.jasss.org/27/2/4.html)). Worth reading for its scheduling and reproducibility patterns.
  - Its Bevy coupling and generic design are less suited here than a purpose-built SoA core.

---

## 4. Build tooling

| Concern | Recommendation |
|---|---|
| Bindings | **wasm-bindgen 0.2.129** for the control API (init, commands, config as JSON or serde-wasm-bindgen, errors), plus **raw pointer/length exports** for bulk SoA arrays read as typed-array views. Generate TS types for config/commands with `ts-rs` or similar. Raw `extern "C"` alone (as in my benchmark) works and has zero glue, but you would hand-write string/error marshalling. |
| Build driver | `cargo build --target wasm32-unknown-unknown --release` → `wasm-bindgen --target web` → `wasm-opt -O3` (binaryen), via a small script or [wasm-pack 0.15](https://github.com/wasm-bindgen/wasm-pack/releases) (now maintained under the wasm-bindgen org). Pin versions; the `wasm-bindgen` CLI must match the crate version exactly. |
| Bundler | **Vite 8.x**. The `--target web` output imports cleanly; or use [vite-plugin-wasm](https://github.com/Menci/vite-plugin-wasm) 3.6 (add it to `worker.plugins` too). Workers via `new Worker(new URL('./sim.worker.ts', import.meta.url), { type: 'module' })`. |
| Dev loop | `cargo watch` (or `bacon`) rebuilds the wasm crate in seconds when `sim-core` is small; Vite HMR reloads the worker. Most iteration on economic logic happens natively (`cargo test`, `sim-cli` with CSV/Parquet output), which is faster than in-browser. |
| Size profile | `[profile.release] lto = true, codegen-units = 1, opt-level = 3` (speed matters more than bytes here); `panic = "abort"` for wasm. Expect sim wasm ≲ 300 KB gz even with serde ([Leptos size guide](https://book.leptos.dev/deployment/binary_size.html)). |
| COOP/COEP (only if threads are adopted) | Vite `server.headers` / `preview.headers`; prod `_headers` on Cloudflare Pages/Netlify; coi-serviceworker only for GitHub Pages. |
| CI | `cargo test` (native) → build wasm → run golden scenario in Node with the wasm build **and** natively → compare state hashes → bundle-size budget check → Playwright smoke test in Chromium + Firefox (WebGL2 path) + WebKit. |
| Initial bundle budget | three.js WebGPU build ~310 KB gz + app ~200 KB + sim ~300 KB + chart lib ≈ **< 1.5 MB gz total** (vs a Bevy build of several MB). |

---

## 5. Recommendation

### 5.1 Stack

| Layer | Choice | Why |
|---|---|---|
| Simulation core | **Rust**, `sim-core` crate, SoA + generational indices, single-threaded, deterministic | ~2–3× faster than tuned TS in the browser; the same code runs natively for sweeps and CI; strong typing for complex economic rules; bit-identical native/wasm verified |
| Browser host | Rust→wasm32 (+`simd128` optional) in a **module Web Worker**; wasm-bindgen facade | Stable toolchain; no COOP/COEP needed; host-agnostic |
| Headless | `sim-cli` native binary; `rayon` across runs; outputs Parquet/CSV | Parameter sweeps, Monte Carlo over seeds, regression tests |
| Rendering | **Three.js `WebGPURenderer` + TSL** (auto WebGL2 fallback), InstancedMesh/BatchedMesh per archetype, chunked terrain | Smallest/most mature web 3D option; fallback covers Linux Firefox; DOM UI |
| UI | TypeScript + a DOM framework (e.g. Svelte/React/Solid) + a canvas chart library | Charts and side-by-side regime comparison are the product |
| Data flow | Transferable ArrayBuffer pool (deltas per tick, fields at 1–2 Hz, metrics) | Zero hosting constraints; SAB later if ever needed |
| Policies | `TaxPolicy` trait + serde-tagged config enum; `RevenueUse` trait | New taxes = one enum variant + one impl; regimes are data |
| Reproducibility | Named PRNG (PCG/ChaCha8), per-agent streams, `libm`, i64 cents ledger, state hashes, command-log replay, postcard snapshots | Fair A/B comparisons (common random numbers); shareable runs |
| Hosting | Any static host; prefer Cloudflare Pages/Netlify (headers available if threads are later adopted) | |
| Desktop (later, optional) | Tauri shell around the same web UI, with the simulation running native and threaded | Reuses everything |

Suggested layout:

```
crates/sim-core      # pure, deterministic model (agents, markets, land, taxes)
crates/sim-wasm      # wasm-bindgen facade, buffer export
crates/sim-cli       # scenarios, sweeps (rayon), golden-hash tests
web/                 # Vite + TS: worker shell, three.js renderer, UI/charts
scenarios/*.ron|json # tax regimes, maps, seeds
```

### 5.2 Phasing

1. **Spike (1–2 weeks):** `sim-core` with households, parcels, a land market, one tax (LVT) plus income tax; CLI; wasm worker; three.js instanced buildings on a flat grid; golden-hash CI. Measure ms/tick against §5.3.
2. **Model depth:** firms/jobs, rents, construction and redevelopment, revenue recycling, more taxes; snapshots/replay; scenario comparison UI.
3. **World:** terrain/nature, LOD, cosmetic commuters, field overlays (TSL).
4. **Only if measured necessary:** sharded workers or a nightly threads build; SAB; OffscreenCanvas rendering.

### 5.3 Performance budget

**Reference machine ("mid laptop"):** 4P-core / 8–12-thread 2023–2024 CPU (Intel Core Ultra 5 / Ryzen 5 7x40U / Apple M2), 16 GB RAM, integrated GPU (Iris Xe / Radeon 780M / M2 10-core), Chrome or Safari stable.

| Metric | Target | Stretch | Basis |
|---|---|---|---|
| Agents (households + persons + firms) | **100k** | 250k | Toy kernel: 200k agents = 15 ms/tick wasm on a slow 2.1 GHz vCPU; a real model is ~5–10× heavier per agent but staggers decisions |
| Parcel grid | **512 × 512** (262k parcels) | 1024² | Diffusion about 1 ms at 512² |
| Sim tick (1 tick = 1 week), browser | **≤ 30 ms** at 100k (≈300 ns/agent/tick) | ≤ 15 ms | Leaves headroom in one worker core |
| Normal speed | **10 ticks/s** (a simulated year ≈ 5 s), ≤ 30% of one core | — | |
| Fast-forward | **≥ 30 ticks/s** (a 50-year run in < 90 s in-browser) | 60 ticks/s | |
| Headless native | **≤ 20 ms/tick/core** → 50-year run ≈ 50 s/core → **≥ 300 runs/hour on 8 cores** | 1,000 runs/h | Native ≈ 1.3× faster than wasm, plus run-level parallelism |
| Render | **60 fps** (≤ 16.6 ms; main-thread JS ≤ 4 ms, GPU ≤ 12 ms) with ≤ 50k visible buildings, ≤ 300 draw calls, ≤ 2M visible triangles | 120 fps on dGPU | Instancing per archetype, chunk culling, LOD, shadow-caster limits |
| Worker→main traffic | ≤ 1 MB/tick of deltas; fields ≤ 2 Hz | — | Transfer, not copy |
| Memory | wasm heap ≤ 256 MB; tab ≤ 1 GB | — | Fits iOS Safari limits too |
| Download | ≤ 1.5 MB gz first load; time-to-interactive ≤ 3 s on a 50 Mbps link | — | |

Per-agent arithmetic: 30 ms / 100k agents = 300 ns/agent/tick ≈ 1,000 cycles at 3.5 GHz. That is enough for a few cache misses and a utility evaluation over a handful of candidates. It is **not** enough for per-agent pathfinding every tick. Hence:

- precomputed accessibility/commute-cost fields (recomputed when the road network changes),
- spatial buckets for candidate sampling,
- staggered decisions (~1/26 of agents re-decide each tick).

### 5.4 Risk register

| # | Risk | Likelihood / impact | Mitigation |
|---|---|---|---|
| 1 | Economic algorithms (market clearing, matching) scale O(n²) and blow the budget regardless of language | **High / high** | Candidate sampling, spatial hashing, staggered updates, auction batches; benchmark in `sim-cli` from day 1 |
| 2 | WebGPU missing on Linux Firefox and some older Chrome/GPU configurations | Certain / medium | Three.js auto WebGL2 fallback; no simulation dependency on GPU compute; CI tests the WebGL2 path |
| 3 | Rust wasm threads stay nightly-only (`build-std` not stabilising in 2026) | High / low (by design) | Single-threaded browser simulation; parallelism via runs natively; shard workers if needed |
| 4 | Native↔wasm determinism drift (libm, FMA, HashMap, `usize`, parallel reductions) | Medium / high for credibility | Rules in §3.5 + golden-hash CI on both targets from the first commit |
| 5 | wasm-bindgen / wasm-pack maintenance transition (rustwasm org archived 2025) | Low / medium | Thin boundary; pinned versions; raw `extern "C"` fallback proven |
| 6 | Three.js API churn (monthly releases, TSL still evolving) | Medium / medium | Pin version; isolate the renderer behind an adapter module; Babylon.js as plan B |
| 7 | Main-thread jank from charts/UI plus rendering | Medium / medium | Simulation already off-thread; throttle charts; OffscreenCanvas later |
| 8 | Chatty JS↔wasm boundary (per-agent calls) | Medium / medium | One `step(n)` call per batch; bulk views; commands batched |
| 9 | Render LOD/shadows neglected (Cities: Skylines II failure mode) | Medium / high | LOD and shadow-caster limits as acceptance criteria; GPU timing in dev HUD |
| 10 | COOP/COEP needed later (threads/SAB) on a host without headers | Low / low | Choose Cloudflare Pages/Netlify now; coi-serviceworker fallback |
| 11 | Rust learning curve and compile times slow iteration on model logic | Medium / medium | Small `sim-core`; no Bevy; most iteration in native tests; consider a TS prototype of the *model* only if the team is TS-native |
| 12 | Model credibility (GlassBox lesson: shallow agents) | Medium / high | Persistent tenure, balance sheets and contracts; validation scenarios; publish assumptions |

### 5.5 What would change this recommendation

- **The team is TypeScript-only and ≤50k agents is enough:** do everything in TS with SoA typed arrays (and optionally bitECS). Accept the 2–3× CPU cost; headless still runs in Node/Bun.
- **A native desktop app becomes the primary product, with web secondary:** consider Bevy for an all-Rust stack, or Tauri plus the recommended stack with a native threaded simulation.
- **Agents must reach millions and still run in the browser:** then pay for wasm threads (nightly + COOP/COEP host) or a sharded-worker design, and revisit GPU compute for the field-heavy parts, accepting non-bit-exact GPU results with tolerance-based comparisons.

---

## Sources

**WebAssembly and language performance**
- WebAssembly 3.0 announcement: https://webassembly.org/news/2025-09-17-wasm-3.0
- WebAssembly feature status: https://webassembly.org/features/ ; data: https://raw.githubusercontent.com/WebAssembly/website/main/features.json
- Jangda et al., "Not So Fast" (USENIX ATC '19): https://usenix.org/conference/atc19/presentation/jangda ; https://arxiv.org/abs/1901.09056
- SpiderMonkey, "Is Memory64 actually worth using?": https://spidermonkey.dev/blog/2025/01/15/is-memory64-actually-worth-using.html
- WebAssembly nondeterminism: https://www.webassembly.org/docs/nondeterminism/
- Wasmtime deterministic execution: https://docs.wasmtime.dev/examples-deterministic-wasm-execution.html
- alice-det-math (libm/FMA determinism notes): https://docs.rs/alice-det-math
- rand reproducibility: https://rust-random.github.io/book/crate-reprod.html
- Rust `core::arch::wasm32`: https://doc.rust-lang.org/core/arch/wasm32/
- Blog benchmarks: https://www.anuragnandi.com/blog/replaced-slowest-javascript-with-webassembly-2026 ; https://dev.to/bence_rcz_fe471c168707c1/rust-webassembly-performance-javascript-vs-wasm-bindgen-vs-raw-wasm-with-simd-4pco ; https://ankursethi.com/blog/data-locality-sometimes-beats-algorithmic-complexity/
- ECS benchmark (JS): https://github.com/bytro/ecs-benchmark

**Threads, isolation, hosting**
- MDN COEP: https://developer.mozilla.org/docs/Web/HTTP/Reference/Headers/Cross-Origin-Embedder-Policy
- MDN SharedArrayBuffer: https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer
- GitHub Pages header request: https://github.com/orgs/community/discussions/13309
- coi-serviceworker: https://github.com/gzuidhof/coi-serviceworker ; write-up: https://dev.to/stefnotch/enabling-coop-coep-without-touching-the-server-2d3n
- WebKit COEP credentialless bug: https://bugs.webkit.org/show_bug.cgi?id=230550
- Cross-origin isolation checker / `_headers`: https://app.cinevva.com/tools/cross-origin-isolation-checker
- wasm-bindgen-rayon: https://github.com/RReverser/wasm-bindgen-rayon
- Rust 2026 project goal, build-std: https://rust-lang.github.io/goals/2026/build-std.html
- Surma Atomics demo: https://surma.dev/lab/mandelbrot/ ; Mozilla Hacks SAB: https://hacks.mozilla.org/2017/06/a-cartoon-intro-to-arraybuffers-and-sharedarraybuffers/

**Rust/wasm tooling**
- Sunsetting rustwasm org: https://blog.rust-lang.org/inside-rust/2025/07/21/sunsetting-the-rustwasm-github-org
- wasm-bindgen releases: https://github.com/wasm-bindgen/wasm-bindgen/releases
- wasm-pack releases: https://github.com/wasm-bindgen/wasm-pack/releases
- vite-plugin-wasm: https://github.com/Menci/vite-plugin-wasm
- Leptos wasm size guide: https://book.leptos.dev/deployment/binary_size.html

**Rendering**
- gpuweb implementation status: https://github.com/gpuweb/gpuweb/wiki/implementation-status
- web.dev WebGPU in all major browsers: https://web.dev/blog/webgpu-supported-major-browsers
- Chrome 146 WebGPU compatibility mode: https://developer.chrome.com/blog/new-in-webgpu-146
- Firefox 141 WebGPU: https://simonwillison.net/2025/Jul/16/webgpu-firefox
- Three.js WebGPURenderer manual: https://threejs.org/manual/en/webgpurenderer ; releases: https://github.com/mrdoob/three.js/releases
- Three.js migration guides: https://www.utsubo.com/blog/webgpu-threejs-migration-guide ; https://www.buildmvpfast.com/blog/threejs-webgl-to-webgpu-renderer-migration-2026
- InstancedMesh vs BatchedMesh: https://discourse.threejs.org/t/how-to-choose-between-instancedmesh-and-batchedmesh/81221 ; https://github.com/agargaro/batched-mesh-extensions/
- TSL compute examples: https://github.com/Makio64/advanced-threejs-tsl-webgpu-rendering
- Browser rendering roundup, March 2026 (Babylon 9.0, PlayCanvas 2.17, three r183, WebGPU CR): https://app.cinevva.com/signals/2026-03-27-browser-rendering-best-month
- Babylon.js release news: https://gamedev.net/news/6140/
- PlayCanvas: https://forum.playcanvas.com/t/engine-v2-19-0/42321 ; https://app.cinevva.com/blog/2026-06-09-web-game-engines-2026-comparison
- Bevy news and releases: https://bevy.org/news/ ; https://bevy.org/news/bevy-0-20/
- Bevy wasm size anecdote: https://hackmd.io/r15ulD6yR2G6kchYH0RQPw
- OffscreenCanvas: https://web.dev/articles/offscreen-canvas
- Fix Your Timestep: https://gafferongames.com/post/fix_your_timestep/

**Reference projects**
- Citybound: https://github.com/citybound/citybound ; Kay: https://github.com/aeplay/kay
- A/B Street dev docs: https://a-b-street.github.io/docs/tech/dev/index.html ; discrete-event sim: https://a-b-street.github.io/docs/tech/trafficsim/discrete_event/index.html
- Cities: Skylines limits: https://steamcommunity.com/app/255710/discussions/0/152390648082792963
- Cities: Skylines II: https://80.lv/articles/cities-skylines-2-doesn-t-have-limit-for-people-it-can-track ; https://blog.paavo.me/cities-skylines-2-performance/
- SimCity GlassBox: https://gamedeveloper.com/design/gdc-2012-breaking-down-em-simcity-em-s-glassbox-engine ; https://pcgamer.com/simcity-inside-the-glassbox-engine ; https://www.bgr.com/general/simcity-pr-nightmare-analysis-372284/ ; https://humantransit.org/2013/03/sim-city-will-continue-to-mislead-on-transit.html
- krABMaga: https://docs.rs/crate/krabmaga/latest ; JASSS 2024: https://www.jasss.org/27/2/4.html

**Version checks performed 2026-10-21 (npm / crates.io):** three 0.186.1, @babylonjs/core 9.31.2, playcanvas 2.23.3, bitecs 0.4.0, vite 8.3.4, vite-plugin-wasm 3.6.0, bevy/bevy_ecs 0.20.0, hecs 0.11.2, wasm-bindgen 0.2.129, rayon 1.12.0, rand_pcg 0.10.2, rand_chacha 0.10.0, postcard 1.1.3, rkyv 0.8.18, glam 0.34.1. Toolchain used for the benchmark: rustc 1.97.0, Node 22 (V8 12.4), Bun 1.4.2.

## Appendix: reproducing the benchmark

Sources are in `docs/research/bench-wasm-vs-js/` (`kernel.rs` is shared by `native.rs` and `wasm.rs`; `run.mjs` contains the JS port and a wasm runner).

```sh
cd docs/research/bench-wasm-vs-js
rustc -C opt-level=3 -C codegen-units=1 -o /tmp/native native.rs
rustc --target wasm32-unknown-unknown --crate-type cdylib -C opt-level=3 -C codegen-units=1 -C panic=abort -o /tmp/sim.wasm wasm.rs
/tmp/native 200000 512 100
node run.mjs /tmp/sim.wasm 200000 512 100     # V8: wasm + JS
bun  run.mjs /tmp/sim.wasm 200000 512 100     # JavaScriptCore: wasm + JS
```

Caveats:

- Node 22's V8 (12.4) is older than current Chrome's V8.
- SpiderMonkey was not tested.
- The kernel is a simplified, memory-bound proxy, so real models will differ in absolute numbers but likely not in ranking.
- Run it on the target laptop to calibrate §5.3.
