# The model

This document describes what the simulation actually does. It is grounded in the literature review in
[research/economics.md](research/economics.md). Parameter defaults live in `crates/sim-core/src/config.rs`
(`EconParams`) and can be overridden in scenario JSON.

**Principle:** nothing below says "if tax X then outcome Y". Taxes change cash flows, and agents optimize over
cash flows with the same functions the tax system uses to levy them. City-scale outcomes are emergent and can
contradict the literature.

## World

* A grid of parcels (default 128 × 128, one cell ≈ 40 m). Procedural terrain: hills, a meandering river, and
  sometimes a coast. It also has soil fertility and initial vegetation (forest and meadow). Water is unbuildable;
  slope raises construction cost.
* Land uses: wild, farm, vacant, road, residential, commercial, industrial, park, civic, construction.
* The founding settlement is a river crossing with a few streets, homes, shops, workshops, a town hall and about
  400 households, all employed. Everything after that grows from agent decisions.
* Time step: one month. Indicators are recorded every tick.

## Agents

| agent | state | decides |
|---|---|---|
| **Household** | skill (log-normal), effort cost, cash, debt, home, lease rent, job, age, generation, net worth | work or not, which job, where to live, how much to consume, whether to stay, buying property |
| **Firm** | sector (retail / industry / public), parcel, owner, productivity, wage rate, workers, cash, emission intensity | hiring and firing, wages, abatement, exit |
| **Owner** (a household, an absentee investor or the government) | title, mortgage, Harberger markup | rents, maintenance, build / wait / keep, sell, self-assessed value |
| **Absentee capital** | deep pockets, required return = outside return + 1.5% | buys and develops land when the after-tax value beats the price; its income leaves the city |
| **Government** | cash or debt, programme budgets | taxes (per the active modules), spends by the user's mix, builds roads, parks and civic buildings |
| **Outside world** | utility level by skill, export price (AR(1) business cycle), interest | the open-city closure: migration in and out |

## The monthly pipeline (`sim.rs`)

1. **exogenous:** export-price cycle.
2. **fields:**
   * municipal boundary;
   * job accessibility (soft-min distance);
   * Huff retail demand;
   * pollution;
   * amenities (parks, water, nature);
   * service coverage;
   * the residential rent surface (hedonic level plus location premium);
   * business bid-rent surfaces;
   * occupancy and the construction pipeline.
3. **firm decisions:** hiring, firing and wages; **labour matching:** sampled job search.
4. **production:**
   * wages paid;
   * industry sells to the outside world;
   * shops capture last month's local spending via the Huff model;
   * firms pay rent, payroll tax, emission tax and corporate tax, then dividends.
5. **transfers:** citizen's dividend, means-tested welfare and surplus rebate.
6. **rents:** tenants pay or are evicted.
7. **owner costs:** property-type taxes, maintenance decisions and mortgages.
8. **settle:**
   * deposit interest;
   * income, wealth and head taxes;
   * consumption, with sales tax and an import share.
9. **housing market:** searching households choose homes; landlords adjust asking rents, and leases renew yearly.
10. **commercial rents:** renegotiation with tenant firms.
11. **assessment:** quarterly mass appraisal; **declare:** Harberger self-assessment; **land market:** forced
    sales, voluntary sales and Harberger buy-outs.
12. **development:** construction progress, then build / wait / keep decisions.
13. **government:** budget, roads, parks and civic buildings; **firm exit/entry**.
14. **emigration, immigration, demographics** (death and inheritance), **environment**, **net worth**,
    **price indices** (feeding expectations).

## Key decision rules

### Valuation (`econ.rs`), used by every buyer, owner and developer

Growing perpetuity of after-tax cash flow:

```
V = (12·NOI·(1 − t_inc) − T_fixed) / (r − g + h + w)
```

* **NOI:** expected rent × units × expected occupancy − maintenance.
* **T_fixed:** annual property-type taxes on the *assessed* land value (comparables, so unaffected by the owner's
  own choices) and on the building value, plus any vacancy tax.
* **h:** the Harberger rate (the declared value tracks own value); **w:** the owner's marginal wealth-tax rate;
  **t_inc:** the owner's marginal income tax on rental income.
* **r:**
  * absentee: outside return + 1.5%;
  * residents: deposit rate + 3.5%.
* **g:** adaptive expectation, long-run growth + `speculation` × recent rent and price trend, capped below r.

Consequences that follow from this formula rather than being imposed:
* A land value tax is a fixed charge, so it is neutral for height and maintenance. It still lowers land prices,
  because buyers deduct its present value and sales feed the comparables.
* A building tax scales with floors, so it lowers the optimal height (tested).
* A Harberger tax and a wealth tax raise the effective discount rate on the whole property.

### Development (`development.rs`)

For each serviced parcel under review, the owner finds the best (use, floors) option. The use is residential,
commercial or industrial; floors go up to 40, or 3 for industry, subject to the height limit and zoning:

* **cost:** `cost_per_floor × floors^(1+height_premium) × use factor × slope factor`;
* **expected occupancy:** local occupancy minus absorption of the new units *and* the units already under
  construction nearby, so developers see competing cranes;
* **business space:** priced at the firm's **bid-rent** for premises of that size. That is revenue per worker at
  that scale, minus labour cost and a margin. Shops share local spending (Huff); factories have decreasing
  average product.

The owner then **builds** only if the residual (value − cost − demolition) beats both keeping the current use and
**waiting a year**. Waiting means (value × (1+g) − cost)/(1+r) plus interim after-tax income. A land tax paid on
an idle lot lowers that interim income. Household owners need a 30% down payment, or they cannot build.

### Land market (`land.rs`)

* **Assessment** (quarterly, smoothed over about 2 years):
  * observed land residuals of built parcels (capitalized actual rents minus depreciated building value) and
    recent sale prices;
  * spatially smoothed;
  * floored at agricultural value;
  * with ± assessment noise.
* **Voluntary sales:** a sample of parcels is offered to the absentee investor and a few resident households
  with savings. A trade happens if the best bid exceeds the owner's own valuation × (1 + transfer tax). Bids
  between the two count as **blocked sales** (lock-in).
* **Forced sales:** households with negative cash sell their most valuable parcel at a discount. This is the
  liquidity channel for wealth tax, property tax, mortgages and estate tax.
* **Harberger:**
  * owners declare own value × a personal markup;
  * the markup decays when unchallenged and rises after a buy-out;
  * anyone valuing the parcel above the declared price plus transfer tax can take it.

### Households

* **Labour.** Unemployed people (and 4% of the employed) sample about 10 openings. They accept if the net gain
  beats their effort cost and current job. The net gain is the wage, minus the extra income tax, minus the lost
  means-tested welfare, minus commute cost. A job seeker who would accept without taxes but not with them counts
  as a **deterred worker**.
* **Housing.**
  * Who searches: the unhoused, the rent-burdened (> 55%) and a random 3%/month.
  * Each samples about 10 vacancies they can afford (rent ≤ 50% of budget).
  * They pick by utility (amenity + services − pollution − commute − rent − disrepair) plus Gumbel noise, and
    move if it beats staying.
* **Rents.** Landlords cut asking rents when units stand empty. Full buildings raise rents only when
  households queue (unhoused people or would-be immigrants), plus inflation drift. Leases renew yearly.
* **Consumption.** A buffer-stock rule out of disposable income and cash. Part is spent at local shops; the rest
  is imported.
* **Migration** (open city):
  * Would-be immigrants come if the expected after-tax utility for their skill beats the outside option plus
    moving cost, and housing is available. Otherwise they become **latent demand**, which landlords and
    developers see.
  * Residents leave after about 8 months of being worse off than outside. Capital income counts as portable,
    taxed at outside rates. Wealth leaving with emigrants is **capital flight**.
  * The unhoused give up after a few months.
* **Death and inheritance.** The heir continues the household with the estate, net of estate tax, and a skill
  that regresses to the mean.

### Firms

* **Industry** sells unlimited quantities at the export price (Y = A·S^0.85 with agglomeration). It hires while
  the marginal product exceeds the wage, and chooses emission intensity to minimize pollution tax plus abatement
  cost.
* **Shops** capture local spending via the Huff gravity model with an outside "shop elsewhere" option.
* Wages rise when vacancies stay unfilled *and* the extra worker pays. They fall with losses or high
  unemployment.
* Firms exit after sustained losses. New firms enter empty premises when expected after-tax profit is positive.
  They are founded by resident savers or by outside capital.

### Government

* Revenue is split by the user's spending mix, plus a slow draw-down of reserves.
* **Services:** staff civic buildings and build new ones where coverage is lacking. Coverage adds value to
  nearby locations.
* **Roads:** extend the block grid where it unlocks the most rent potential, but only while serviced idle land
  is scarce relative to construction activity.
* **Parks:** buy land at market price where many residents lack park access.
* **Dividend:** equal per household. **Welfare:** a top-up with 50% withdrawal, extra for the unhoused.
* **Surplus rebate:** reserves above 12 months of revenue that no programme can absorb are returned equally.
  Otherwise high-tax regimes would simply destroy money.
* A deficit is allowed up to a debt limit, after which spending is cut.

### Environment

* Vegetation regrows towards its natural level on wild land, is damaged by pollution and is cleared by
  construction.
* Idle lots go to weeds; low-rise homes keep gardens.
* Pollution is industrial emissions, smoothed spatially. It reduces residents' utility, which they price into
  location choice.

## Taxes: base, payer, and where behaviour responds

| tax | base | paid by | decisions it enters |
|---|---|---|---|
| Land value | assessed land (comparables) | owner | valuation (fixed charge), wait vs. build (interim income), forced sales |
| Property / split-rate | assessed land + building | owner | as above, plus height choice and maintenance (building part) |
| Income | annual labour (+ capital) income, brackets | household | work vs. not, job choice, migration, investor's after-tax rental yield |
| Wealth | net worth above a threshold | household | investor discount rate, forced sales, emigration of the wealthy |
| Sales / VAT | consumption | household | real consumption, shop revenue, firm bid-rents |
| Corporate | positive profit | firm | entry, business bid-rent |
| Payroll | wages | firm | hiring margin, bid-rent |
| Transfer | sale price | buyer | trade threshold (lock-in) |
| Harberger (COST) | self-declared value | owner | declaration, buy-outs, valuation (like a tax on total value) |
| Pollution | emissions | firm | abatement choice, industry bid-rent, entry |
| Vacancy | idle share of property value | owner | valuation of empty space |
| Estate | estate above a threshold | heir | forced sales, dynastic wealth |
| Head | per household | household | migration (pure income effect) |

## Indicators (`metrics.rs`)

* **Population:** households, unhoused, unemployment.
* **Income and housing:** median income, mean wage, median rent, rent burden, housing vacancy.
* **Inequality:** Gini of income and wealth, top 10% and 1% wealth shares, poverty rate, absentee share of land
  value.
* **Land and buildings:** land price index, average and maximum floors, urban footprint, idle lots inside the
  city, speculative holds, derelict buildings.
* **Transactions and construction:** sales, forced sales, Harberger buy-outs, construction starts and completions.
* **Environment and access:** green cover, forest cells, population-weighted pollution exposure, emissions,
  average commute, parks.
* **Government:** revenue by instrument, spending by programme, reserves.
* **Distortion counterfactuals:** deterred workers, blocked sales, floors built vs. floors without taxes, deferred
  maintenance, wealthy emigrants and capital flight.
* **Henry George test:** annual differential land rent vs. spending on public goods.
* **Mean welfare:** residents' money-metric utility.

## What the model currently says, and how much to trust it

These are 10 seeds × 40 years, from `taxsim compare` (mean ± standard error; see the README). Differences
smaller than a couple of standard errors are noise.

* **Georgist** (6% land value tax, dividend):
  * land prices −55–60% versus the status quo, rent burden ~22% vs ~33%;
  * a more compact city with shorter commutes and slightly taller buildings;
  * more land owned by residents and much higher average resident welfare;
  * but higher wealth inequality (no income or wealth taxes), and population within noise of the status quo.
* **Wealth tax** (2% above 1M): lower wealth inequality and the most departures of wealthy households. City size
  is within noise.
* **Nordic** (high income tax and VAT): the lowest wealth inequality and the most deterred workers.
* **Harberger at 4% of total value:**
  * development nearly stops;
  * the self-assessed tax falls on buildings too, so waiting beats building until rents rise far more;
  * a mechanism-level warning about COST rates, not a calibrated forecast.
* **Head tax:** a small city, because low earners leave.

**Known limitations and sensitivities.** Read the results as "what these mechanisms produce", not as forecasts.
* **Ownership granularity.** Ownership is per parcel (a whole building). Most households can never afford one,
  so outside capital owns most rental property under most regimes. Unit-level ownership (condos,
  owner-occupation) is the most important missing piece for wealth dynamics.
* **Public goods matter a lot.** Service coverage adds up to 380/month of location value, and roads gate
  development. Low-revenue regimes therefore grow less. This is the Henry George mechanism, but its strength is
  a parameter (`fields.rs`: `SERVICE_VALUE`, `PARK_VALUE`, `POLLUTION_COST`).
* **Stochastic and path-dependent.** Housing cycles emerge from construction lags, and terrain matters. Use many
  seeds.
* **Not modelled:** monetary policy, credit cycles beyond simple mortgages, international trade beyond a single
  export good, traffic congestion (commutes are Manhattan distance), multiple competing municipalities, and
  political feedback.
* **Calibration** targets the stylized facts in the research notes (vacancy 3–10%, rent burden 25–35%,
  income Gini 0.2–0.5) loosely, by hand. A systematic calibration with `taxsim` sweeps is future work.

## Scope: done, next, out

**Done (this version):**
* all tax instruments above, with a revenue-recycling mix;
* an open city with migration, housing, labour and land markets, development with real options, maintenance and
  decay, Harberger buy-outs, inheritance, pollution and nature;
* counterfactual distortion metrics and mechanism tests;
* the 3D browser front end with overlays, inspector and baseline comparison, plus the native batch runner.

**Next:**
* unit-level ownership and owner-occupation (rent vs. buy);
* congestion-aware commuting and transit;
* land value increment tax and land value capture of public investment;
* revenue-neutral comparisons (automatically scale a regime to match a target revenue);
* parameter sweeps and sensitivity reports in the CLI;
* scenario sharing via URL;
* WebGPU rendering;
* faster fields (SIMD or incremental).

**Out of scope, deliberately:**
* a full monetary and banking system with a central bank;
* international trade and exchange rates;
* fully rational, forward-looking optimizing agents;
* hard-coded elasticities from the literature as behaviour.

These either add parameters faster than insight or would script the outcomes the model is meant to discover.
