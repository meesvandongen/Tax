//! Roads: when the government builds them, and where.
//!
//! No land is reserved in advance. Owners may build anywhere their parcel
//! has street access, and when the city needs a new street the government
//! buys the parcels on its route outright, at market value plus a premium
//! (`SpendingPolicy::purchase_premium`). A building in the way is bought,
//! its tenants compensated and it is demolished, which makes it expensive,
//! so routes bend around buildings when they can.
//!
//! **When.** The planner acts only if
//! 1. serviced land is short: the parcels with street access that a
//!    developer would build on at a profit are fewer than about two years
//!    of construction starts, and
//! 2. a project is worth it: the land value it creates (the value of the
//!    parcels it brings into reach of a street, minus their value as
//!    farmland) is at least its cost to the budget, and
//! 3. the road fund can pay for it. If the best project is not yet
//!    affordable but will be within a year, the planner saves up for it
//!    instead of building a worse one.
//!
//! **What.** A project is a chain of parcels branching off the network.
//! Routes are least-cost paths over the parcel mesh: each parcel costs its
//! purchase price, demolition and tenant compensation, plus construction
//! (more on slopes, ten times more for a bridge), and turning off a row of
//! parcels costs extra, so streets follow the mesh's flowing rows. One
//! search from the whole network prices a route to every parcel; the most
//! promising destinations (much unserved potential nearby, cheap to reach)
//! are then evaluated exactly, and the best benefit/cost ratio wins.
//!
//! The value created is the land value a road unlocks, which by the
//! Henry George theorem is also what a land value tax can recover.

use crate::econ;
use crate::land::{compulsory_purchase, farm_value};
use crate::types::*;
use crate::world::{Event, World};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// Construction cost per road parcel, and per bridge parcel over water.
pub const ROAD_COST: f64 = 40_000.0;
pub const BRIDGE_COST: f64 = 400_000.0;
/// A street serves parcels up to this many steps away: the lots fronting it
/// and the lots behind them.
pub const ACCESS_REACH: u32 = 2;
/// Extra cost, in road parcels, of turning off a straight row of parcels.
const TURN_PENALTY: f64 = 0.35;
/// Longest project, in parcels.
pub const MAX_PROJECT: usize = 18;
/// Destinations evaluated in full each planning round.
const CANDIDATES: usize = 24;
/// Land value created per unit of cost a project must at least achieve.
pub const MIN_RETURN: f64 = 1.0;
/// How long the planner will save up for a better project (months).
const SAVE_MONTHS: f64 = 12.0;

#[derive(PartialEq)]
struct Item(f64, u32);
impl Eq for Item {}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
    }
}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Least-cost chains of parcels. States are (parcel, edge it was entered
/// through), so a step straight across to the opposite edge can be told
/// from a turn.
pub struct Routes {
    cost: Vec<f64>,
    prev: Vec<u32>,
    /// Sum of `along` over the parcels of the route.
    acc: Vec<f64>,
}

impl Routes {
    /// Search outward from `starts` = (parcel, entry edge, cost). Entering
    /// parcel `j` from `i` costs `enter(j, i)` (infinite = impassable),
    /// turning costs `turn` more. Stops beyond `max_cost` or `max_len` parcels,
    /// or once `goal` is reached. Routes also total `along` over their parcels.
    #[allow(clippy::too_many_arguments)]
    pub fn search(
        w: &World,
        starts: &[(usize, usize, f64)],
        enter: impl Fn(usize, usize) -> f64,
        along: impl Fn(usize) -> f64,
        turn: f64,
        max_cost: f64,
        max_len: usize,
        goal: Option<usize>,
    ) -> Routes {
        let m = &w.mesh;
        let ns = 4 * m.len();
        let mut cost = vec![f64::INFINITY; ns];
        let mut prev = vec![NONE; ns];
        let mut len = vec![0u16; ns];
        let mut acc = vec![0f64; ns];
        let mut heap = BinaryHeap::new();
        for &(c, e, c0) in starts {
            let s = 4 * c + e;
            if c0 < cost[s] {
                cost[s] = c0;
                len[s] = 1;
                acc[s] = along(c);
                heap.push(Item(c0, s as u32));
            }
        }
        while let Some(Item(cs, s)) = heap.pop() {
            let s = s as usize;
            if cs > cost[s] {
                continue;
            }
            let (i, e) = (s / 4, s % 4);
            if goal == Some(i) {
                break;
            }
            if len[s] as usize >= max_len {
                continue;
            }
            for k in 0..4 {
                let j = m.adj[i][k];
                if j == NONE || k == e {
                    continue;
                }
                let j = j as usize;
                let step = enter(j, i);
                if !step.is_finite() {
                    continue;
                }
                let c = cs + step + if k == (e + 2) % 4 { 0.0 } else { turn };
                let t = 4 * j + m.edge_to(j, i);
                if c < cost[t] && c <= max_cost {
                    cost[t] = c;
                    prev[t] = s as u32;
                    len[t] = len[s] + 1;
                    acc[t] = acc[s] + along(j);
                    heap.push(Item(c, t as u32));
                }
            }
        }
        Routes { cost, prev, acc }
    }

    /// Cheapest arrival at parcel `i`: (cost, state).
    pub fn best(&self, i: usize) -> (f64, usize) {
        (0..4).map(|e| (self.cost[4 * i + e], 4 * i + e)).min_by(|a, b| a.0.total_cmp(&b.0)).unwrap()
    }

    /// The parcels from the start of the route to `state`'s parcel.
    pub fn path(&self, state: usize) -> Vec<usize> {
        let mut out = vec![state / 4];
        let mut s = state;
        while self.prev[s] != NONE {
            s = self.prev[s] as usize;
            out.push(s / 4);
        }
        out.reverse();
        out
    }
}

/// Parcels within [`ACCESS_REACH`] steps of `cells` (steps do not cross water).
pub fn served_by(w: &World, cells: &[usize]) -> Vec<usize> {
    let mut seen: HashSet<usize> = cells.iter().copied().collect();
    let mut frontier: Vec<usize> = cells.to_vec();
    let mut out = Vec::new();
    for _ in 0..ACCESS_REACH {
        let mut next = Vec::new();
        for &i in &frontier {
            for j in w.mesh.neighbors(i) {
                if seen.insert(j) {
                    out.push(j);
                    if !w.terrain.water[j] {
                        next.push(j);
                    }
                }
            }
        }
        frontier = next;
    }
    out.sort_unstable();
    out
}

pub fn set_road(w: &mut World, i: usize) {
    w.p.land_use[i] = LandUse::Road;
    w.p.owner[i] = Owner::Gov;
    w.p.nature[i] = 0.0;
    w.p.units[i] = 0;
    w.p.floors[i] = 0;
    w.p.road_access[i] = true;
    for j in served_by(w, &[i]) {
        w.p.road_access[j] = true;
    }
}

fn build_cost(w: &World, j: usize) -> f64 {
    let base = if w.terrain.water[j] { BRIDGE_COST } else { ROAD_COST };
    base * w.index as f64 * (1.0 + w.terrain.slope[j] as f64 / 4.0)
}

/// What it costs the budget to put a road on parcel `j`: purchase plus the
/// premium, demolition and tenant compensation for a building, and
/// construction. Infinite where no road may go. With `exact`, the purchase
/// is priced as [`compulsory_purchase`] will pay (the owner's valuation, if
/// above the assessment); otherwise at the assessment, for route finding.
pub fn parcel_cost(w: &World, j: usize, exact: bool) -> f64 {
    let u = w.p.land_use[j];
    if matches!(u, LandUse::Road | LandUse::Park | LandUse::Civic | LandUse::Construction) {
        return f64::INFINITY;
    }
    let mut c = build_cost(w, j);
    if u != LandUse::Water && w.p.owner[j] != Owner::Gov {
        let value = if exact {
            econ::valuation(w, j, &w.investor_for(w.p.owner[j])).max(w.parcel_value(j))
        } else {
            w.parcel_value(j)
        };
        c += value * (1.0 + w.cfg.policy.spending.purchase_premium as f64);
    }
    if u.is_building() {
        c += econ::demolition_cost(w, j) + econ::tenant_compensation(w, j);
    }
    c
}

/// How many parcels serviced land is short of about two years of
/// construction starts, counting the parcels with street access that a
/// developer would build on at a profit (zero or less: enough).
pub fn land_shortfall(w: &World) -> f64 {
    let inv = w.absentee_investor();
    let need = 40.0 + 24.0 * w.gov.starts_ema;
    let mut idle = 0.0;
    for i in 0..w.p.len() {
        if w.p.road_access[i]
            && matches!(w.p.land_use[i], LandUse::Wild | LandUse::Farm | LandUse::Vacant)
            && econ::vacant_site_value(w, i, &inv) > 0.0
        {
            idle += 1.0;
            if idle > need {
                break;
            }
        }
    }
    need - idle
}

#[derive(Clone, Debug)]
pub struct Project {
    pub path: Vec<usize>,
    pub cost: f64,
    /// Land value created: newly served parcels' development value over farmland.
    pub gain: f64,
    /// Newly served parcels worth developing.
    pub served: Vec<usize>,
}

impl Project {
    pub fn ratio(&self) -> f64 {
        self.gain / self.cost.max(1.0)
    }
}

/// Candidate projects, best benefit/cost first, among routes costing at
/// most `max_cost`.
pub fn candidates(w: &World, max_cost: f64) -> Vec<Project> {
    let n = w.p.len();
    let m = &w.mesh;
    let is_road = |j: usize| w.p.land_use[j] == LandUse::Road;
    let road_nbs = |j: usize| m.neighbors(j).filter(|&k| is_road(k)).count();
    // Branch off any road parcel into a parcel touching only that road.
    let mut starts = Vec::new();
    for r in (0..n).filter(|&r| is_road(r)) {
        for j in m.neighbors(r) {
            if road_nbs(j) == 1 {
                let c = parcel_cost(w, j, false);
                if c.is_finite() {
                    starts.push((j, m.edge_to(j, r), c));
                }
            }
        }
    }
    if starts.is_empty() {
        return vec![];
    }
    // Beyond its first parcel a new street keeps clear of the network (no
    // parallel streets, no paved blocks).
    let enter = |j: usize, _from: usize| if road_nbs(j) > 0 { f64::INFINITY } else { parcel_cost(w, j, false) };
    // Screen routes by the land value they would unlock per unit of cost,
    // estimated as an appraiser would: unserved land is worth about what
    // serviced land nearby is assessed at (over its farm value).
    let inv = w.absentee_investor();
    let open = |j: usize| !w.terrain.water[j] && w.p.land_use[j].is_developable();
    let mut comps = vec![0f32; n];
    let mut weight = vec![0f32; n];
    for j in (0..n).filter(|&j| w.p.road_access[j] && open(j)) {
        comps[j] = (w.p.assessed_land[j] - farm_value(w, j, &inv) as f32).max(0.0);
        weight[j] = 1.0;
    }
    let appraised = m.smoothed_mean(&comps, &weight, 3.0, 0.0);
    let potential: Vec<f32> = (0..n).map(|j| if !w.p.road_access[j] && open(j) { appraised[j] } else { 0.0 }).collect();
    let near_potential = m.blur(&potential, 1.0);
    let turn = TURN_PENALTY * ROAD_COST * w.index as f64;
    let along = |j: usize| near_potential[j] as f64;
    let routes = Routes::search(w, &starts, enter, along, turn, max_cost, MAX_PROJECT, None);
    let mut dest: Vec<(f64, usize, usize)> = (0..n)
        .filter_map(|t| {
            let (c, s) = routes.best(t);
            (c.is_finite() && routes.acc[s] > 0.0).then(|| (routes.acc[s] / c, t, s))
        })
        .collect();
    dest.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));

    let social = inv.untaxed();
    let mut gains: HashMap<usize, f64> = HashMap::new();
    let mut gain = |j: usize| -> f64 {
        *gains
            .entry(j)
            .or_insert_with(|| (econ::serviced_site_value(w, j, &social, 0.0) - farm_value(w, j, &inv)).max(0.0))
    };
    // Projects are priced exactly, before any of their road exists: land
    // is bought at its value without the scheme (a road raises the value of
    // the parcels further along its own route).
    let mut quotes: HashMap<usize, f64> = HashMap::new();
    let mut quote = |j: usize| -> f64 { *quotes.entry(j).or_insert_with(|| parcel_cost(w, j, true)) };
    let mut chosen: Vec<usize> = Vec::new();
    let mut out = Vec::new();
    for &(_, t, s) in &dest {
        if out.len() >= CANDIDATES {
            break;
        }
        if chosen.iter().any(|&c| m.dist(c, t) < 3.0) {
            continue;
        }
        chosen.push(t);
        let path = routes.path(s);
        let served: Vec<usize> = served_by(w, &path)
            .into_iter()
            .filter(|&j| !w.p.road_access[j] && !path.contains(&j))
            .filter(|&j| !w.terrain.water[j] && w.p.land_use[j].is_developable())
            .collect();
        let served: Vec<usize> = served.into_iter().filter(|&j| gain(j) > 0.0).collect();
        let g: f64 = served.iter().map(|&j| gain(j)).sum();
        let cost = path.iter().map(|&j| quote(j)).sum();
        out.push(Project { path, cost, gain: g, served });
    }
    out.sort_by(|a, b| b.ratio().total_cmp(&a.ratio()));
    out
}

/// Quarterly road programme, given the monthly allocation to the road fund:
/// while serviced land is short, fund the projects with the best
/// benefit/cost ratio (at least [`MIN_RETURN`]) until they serve the
/// shortfall or the fund runs out. If the best project is out of reach for
/// now but affordable within [`SAVE_MONTHS`], save up for it instead.
pub fn plan(w: &mut World, allocation: f64) {
    let mut budget = w.gov.infra_budget;
    if w.tick % 3 != 0 || budget < ROAD_COST * w.index as f64 {
        return;
    }
    let mut short = land_shortfall(w);
    if short <= 0.0 {
        return;
    }
    let horizon = budget + SAVE_MONTHS * allocation.max(0.0);
    let projects: Vec<Project> =
        candidates(w, horizon).into_iter().filter(|p| p.ratio() >= MIN_RETURN && !p.served.is_empty()).collect();
    let Some(best) = projects.first() else { return };
    if best.cost > budget && (best.cost - budget) / allocation.max(1.0) <= SAVE_MONTHS {
        return;
    }
    // Projects are priced before any road is built, so each must keep clear
    // of the land the others serve.
    let mut taken: HashSet<usize> = HashSet::new();
    let mut chosen = Vec::new();
    for p in &projects {
        if short <= 0.0 {
            break;
        }
        if p.cost > budget || p.path.iter().any(|c| taken.contains(c)) {
            continue;
        }
        taken.extend(p.path.iter().copied());
        taken.extend(served_by(w, &p.path));
        budget -= p.cost;
        short -= p.served.len() as f64;
        chosen.push(p);
    }
    for p in chosen {
        build(w, &p.path);
    }
}

/// Buy the parcels on `path`, then lay the road.
pub fn build(w: &mut World, path: &[usize]) {
    for &i in path {
        if matches!(w.p.land_use[i], LandUse::Road | LandUse::Water) {
            continue;
        }
        let paid = compulsory_purchase(w, i);
        w.gov.infra_budget -= paid;
        if w.p.land_use[i].is_building() {
            // The government now owns the building: it pays its
            // tenants' relocation and the demolition.
            let relocation = crate::development::clear_site(w, i);
            let demolition = econ::demolition_cost(w, i);
            w.gov.cash -= relocation + demolition;
            w.gov.infra_budget -= relocation + demolition;
            w.gov.spend_compensation += relocation;
            w.gov.spend_infrastructure += demolition;
            w.flows.construction_spend += demolition;
            w.gov.roads_expropriated += 1;
        }
    }
    for &i in path {
        if w.p.land_use[i] == LandUse::Road {
            continue;
        }
        let build = build_cost(w, i);
        w.gov.infra_budget -= build;
        w.gov.cash -= build;
        w.gov.spend_infrastructure += build;
        w.flows.construction_spend += build;
        set_road(w, i);
        w.gov.roads_built += 1;
        w.push_event(Event::Road { cell: i as u32 });
    }
    w.gov.road_projects += 1;
}

pub fn maintain(w: &mut World) {
    let roads = w.p.land_use.iter().filter(|&&u| u == LandUse::Road).count() as f64;
    let cost = roads * 60.0 * w.index as f64;
    let paid = cost.min(w.gov.infra_budget.max(0.0));
    w.gov.infra_budget -= paid;
    w.gov.cash -= paid;
    w.gov.spend_infrastructure += paid;
    w.flows.construction_spend += paid;
}

/// Road parcels that cannot be reached from the founding crossing along roads.
pub fn disconnected(w: &World) -> usize {
    let is_road = |i: usize| w.p.land_use[i] == LandUse::Road;
    let reach = w.mesh.distances(&[w.center], f32::INFINITY, is_road);
    (0..w.p.len()).filter(|&i| is_road(i) && !reach[i].is_finite()).count()
}
