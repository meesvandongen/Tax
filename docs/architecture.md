# Architecture

## Decisions in one paragraph

The simulation is a **pure Rust crate** (`sim-core`) with no platform dependencies. The same code is compiled
to **WebAssembly** for the browser and to a **native binary** (`taxsim`) for headless batch experiments. That
avoids choosing between "browser" and "native binary": both are first-class. In the browser the wasm module runs
**single-threaded inside a Web Worker**. It streams compact snapshots to the main thread, where **Three.js**
renders the city and a small TypeScript UI edits policy and plots indicators. No `SharedArrayBuffer` or
cross-origin-isolation headers are needed, so `web/dist` deploys to any static host. The full reasoning and
benchmarks are in [research/technology.md](research/technology.md). In short:

* **Rust → wasm vs. JS.** Rust compiled to wasm ran a representative agent kernel at about 1.2–1.4× native speed.
  Hand-tuned typed-array JavaScript took about 2–2.7×. Rust also gives one code path for browser and batch runs.
* **Why not threads in the browser.** wasm threads in Rust still need nightly toolchains and COOP/COEP headers.
  Parallelism goes where it pays off instead: running many independent simulations at once
  (`taxsim compare` uses every core).
* **Why WebGL (Three.js `WebGLRenderer`) today.** The scene is a few instanced meshes (≤ ~100k instances),
  well within WebGL2 on every browser. The renderer is isolated in `web/src/render/`. Switching to
  `WebGPURenderer` later is a local change.
* **No wasm-bindgen.** The wasm boundary is a dozen `extern "C"` functions plus JSON for low-frequency data
  (config, policy, metrics, inspector), with raw typed-array views for per-frame buffers. That means no generated
  glue to keep in sync and no tool-version pinning.

## Data flow

```
 ┌──────────── main thread ─────────────┐        ┌──────────── Web Worker ────────────┐
 │ PolicyPanel ──setPolicy(JSON)──────────┼──────▶│ sim_set_policy                       │
 │ controls ──run/pause/step───────────── ┼──────▶│ loop: sim_step(n) at chosen speed    │
 │                                        │       │       sim_history_json(from) ──┐     │
 │ Charts/KPIs ◀── metrics[] (JSON) ──────┼───────┼────────────────────────────────┘     │
 │ WorldView   ◀── frame: cells u8×8/cell ┼───────┼── sim_snapshot → typed-array copies  │
 │                 overlays f32×11/cell   │       │   (transferred, ~10×/s)              │
 │                 commuter pairs u32     │       │                                      │
 │ Inspector   ◀── sim_inspect_json(cell) ┼───────┼──                                    │
 └────────────────────────────────────────┘       └──────────────────────────────────────┘
        (optional second worker: the baseline run, same seed, metrics only)
```

## The simulation core (`crates/sim-core`)

| module | role |
|---|---|
| `world.rs` | All state: terrain, parcels (struct-of-arrays), households, firms, government, flows, history |
| `types.rs` | `Parcels` (SoA, one entry per grid cell), `Household`, `Firm`, `LandUse`, `Owner` |
| `sim.rs` | **The monthly pipeline**: an ordered list of `(name, fn(&mut World))` systems; optional profiling |
| `fields.rs` | Spatial fields recomputed every tick: job access, Huff retail demand, pollution, amenity, services, rent surfaces |
| `grid.rs` | O(n) separable exponential-kernel blur (`a^(\|dx\|+\|dy\|)`), which turns spatial interaction into convolutions |
| `econ.rs` | Valuation and development feasibility used by *every* agent (perpetuity with taxes, best (use, height) option, firm bid-rents) |
| `labour.rs` | Firm hiring/wage rules, job search with after-tax participation choice |
| `production.rs` | Output, wages, rents, firm taxes, dividends; Huff-model allocation of local spending to shops |
| `housing.rs` | Rent payment and eviction, landlord rent adjustment, household location choice (sampled logit) |
| `property.rs` | Owners' property-type taxes, maintenance decisions, mortgage service |
| `land.rs` | Mass appraisal by comparables, forced and voluntary sales via bids, Harberger declarations and buy-outs, compulsory purchase |
| `development.rs` | Build / wait / keep decisions (real-option rule), construction pipeline |
| `firms.rs` | Firm entry and exit |
| `government.rs` | Transfers, budget allocation, roads, parks, civic buildings |
| `migration.rs` | Open-city immigration and emigration, ageing, death and inheritance |
| `environment.rs` | Vegetation regrowth and damage, weeds on idle lots |
| `households.rs` | Month-end settlement: income, wealth and head taxes, consumption with sales tax, net worth |
| `tax/` | **One file per tax instrument**, implementing `TaxModule`; `TaxSystem` aggregates them and keeps a revenue ledger |
| `presets.rs` | Named policies |
| `metrics.rs` | City-wide indicators, computed each tick |
| `snapshot.rs` / `inspect.rs` | Render buffers and the parcel inspector |

### Design rules

1. **Taxes only act through agents' own calculations.** A `TaxModule` is a set of pure functions from tax bases
   to amounts (`income`, `wealth`, `property`, `transfer`, `emission`, …). The simulation levies taxes with
   `TaxSystem::levy` and agents *anticipate* them with `TaxSystem::estimate`, using the same functions. No system
   checks "which tax is active" to change behaviour.
2. **Prices come from bids**, not formulas. Land is valued by each bidder with its own discount rate and tax
   position. Assessments are made from comparables (observed residual land rents and sales).
3. **Deterministic.** A single seeded xoshiro256++ stream; no hash-map iteration in the tick; same seed and policy
   give identical histories (tested). Results can differ in the last float bits between native and wasm because
   `libm` differs, so compare runs within one platform.
4. **Plain data, explicit order.** Systems are free functions over `&mut World`. The pipeline in `sim.rs` is the
   single place that defines what happens in a month.

## Extending

**Add a tax.**

1. Create `crates/sim-core/src/tax/my_tax.rs` implementing `TaxModule`. Override only the bases it uses.
2. Add a variant to `TaxSpec` and a line in `TaxSpec::build`, and add `kind()`.
3. Add the variant to `web/src/sim/protocol.ts` and an entry with slider definitions to `TAX_META` in
   `web/src/ui/policy.ts`.

If the tax needs a *new base* (e.g. a land-value-increment tax), add a method to `TaxModule` with a default of 0.
Compute the base where the relevant decision happens, and use `estimate` there so agents anticipate it.

**Add a system.** Write `fn my_system(w: &mut World)` and insert it into `PIPELINE` in `sim.rs` at the right point
in the month.

**Add an indicator.** Add a field to `Metrics` and compute it in `metrics::compute`. The CLI's CSV, the charts
(`web/src/ui/charts.ts`) and KPIs (`web/src/ui/kpis.ts`) pick fields by name.

**Add a map overlay.** Push a channel in `snapshot.rs` (`OVERLAYS` and the fill loop) and add a matching
`OVERLAY_DEFS` entry in `web/src/render/palette.ts`. They must stay in the same order.

## Performance

Measured on the 4-core cloud container used for development (128² map unless noted):

| scenario | native | wasm (Node/V8) |
|---|---|---|
| founding town, ~500 households | ~6 ms / month | ~12 ms / month |
| year 50, ~4,000 households | ~6 ms / month | ~13 ms / month |
| 192² map, year 50, ~3,000–5,000 households | ~14 ms / month | ~30 ms / month |

The web UI runs the simulation at up to 60 months per second of requested speed. In practice it runs at whatever
the worker sustains. Rendering is decoupled and stays smooth because the worker sends at most ~10 frames per second.

Where the time goes (`taxsim run --profile`): about 75% in `fields`, which is ~25 separable blurs plus the
business bid-rent surface. These are the obvious optimization targets: SIMD blurs, updating slow fields every
few ticks, or moving them to the GPU for display-only use.

## Front end (`web/`)

| file | role |
|---|---|
| `src/sim/worker.ts` | Loads `sim.wasm`, runs the loop, posts metrics and frames |
| `src/sim/client.ts` | Main-thread handle (main city or baseline "shadow" city) |
| `src/sim/protocol.ts` | Shared constants and message types (mirrors the Rust layouts) |
| `src/render/world.ts` | Terrain mesh + per-cell ground texture, instanced buildings, roofs, trees, weeds, fences, tents, cranes, bridges, smog layer, commuter traffic, migration movers, event pulses, picking |
| `src/render/palette.ts` | Land-use colours, colour ramps, overlay definitions |
| `src/ui/policy.ts` | Preset loader, tax cards with sliders, income brackets, spending mix, zoning and height rules |
| `src/ui/charts.ts` | uPlot time series (dashed lines = baseline run) |
| `src/ui/kpis.ts`, `src/ui/inspector.ts` | KPI tiles with deltas vs. baseline; parcel inspector |
