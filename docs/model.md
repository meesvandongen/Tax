# The model

This document describes what the simulation actually does. It is grounded in the literature review in
[research/economics.md](research/economics.md). Parameter defaults live in `crates/sim-core/src/config.rs`
(`EconParams`) and can be overridden in scenario JSON.

**Principle:** nothing below says "if tax X then outcome Y". Taxes change cash flows, and agents optimize over
cash flows with the same functions the tax system uses to levy them. City-scale outcomes are emergent and can
contradict the literature.

## World

* An irregular grid of quadrilateral parcels, built the way Townscaper builds its grid (`mesh.rs`): a
  triangular lattice over the map, random pairs of neighbouring triangles merged into quads, every polygon split
  into quads, and the corners relaxed until each quad is as square as its neighbours allow. A 128 × 128 map has
  about 16 600 parcels of about one cell (≈ 40 m) each. Every parcel has at most four edge neighbours, but rows
  of parcels meander and meet at three- to six-way corners, so streets that follow them curve.
* Procedural terrain: hills, a meandering river, and sometimes a coast, as continuous functions sampled at
  parcel centres and corners. It also has soil fertility and initial vegetation (forest and meadow). Water is
  unbuildable; slope raises construction cost.
* Spatial interactions (job access, shopping, pollution, amenities) are exponential-decay convolutions on a
  square raster under the mesh (one raster cell per map cell): parcel values are splatted onto the raster,
  blurred and sampled back at each parcel's centroid. Commutes are straight-line distance times 1.27, the mean
  detour of a street network (4/π for a grid).
* Land uses: wild, farm, vacant, road, residential, commercial, industrial, park, civic, construction.
* Roads are parcels. A street serves the parcels up to two steps away across parcel edges (the lots fronting it
  and the lots behind them). No land is reserved for future streets: owners may build anywhere their parcel has
  access, and the government buys the land when it lays a road (see Government).
* The founding settlement is a river crossing: main roads out of the centre along the cheapest routes over the
  mesh (bridging the river), a ring street, homes, shops, workshops, a town hall and about 400 households,
  nearly all employed. Everything after that grows from agent decisions.
* Time step: one month. Indicators are recorded every tick.
* **Real money.** Money amounts mean the same at year 300 as at year 0, and interest rates are real (outside
  capital 4.5%, deposits 1.5%, mortgages 4.5%). The optional `outside_growth` (0 by default) scales every
  money-denominated parameter together: wages, construction and demolition costs, farm rents, amenity and
  service values, commute costs, road costs, thresholds, and the money amounts in tax schedules (income
  brackets, wealth and estate exemptions, head and pollution taxes). Since everything scales at once, it
  changes only the units, like inflation, not living standards.

## Agents

| agent | state | decides |
|---|---|---|
| **Household** | skill (log-normal), effort cost, cash, debt, home, lease rent, owned flat and its mortgage, job, age, generation, net worth | work or not, which job, where to live, rent or buy the flat, how much to consume, whether to stay, buying property |
| **Firm** | sector (retail / industry / public), parcel, owner, productivity, wage rate, workers, cash, emission intensity | hiring and firing, wages, abatement, exit |
| **Owner** (a household, an absentee investor or the government) | title to the parcel (the freehold: land, common parts and every dwelling not sold as a flat), mortgage, Harberger markup | rents, maintenance, build / wait / keep, sell, self-assessed value, selling flats to their tenants |
| **Absentee capital** | deep pockets, required return = outside return + 1.5% | buys and develops land when the after-tax value beats the price; its income leaves the city; stops paying tax on parcels worth nothing |
| **Government** | cash or debt, programme budgets | taxes (per the active modules), spends by the user's mix, builds roads, parks and civic buildings |
| **Outside world** | utility level by skill, export price (AR(1) business cycle × downward-sloping demand), interest | the open-city closure: migration in and out |

## The monthly pipeline (`sim.rs`)

1. **exogenous:** export-price cycle, and the export price level given last month's export volume.
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
9. **housing market:** searching households choose homes; **tenure:** tenants decide whether to buy the flat they
   rent; landlords adjust asking rents, and leases renew yearly.
10. **commercial rents:** renegotiation with tenant firms.
11. **assessment:** quarterly mass appraisal; **declare:** Harberger self-assessment; **land market:** forced
    sales, voluntary sales and Harberger buy-outs.
12. **development:** construction progress, then build / renovate / wait / keep decisions.
13. **government:** budget, roads, parks and civic buildings; **firm exit/entry**.
14. **emigration, immigration, demographics** (death and inheritance), **environment**, **net worth**,
    **price indices** (feeding expectations).

## Key decision rules

### Valuation (`econ.rs`), used by every buyer, owner and developer

Growing perpetuity of after-tax cash flow:

```
V = (12·NOI·(1 − t_inc) − T_fixed) / (r − g + h + w)
```

* **NOI:** expected rent × units × expected occupancy − maintenance. Business space is priced at the firm
  bid-rent, but never much above the rents tenants nearby actually pay.
* **T_fixed:** annual property-type taxes on the *assessed* land value (comparables, so unaffected by the owner's
  own choices) and on the building value, plus any vacancy tax.
* **h:** the Harberger rate (the declared value tracks own value); **w:** the owner's marginal wealth-tax rate;
  **t_inc:** the owner's marginal income tax on rental income.
* **r:**
  * absentee: outside return + 1.5%;
  * residents: deposit rate + 3.5%.
* **g:** adaptive expectation: long-run growth + `speculation` (0.1) × the recent *real rent* trend, bounded to
  −2 / +1.5 points around long-run growth and below r. Land prices are deliberately not extrapolated: they are
  themselves valuations built on g, and feeding them back produced self-fulfilling boom-bust cycles in land
  values (100× within a few years).

Consequences that follow from this formula rather than being imposed:
* A land value tax is a fixed charge, so it is neutral for height and maintenance. It still lowers land prices,
  because buyers deduct its present value and sales feed the comparables.
* A building tax scales with floors, so it lowers the optimal height (tested).
* A Harberger tax and a wealth tax raise the effective discount rate on the whole property.

### Development (`development.rs`)

For each serviced parcel under review, the owner finds the best (use, floors) option. The use is residential,
commercial or industrial; floors go up to 40, or 3 for industry, subject to the height limit and zoning:

* **cost:** `cost_per_floor × floors^(1+height_premium) × use factor × slope factor`;
* **expected occupancy:** local occupancy once let, minus the effect of competing projects under construction
  nearby (developers see the cranes);
* **lease-up:** a new building competes in the whole city's market, not only its block. Its new space fills at a
  pace that grows with the neighbourhood and the city (4 + 3% of local units + 0.05% of households per month), and
  the rent lost meanwhile is a one-off cost. An earlier version treated each new dwelling as permanently lowering
  occupancy against the stock within about 120 m, so every tall building looked a third empty forever and
  mature cities stopped building;
* **business space:** priced at the firm's **bid-rent** for premises of that size. That is revenue per worker at
  that scale, minus labour cost and a margin. Shops share local spending (Huff); factories have decreasing
  average product. Factories are priced *through the cycle*: at the export price expected over a ten-year
  occupancy, not today's boom or slump price;
* **clearing the site:** demolition, plus compensation for sitting tenants: a relocation allowance of
  `relocation_months` (6) of rent per household, and twice that per leased job slot as a lease buy-out for a firm.
  Owner-occupiers must be bought out of their flats (a collective sale). The developer offers the lowest price at
  which the policy's share of the building's dwellings accepts (`collective_sale_threshold`, 80% by default; the
  parcel owner's unsold dwellings count as accepting), and at least market value plus 20%. An owner accepts once
  the offer beats what living there is worth to them plus the cost of moving; holdouts are paid the same price.
  The project goes ahead only if it still pays at that price, so a building of owned flats is harder to
  redevelop than a rented one.
* **owned flats:** the parcel owner values only its own share of the building (the dwellings not sold as flats).
  Flat owners pay their share of the building's property taxes, upkeep and renovations; the parcel owner still
  decides on maintenance.

An existing building is valued as it stands. Its rent falls steeply as it nears ruin (the square root of its
condition: a building at a quarter of new condition lets for half the rent), and tenants' disrepair cost is the
same share of the local rent. Before, a ruin kept 60% of its rent, neglect was rational, and about 30% of
buildings were derelict after 150 years. Upkeep rises with the building's age (it doubles every 60 years), so
old buildings are eventually worth replacing. Let business premises are worth at least the lease the sitting
tenant pays (scaled by condition), as an appraiser would value them, not only what a newcomer would bid in
today's labour market. Without that, a fully let factory looked worthless whenever labour was tight and was
torn down.

* **expected occupancy for business space:** the lower of what the labour market can staff, local business
  occupancy and city-wide business occupancy, minus absorption, so empty premises deter new ones.

The owner then **builds** only if the residual (value − cost − demolition) beats both keeping the current use and
**waiting a year**. Waiting means (value × (1+g) − cost)/(1+r) plus interim after-tax income. A land tax paid on
an idle lot lowers that interim income. Household owners need a 30% down payment; if they cannot pay it, they sell
to outside capital when its bid beats keeping the parcel.

A run-down building can instead be **renovated** to new condition for half its construction cost times the
missing condition, which also halves its effective age, when that adds more value than rebuilding. A sound
building of up to 8 floors can instead **add 1 or 2 floors** (at a 25% cost premium over building them new,
renovating the rest at the same time): no demolition, and the residents stay. Renovations and extensions are
financed like construction (a 30% deposit); an owner who cannot finance one sells to outside capital, whose
valuation includes the option. **Maintenance** is decided each month the same way:
the owner pays the upkeep if their own after-tax valuation of the building in better condition exceeds it. This
includes the point (condition 0.12) below which nobody new will rent a dwelling.

### Land market (`land.rs`)

* **Assessment** (quarterly, smoothed symmetrically over about a year):
  * land is valued **as if vacant**: what a typical investor would pay for the site given its best permitted
    project, local rents and occupancy and the tax schedule. Nothing the owner has built enters, so a land tax
    stays a fixed charge;
  * land without street access can only be farmed, so it is assessed as farmland;
  * recent sale prices of unbuilt land are blended in. A built parcel sells with its building and leases (a sitting
    tenant may pay far above market rent), so its price minus replacement cost is not a land comparable;
  * values are floored at agricultural value;
  * with ± assessment noise;
  * the smoothing is symmetric, as in mass appraisal over several years of evidence. The best-project residual is
    a thin margin of property value over construction cost: a nearby project starting, or a quarter's dip in
    expected occupancy, can wipe it out and it returns a quarter later. Passing every dip straight through (an
    instant appeal) made individual parcels' land values collapse and recover within a year or two;
  * the value is **self-consistent with the land tax**: buyers deduct the tax on the assessed land, and the
    assessment is the land value, so the assessor solves `L = V(L)` (property taxes are linear in `L`, so
    `L = V(0) / (1 + c)`). Valuing at last quarter's assessment instead made high assessments produce low values
    and vice versa, and under a 6% land tax assessments oscillated every quarter.
* **Voluntary sales:** a sample of parcels is offered to the absentee investor and a few resident households
  with savings. A trade happens if the best bid exceeds the owner's own valuation × (1 + transfer tax). Bids
  between the two count as **blocked sales** (lock-in).
* **Forced sales:** households with negative cash sell their most valuable parcel at a discount. This is the
  liquidity channel for wealth tax, property tax, mortgages and estate tax.
* **Tax default and foreclosure:** outside capital has no other stake in the city. When a parcel is worth nothing
  to its absentee owner net of future taxes, the owner stops paying and offers it for sale. If nobody buys within
  a year, the city forecloses and auctions the parcel to the highest bidder. Taxes are therefore never paid out
  of nowhere on land that cannot earn them.
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
  * Who searches: the unhoused, those in dwellings too run-down to let, the rent-burdened (> 55%) and a random
    3%/month.
  * Each samples about 10 vacancies they can afford (rent ≤ 50% of budget).
  * They pick by utility (amenity + services − pollution − commute − rent − disrepair) plus Gumbel noise, and
    move if it beats staying.
* **Rents.** Landlords adjust asking rents to vacancy (their building's and the neighbourhood's) against a natural
  rate of 5%: rents rise in tight markets, faster when households queue (unhoused people, would-be immigrants,
  households in unlettable dwellings), and fall, more slowly, in slack ones. Leases renew yearly. Vacancy moves rents
  only gently (`housing_rent_adjust`, at most about 6% a year beyond trend growth). Tenants feel a rent only at
  renewal and leave after months of discontent, so landlords who chased vacancy faster overshot. That made a
  ten-year citywide rent cycle of ±15%, which the leveraged land residual turned into tenfold land value swings.
  A full building with people queueing for homes is a shortage landlords can see directly, so there rents rise
  fast (`housing_queue_response`). That is what lets a young, housing-short city grow.
* **Owning a flat.** A household that cannot afford a whole parcel can still own its home: a tenant may buy the
  flat it rents, once a year at lease renewal:
  * the parcel owner sells at its own valuation of a dwelling (its building value / units) plus 3%;
  * the household values owning at the rent it saves, never vacant and untaxed (imputed rent is not income),
    minus its share of upkeep and property taxes, at its own discount rate plus 1% (its home is one
    undiversified, illiquid asset) and its wealth-tax rate;
  * it buys if that beats the price plus transfer tax, it has the 30% deposit, transfer tax and three months'
    subsistence in cash, and the mortgage payment is at most 35% of its budget.

  Owners pay no rent, search for a new home a third as often as tenants, and sell the flat back to the parcel
  owner at market value when they move, emigrate or run out of cash (then they rent it on). The flat counts in
  net worth (its share of the assessed parcel value, less the mortgage) and so in wealth and estate taxes, and
  heirs inherit it.
* **Consumption.** A buffer-stock rule out of disposable income (after the rent actually paid; owner-occupiers
  pay none) and net worth (property and businesses too, not only cash, so a rentier family spends out of its
  fortune), twice as fast after 65. Part is spent at local shops; the rest is imported.
* **Migration** (open city):
  * Prospective movers arrive at 0.6% of the population a month, plus a quarter of the advertised vacancies.
  * They come if the expected after-tax utility for their skill (net of effort, typical commute, rent, dividend)
    beats the outside option plus moving cost, and they can afford a typical vacant home. If no home is free they
    become **latent demand**, which landlords and developers see.
  * Residents leave after about 8 months of being worse off than outside. Capital income counts as portable,
    taxed at outside rates. Emigrants sell their property and businesses to outside capital; the wealth leaving
    with them is **capital flight**.
  * **Congestion:** commuting costs scale with `1 + (households / 20 000)^0.6`, so a larger city is a slower one.
  * The unhoused give up after a few months.
* **Death and inheritance.** The estate, net of estate tax, is split among about two children (1 to 4). One
  continues the household (home, property and businesses) with a skill that regresses to the mean, and buys the
  others out of everything but the businesses, selling property if the cash does not cover it. Half the other
  children grew up here and consider staying: they set up a household if the city looks better to them than
  moving away, as for a newcomer; otherwise their share leaves the city. Before, one heir took everything, and
  in regimes without income, wealth or estate taxes the largest fortunes kept compounding for centuries.

### Firms

* **Industry** sells to the outside world (Y = A·S^0.85 with agglomeration). It hires while the marginal product
  exceeds the wage, and chooses emission intensity to minimize pollution tax plus abatement cost. The outside
  market is not unlimited: once the city's exports exceed `export_market_size`, the export price falls with
  elasticity 3. Without this (and congestion), agglomeration would make the city grow until the map is full.
  But each export firm sells its own variety, so the market grows with the number of export firms, as
  `(firms / 300)^0.25` (`export_variety`): a bigger city sells more kinds of things, with diminishing returns.
* **Offices** (tradable services: finance, software, consulting) also sell to the outside world, with their own
  market (`office_market_size`, the same variety rule and price elasticity). They locate in commercial
  buildings and gain much more from clustering than factories: productivity rises with all the jobs within a
  short walk (`office_agglomeration` on the log of jobs within ~2.5 parcels), so they concentrate downtown.
  Their returns to scale fall slowly (S^0.93), so one firm can fill a tall building, and new office space is
  not limited by the size of a typical firm. Commercial premises go to a shop or an office, whichever bids
  more. They are not tied to the goods cycle.
* **New business space** can only expect the workers the city can supply (job seekers, would-be newcomers and
  2% of residents), so a small town does not build offices for thousands.
* **Shops** capture local spending via the Huff gravity model with an outside "shop elsewhere" option. A worker
  can serve at most `retail_output_per_worker` of sales a month; the rest is bought elsewhere. A bigger city
  makes more of what it buys (import substitution): the local share of spending rises from 55% toward 70%,
  halfway there at 20 000 households (`local_spend_max`, `local_spend_scale`).
* **Business rents:** landlords raise a tenant's rent while it earns a margin above 15% and cut it when the
  margin falls below 3%; empty premises are offered at falling rents. A tenant does not pay more than 1.3× what
  the premises would cost a typical firm (it would move), so a landlord captures the location's rent, not one
  tenant's productivity.
* Wages rise when vacancies stay unfilled *and* the extra worker pays. They fall with losses or high
  unemployment.
* Firms exit after about a year of losses. New firms enter empty premises when expected after-tax profit is
  positive. They are founded by resident savers or by outside capital.

### Government

* Revenue is split by the user's spending mix, plus a slow draw-down of reserves. Reserves earn the deposit rate.
* **Services:** staff civic buildings, and build a new one when the budget could staff more than the existing
  and planned buildings hold, residents need more staff, and it can be paid for within the debt limit. Coverage
  adds value to nearby locations.
* **Roads** (`roads.rs`) are planned each quarter as projects: chains of up to 18 parcels branching off the
  network.
  * **When:** only while serviced land is short (fewer parcels with street access that a developer would build
    on at a profit than about two years of construction starts), and only projects whose **benefit/cost ratio**
    is at least 1. The benefit is the land value the road creates: the newly served parcels' value as building
    land (untaxed, as if vacant) minus their value as farmland. By the Henry George theorem this is also what a
    land value tax can recover. Projects are funded best ratio first, without overlapping each other's served
    land, until they cover the shortfall or the road fund runs out. If the best project is not yet affordable
    but the fund will reach it within a year, the planner saves up for it instead of building a worse one.
  * **What:** routes are least-cost paths over the parcel mesh. Each parcel costs its purchase, demolition and
    tenant compensation if built on, and construction (more on slopes, ten times more for a bridge); turning
    off a row of parcels costs extra, so streets follow the mesh's flowing rows. A new street touches the
    network only where it branches off (no parallel streets, no paved blocks). One search from the whole
    network prices a route to every parcel. Routes are screened by the land value they would unlock, estimated
    from what serviced land nearby is assessed at, per unit of cost; the 24 most promising are priced exactly
    and the best are built.
  * **Buying the land:** nothing is reserved in advance. The government buys every parcel on the route, before
    laying any of it (so owners are paid the value without the scheme, as compulsory-purchase law prescribes),
    at the owner's valuation or the assessment if higher, **plus a premium** (`purchase_premium`, 25% by
    default). A building in the way is bought the same way; the government then compensates its tenants and
    demolishes it. That makes buildings far more expensive than open land, so routes bend around them, but a
    street can still break out through a built-up edge when the land beyond is worth it.
* **Parks:** buy land at market price plus the purchase premium where many residents lack park access.
* Road and park funds accumulate; what is left after the month's works is capped at three years of allocations.
* **Dividend:** equal per household. **Welfare:** a top-up with 50% withdrawal, extra for the unhoused, at the
  highest level the welfare budget can pay for.
* **Surplus rebate:** reserves above 12 months of revenue that no programme can absorb are returned equally
  (a twelfth per month). Otherwise high-tax regimes would simply destroy money.
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
* **Transactions and construction:** sales, forced sales, tax foreclosures, Harberger buy-outs, construction
  starts and completions, renovations.
* **Environment and access:** green cover, forest cells, population-weighted pollution exposure, emissions,
  average commute, parks.
* **Government:** revenue by instrument, spending by programme, reserves.
* **Distortion counterfactuals:** deterred workers, blocked sales, floors built vs. floors without taxes, deferred
  maintenance, wealthy emigrants and capital flight.
* **Henry George test:** annual differential land rent vs. spending on public goods.
* **Mean welfare:** residents' money-metric utility.

## Plausibility audit (`audit.rs`)

`taxsim audit` and the test suite check, every simulated year, that the city stays economically coherent:

* tax revenue stays well below output, and property taxes stay below the rent the buildings actually earn;
* business space is not mostly empty, there are not far more job slots than households, and there are enough
  dwellings;
* unemployment, homelessness, vacancy, rent burden, wages and rents stay within broad bounds;
* the government neither hoards decades of revenue nor runs up runaway debt, transfers do not exceed residents'
  market income, and welfare never pays out more than its budget;
* no household holds absurd cash, and land prices stay finite and do not swing more than threefold within five
  years (after the founding period);
* bookkeeping: no parcel or firm belongs to a household that has left, every firm's head count and skill total
  match the households it employs, and every road parcel is connected to the network.

The citywide land price index can stay calm while individual parcels boom and bust. So the test suite also
follows every taxable parcel's assessment for 50 years in a mature Georgist city: 90% of valuable parcels must
not rise or fall more than threefold and back within any five years. Before the slower residential rent
adjustment and the symmetric assessment smoothing, the 90th percentile parcel swung about 14× and back on the
full-size map, and parcels worth millions were reassessed near zero for a year.

An earlier version failed these badly on long runs. Outside owners paid any tax out of nowhere, land was assessed
from fictional bid-rents, shops could sell any amount per worker, and the road network froze. A 500-year Georgist
run ended with 26 households, empty commercial towers and tax revenue growing without bound. The mechanisms above
(tax default, as-if-vacant assessment, shop capacity, observed business rents, road planning, congestion and
export demand) are the fixes. A bookkeeping bug did similar damage more quietly: when a worker died, their
employer's skill total lost the heir's skill instead of theirs, so factories built up phantom skill (1.9× their
real staff by year 200). Export income per worker grew fourfold, wages fell and the surplus went to owners.

## What the model currently says, and how much to trust it

These are 6 seeds × 40 years, from `taxsim compare` (mean ± standard error; see the README), with
owner-occupied flats. Before flats, outside capital owned almost all land under every preset except Georgist,
Harberger and the minimal-tax ones (≈ 100% vs ≈ 63–77%); now it owns 29–52% everywhere, and 56–76% of
households own their home. Differences
smaller than a couple of standard errors are noise. Over 200 years (seed 1) the status quo, Georgist and
Harberger presets pass every audit check, and cities settle at roughly 26 000–36 000 households. The Nordic
preset passes in 6 of 8 seeds over 100 years; in the other two, unemployment briefly exceeds the 25% bound
during an export slump (a town whose jobs are almost all in export industry, with a single shop or two).

* **Georgist** (6% land value tax, dividend):
  * a larger city than the status quo (≈ 15 100 vs ≈ 11 900 households, about five standard errors), higher
    output, lower rents and taller buildings;
  * more land owned by residents (absentee share ≈ 34% vs ≈ 48%) and higher average resident welfare;
  * but fewer owner-occupiers (≈ 57% vs ≈ 75%): the 6% land tax is a large fixed charge on every flat, and
    the status quo's surplus rebate leaves tenants more cash for a deposit;
  * and modest revenue: land rent is small where land is plentiful and buildings are priced near construction
    cost, so a 6% land tax raises about a third of the status quo's revenue;
  * and much higher wealth inequality (no income or wealth taxes).
* **Wealth tax** (2% above 1M): lower wealth inequality than the status quo and more departures of wealthy
  households, and a slightly smaller city (about two standard errors).
* **Nordic** (high income tax and VAT): the lowest inequality, the highest revenue and many deterred workers
  (only the flat tax deters more). In this run it is also one of the largest cities.
* **Harberger at 4% of total value:** the smallest city, with almost no building above one floor: the
  self-assessed tax falls on buildings too, so waiting beats building. A mechanism-level warning about COST rates,
  not a calibrated forecast.
* **Head tax:** a large but very unequal city with little revenue.

**Known limitations and sensitivities.** Read the results as "what these mechanisms produce", not as forecasts.
* **Ownership granularity.** Households can own a whole parcel or the single flat they live in. They cannot
  buy flats to let them out (buy-to-let), and only tenants buy, so there is no market for flats between
  households: a flat changes hands through the parcel owner. Flats pay their share of a Harberger tax on the
  owner's declaration, which only covers the parcel owner's share, so flats under Harberger are approximate.
  Owner-occupation settles high (roughly 70–85%, above most real cities) because the deposit is the only
  barrier: there is no mortgage approval beyond a payment-to-income cap and no preference for renting.
* **Public goods matter a lot.** Service coverage adds up to 380/month of location value, and roads gate
  development. Low-revenue regimes therefore grow less. This is the Henry George mechanism, but its strength is
  a parameter (`fields.rs`: `SERVICE_VALUE`, `PARK_VALUE`, `POLLUTION_COST`).
* **Factories outbid homes without zoning.** A floor holds 14 factory jobs but only 4 dwellings, so per floor
  industry can pay more rent than housing. Real industrial rents per m² are well below residential ones. With
  zoning off, homes near industry are still rebuilt as factories, often when they are only 10–20 years old.
  Real cities mostly prevent this with use zoning. In test runs, turning zoning on cut rebuilds by 2–10×.
* **Stochastic and path-dependent.** Housing cycles emerge from construction lags, and terrain matters. Use many
  seeds.
* **City size** is set by the outside demand for the city's exports far more than by congestion. With a single
  export good in a fixed market, the city stopped at roughly 30 000–40 000 households whatever the map size or
  congestion. With offices, product variety and import substitution it grows to about 80 000–130 000
  households after 200 years, until it fills the default map and builds up: towers of 20–30 floors downtown,
  and in the high-tax presets most dwellings in buildings of 5+ floors. That makes the map edge (no suburbs or
  neighbouring towns) the binding limit late in long runs. The variety and office parameters are the main
  levers on city size, and they are set by eye with `taxsim longrun`, not estimated. Outside capital is never
  the limit: absentee investors have unlimited funds at a 6% return. A bigger city is proportionally slower to
  simulate.
* **Not modelled:** monetary policy, credit cycles beyond simple mortgages, international trade beyond a single
  export good, local traffic congestion and the street network's actual shape (commutes are straight-line
  distance times a fixed detour factor, with a city-wide congestion factor, so roads add value by unlocking land
  rather than by shortening trips),
  multiple competing municipalities, and political feedback.
* **Calibration** targets the stylized facts in the research notes (vacancy 3–10%, rent burden 25–35%,
  income Gini 0.2–0.5) loosely, by hand. A systematic calibration with `taxsim` sweeps is future work.

## Scope: done, next, out

**Done (this version):**
* all tax instruments above, with a revenue-recycling mix;
* an open city with migration, housing, labour and land markets, development with real options, maintenance and
  decay, Harberger buy-outs, owner-occupied flats (rent vs. buy), inheritance, pollution and nature;
* counterfactual distortion metrics and mechanism tests;
* the 3D browser front end with overlays, inspector and baseline comparison, plus the native batch runner.

**Next:**
* buy-to-let flats and a market for flats between households;
* more export sectors (tourism, a university or hospital serving the region) and rival cities competing for
  the same markets;
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
