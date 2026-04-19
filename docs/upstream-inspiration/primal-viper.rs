use super::*;
use anyhow::Result;
use serde_json::{Map, Value};
use std::time::Instant;

const TIME_LIMIT_MS: u128 = 27_000;

pub fn solve_challenge(
    challenge: &Challenge,
    save_solution: &dyn Fn(&Solution) -> Result<()>,
    _hyperparameters: &Option<Map<String, Value>>,
) -> Result<()> {
    let start = Instant::now();

    // Precompute K nearest-customer neighbor lists (excluding depot). Used by or-opt
    // inter-route to restrict insertion candidates to positions near a spatial neighbor
    // of the segment's endpoints — cheap insertions almost always happen there.
    const K_NEIGHBORS: usize = 40;
    let neighbors: Vec<Vec<usize>> = (0..challenge.num_nodes)
        .map(|c| {
            if c == 0 {
                return Vec::new();
            }
            let mut others: Vec<usize> = (1..challenge.num_nodes).filter(|&o| o != c).collect();
            others.sort_by_key(|&o| challenge.distance_matrix[c][o]);
            others.truncate(K_NEIGHBORS);
            others
        })
        .collect();

    let mut current = super::solomon::run(challenge)?;
    drop_empty(&mut current.routes);
    let mut current_demand: Vec<i32> = current
        .routes
        .iter()
        .map(|r| r.iter().map(|&n| challenge.demands[n]).sum())
        .collect();

    local_search_loop(&mut current, &mut current_demand, challenge, &neighbors, &start);
    let mut best = current.clone();
    drop_empty(&mut best.routes);
    let mut best_dist = total_distance(&best.routes, &challenge.distance_matrix);
    save_solution(&best)?;

    // Seed an RNG from the challenge seed for reproducibility.
    let mut rng_state: u64 = 0;
    for (i, &b) in challenge.seed.iter().take(8).enumerate() {
        rng_state |= (b as u64) << (8 * i);
    }
    if rng_state == 0 {
        rng_state = 0x9E3779B97F4A7C15;
    }
    let mut rng = XorshiftRng { state: rng_state };

    let num_customers = challenge.num_nodes.saturating_sub(1);
    let mut iter: u32 = 0;
    while start.elapsed().as_millis() < TIME_LIMIT_MS {
        let mut candidate = best.clone();
        let mut cand_demand: Vec<i32> = candidate
            .routes
            .iter()
            .map(|r| r.iter().map(|&n| challenge.demands[n]).sum())
            .collect();

        // Vary removal size: mostly ~20% with occasional bigger/smaller kicks.
        let size_bucket = rng.next() % 10;
        let frac_denom = match size_bucket {
            0..=1 => 8,   // ~12.5% — small kick
            2..=7 => 5,   // ~20%  — default
            _ => 3,       // ~33%  — big kick
        };
        let num_remove = ((num_customers / frac_denom).max(10)).min(num_customers);

        let removed = match iter % 3 {
            0 => destroy_random(&mut candidate.routes, &mut cand_demand, challenge, &mut rng, num_remove),
            1 => destroy_worst(&mut candidate.routes, &mut cand_demand, challenge, &mut rng, num_remove),
            _ => destroy_shaw(&mut candidate.routes, &mut cand_demand, challenge, &mut rng, num_remove),
        };
        // Drop routes that became empty after destroy.
        let mut i_r = 0;
        while i_r < candidate.routes.len() {
            if candidate.routes[i_r].len() <= 2 {
                candidate.routes.remove(i_r);
                cand_demand.remove(i_r);
            } else {
                i_r += 1;
            }
        }
        // Regret repair is slower per step; use it occasionally for diversification.
        let use_regret = iter % 4 == 0;
        if use_regret {
            repair_regret_k(
                &mut candidate.routes,
                &mut cand_demand,
                challenge,
                removed,
                2,
            );
        } else {
            repair_greedy(
                &mut candidate.routes,
                &mut cand_demand,
                challenge,
                &mut rng,
                removed,
            );
        }

        local_search_loop(&mut candidate, &mut cand_demand, challenge, &neighbors, &start);

        drop_empty(&mut candidate.routes);
        let cand_dist = total_distance(&candidate.routes, &challenge.distance_matrix);
        if cand_dist < best_dist {
            best_dist = cand_dist;
            best = candidate.clone();
            save_solution(&best)?;
        }
        iter = iter.wrapping_add(1);
    }

    Ok(())
}

fn local_search_loop(
    solution: &mut Solution,
    route_demand: &mut Vec<i32>,
    challenge: &Challenge,
    neighbors: &[Vec<usize>],
    start: &Instant,
) {
    loop {
        if start.elapsed().as_millis() > TIME_LIMIT_MS {
            break;
        }
        let mut improved = false;

        for r in 0..solution.routes.len() {
            if start.elapsed().as_millis() > TIME_LIMIT_MS {
                break;
            }
            if two_opt_route(
                &mut solution.routes[r],
                &challenge.distance_matrix,
                challenge.service_time,
                &challenge.ready_times,
                &challenge.due_times,
            ) {
                improved = true;
            }
            if intra_or_opt_route(
                &mut solution.routes[r],
                &challenge.distance_matrix,
                challenge.service_time,
                &challenge.ready_times,
                &challenge.due_times,
            ) {
                improved = true;
            }
        }

        for seg_len in 1..=3 {
            if start.elapsed().as_millis() > TIME_LIMIT_MS {
                break;
            }
            if or_opt_pass(
                &mut solution.routes,
                route_demand,
                challenge,
                neighbors,
                start,
                seg_len,
            ) {
                improved = true;
            }
        }

        if exchange_pass(&mut solution.routes, route_demand, challenge, start) {
            improved = true;
        }

        if two_opt_star_pass(&mut solution.routes, route_demand, challenge, start) {
            improved = true;
        }

        if eliminate_routes_pass(&mut solution.routes, route_demand, challenge, start) {
            improved = true;
        }

        if !improved {
            break;
        }
    }
}

struct XorshiftRng {
    state: u64,
}

impl XorshiftRng {
    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// Worst-removal destroy: remove customers with largest current detour cost (with Shaw-style
// u^3 bias so we don't always pick the same few). Returns removed customers.
fn destroy_worst(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    ch: &Challenge,
    rng: &mut XorshiftRng,
    num_remove: usize,
) -> Vec<usize> {
    let mut costs: Vec<(i32, usize, usize)> = Vec::new(); // (cost, route, pos)
    for (r, route) in routes.iter().enumerate() {
        for p in 1..route.len().saturating_sub(1) {
            let prev = route[p - 1];
            let cur = route[p];
            let next = route[p + 1];
            let c = ch.distance_matrix[prev][cur] + ch.distance_matrix[cur][next]
                - ch.distance_matrix[prev][next];
            costs.push((c, r, p));
        }
    }
    if costs.is_empty() {
        return Vec::new();
    }
    // Sort descending by cost.
    costs.sort_by(|a, b| b.0.cmp(&a.0));
    // Randomized selection: pick index ranked by `r^3 * n` where r is uniform in [0,1),
    // biasing toward higher-cost customers (Shaw-style noise).
    let mut picked: Vec<(usize, usize)> = Vec::new();
    let mut used: std::collections::HashSet<(usize, usize)> =
        std::collections::HashSet::new();
    while picked.len() < num_remove && used.len() < costs.len() {
        let u = (rng.next() as f64) / (u64::MAX as f64);
        let idx = (u * u * u * costs.len() as f64) as usize;
        let idx = idx.min(costs.len() - 1);
        let (_, r, p) = costs[idx];
        if used.insert((r, p)) {
            picked.push((r, p));
        }
    }

    // Remove (descending within route) and collect.
    let mut by_route: Vec<Vec<usize>> = vec![Vec::new(); routes.len()];
    for &(r, p) in &picked {
        by_route[r].push(p);
    }
    let mut removed: Vec<usize> = Vec::with_capacity(picked.len());
    for r in 0..routes.len() {
        by_route[r].sort_unstable();
        for &p in by_route[r].iter().rev() {
            let c = routes[r].remove(p);
            route_demand[r] -= ch.demands[c];
            removed.push(c);
        }
    }

    removed
}

// Shaw relatedness destroy: pick a seed customer, then grow the removal set by greedily
// taking customers spatially close to an already-picked one (with u^p bias). Produces a
// spatially coherent destroy that LNS can rearrange together, giving larger structural
// moves than random or worst-removal, which tend to be scattered.
fn destroy_shaw(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    ch: &Challenge,
    rng: &mut XorshiftRng,
    num_remove: usize,
) -> Vec<usize> {
    let n = ch.num_nodes;
    let mut cust_route: Vec<Option<(usize, usize)>> = vec![None; n];
    let mut all_custs: Vec<usize> = Vec::new();
    for (r, route) in routes.iter().enumerate() {
        for p in 1..route.len().saturating_sub(1) {
            let c = route[p];
            cust_route[c] = Some((r, p));
            all_custs.push(c);
        }
    }
    if all_custs.is_empty() {
        return Vec::new();
    }

    let mut picked_set: Vec<bool> = vec![false; n];
    let mut picked_list: Vec<usize> = Vec::new();

    let seed = all_custs[rng.range(all_custs.len())];
    picked_set[seed] = true;
    picked_list.push(seed);

    while picked_list.len() < num_remove && picked_list.len() < all_custs.len() {
        let ref_c = picked_list[rng.range(picked_list.len())];
        let mut candidates: Vec<(i32, usize)> = all_custs
            .iter()
            .filter(|&&c| !picked_set[c])
            .map(|&c| (ch.distance_matrix[ref_c][c], c))
            .collect();
        if candidates.is_empty() {
            break;
        }
        candidates.sort_unstable_by_key(|&(d, _)| d);
        let u = (rng.next() as f64) / (u64::MAX as f64);
        // bias ~ u^6: strongly favor small-distance picks
        let idx = (u.powi(6) * candidates.len() as f64) as usize;
        let idx = idx.min(candidates.len() - 1);
        let c = candidates[idx].1;
        picked_set[c] = true;
        picked_list.push(c);
    }

    // Remove picked customers from their routes (descending per route).
    let mut by_route: Vec<Vec<usize>> = vec![Vec::new(); routes.len()];
    for &c in &picked_list {
        if let Some((r, p)) = cust_route[c] {
            by_route[r].push(p);
        }
    }
    let mut removed: Vec<usize> = Vec::with_capacity(picked_list.len());
    for r in 0..routes.len() {
        by_route[r].sort_unstable();
        for &p in by_route[r].iter().rev() {
            let c = routes[r].remove(p);
            route_demand[r] -= ch.demands[c];
            removed.push(c);
        }
    }
    removed
}

// Random removal destroy. Returns removed customers.
fn destroy_random(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    ch: &Challenge,
    rng: &mut XorshiftRng,
    num_remove: usize,
) -> Vec<usize> {
    let mut positions: Vec<(usize, usize)> = Vec::new();
    for (r, route) in routes.iter().enumerate() {
        for p in 1..route.len().saturating_sub(1) {
            positions.push((r, p));
        }
    }
    if positions.is_empty() {
        return Vec::new();
    }
    // Fisher-Yates shuffle
    for i in (1..positions.len()).rev() {
        let j = rng.range(i + 1);
        positions.swap(i, j);
    }
    let k = num_remove.min(positions.len());
    let chosen = &positions[..k];

    // Group removal indices by route, sort descending so remove doesn't shift earlier indices.
    let mut by_route: Vec<Vec<usize>> = vec![Vec::new(); routes.len()];
    for &(r, p) in chosen {
        by_route[r].push(p);
    }
    let mut removed: Vec<usize> = Vec::with_capacity(k);
    for r in 0..routes.len() {
        by_route[r].sort_unstable();
        for &p in by_route[r].iter().rev() {
            let c = routes[r].remove(p);
            route_demand[r] -= ch.demands[c];
            removed.push(c);
        }
    }
    removed
}

// Cheapest-feasible reinsertion. Shuffles order to decorrelate across iterations.
fn repair_greedy(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    ch: &Challenge,
    rng: &mut XorshiftRng,
    mut removed: Vec<usize>,
) {
    for i in (1..removed.len()).rev() {
        let j = rng.range(i + 1);
        removed.swap(i, j);
    }
    for c in removed {
        let mut best: Option<(usize, usize, i32)> = None;
        for sr in 0..routes.len() {
            if route_demand[sr] + ch.demands[c] > ch.max_capacity {
                continue;
            }
            for pos in 1..routes[sr].len() {
                let a = routes[sr][pos - 1];
                let b = routes[sr][pos];
                let cost = ch.distance_matrix[a][c] + ch.distance_matrix[c][b]
                    - ch.distance_matrix[a][b];
                if best.map_or(true, |(_, _, bc)| cost < bc)
                    && insertion_time_feasible(
                        &routes[sr],
                        pos,
                        c,
                        &ch.distance_matrix,
                        ch.service_time,
                        &ch.ready_times,
                        &ch.due_times,
                    )
                {
                    best = Some((sr, pos, cost));
                }
            }
        }
        match best {
            Some((sr, pos, _)) => {
                routes[sr].insert(pos, c);
                route_demand[sr] += ch.demands[c];
            }
            None => {
                routes.push(vec![0, c, 0]);
                route_demand.push(ch.demands[c]);
            }
        }
    }
}

// Regret-k reinsertion: at each step, for each pending customer compute top-k best
// feasible insertion costs (across all routes); insert the customer whose sum of
// (cost_i - cost_0) for i=1..k is largest (i.e. whose alternatives are worst, so we'd
// regret leaving them for later). Opens new routes only if a customer has no feasible
// slot anywhere.
fn repair_regret_k(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    ch: &Challenge,
    mut pending: Vec<usize>,
    k: usize,
) {
    while !pending.is_empty() {
        let mut best_pending_idx: Option<usize> = None;
        let mut best_regret: i64 = i64::MIN;
        let mut best_primary: i32 = i32::MAX;
        let mut best_route: usize = 0;
        let mut best_pos: usize = 0;
        let mut forced_idx: Option<usize> = None;

        for (i, &c) in pending.iter().enumerate() {
            let mut per_route_best: Vec<i32> = Vec::new();
            let mut primary_route: usize = 0;
            let mut primary_pos: usize = 0;
            let mut primary_cost: i32 = i32::MAX;

            for r in 0..routes.len() {
                if route_demand[r] + ch.demands[c] > ch.max_capacity {
                    continue;
                }
                let mut bir: Option<(usize, i32)> = None;
                for pos in 1..routes[r].len() {
                    let a = routes[r][pos - 1];
                    let b = routes[r][pos];
                    let cost = ch.distance_matrix[a][c] + ch.distance_matrix[c][b]
                        - ch.distance_matrix[a][b];
                    if bir.map_or(true, |(_, bc)| cost < bc)
                        && insertion_time_feasible(
                            &routes[r],
                            pos,
                            c,
                            &ch.distance_matrix,
                            ch.service_time,
                            &ch.ready_times,
                            &ch.due_times,
                        )
                    {
                        bir = Some((pos, cost));
                    }
                }
                if let Some((pos, cost)) = bir {
                    per_route_best.push(cost);
                    if cost < primary_cost {
                        primary_cost = cost;
                        primary_route = r;
                        primary_pos = pos;
                    }
                }
            }

            if per_route_best.is_empty() {
                // No feasible slot in any route — force placement of this customer next
                // (opens a new route). Prefer the first such customer we encounter.
                if forced_idx.is_none() {
                    forced_idx = Some(i);
                }
                continue;
            }
            per_route_best.sort_unstable();
            let c0 = per_route_best[0];
            let mut regret: i64 = 0;
            let kk = k.min(per_route_best.len());
            for j in 1..kk {
                regret += (per_route_best[j] - c0) as i64;
            }
            if per_route_best.len() < k {
                regret += 1_000_000 * (k - per_route_best.len()) as i64;
            }

            let pick = regret > best_regret
                || (regret == best_regret && c0 < best_primary);
            if pick {
                best_regret = regret;
                best_primary = c0;
                best_pending_idx = Some(i);
                best_route = primary_route;
                best_pos = primary_pos;
            }
        }

        if let Some(idx) = best_pending_idx {
            let c = pending.swap_remove(idx);
            routes[best_route].insert(best_pos, c);
            route_demand[best_route] += ch.demands[c];
        } else if let Some(idx) = forced_idx {
            let c = pending.swap_remove(idx);
            routes.push(vec![0, c, 0]);
            route_demand.push(ch.demands[c]);
        } else {
            // Shouldn't happen, but safety: open new routes for anything left.
            for c in pending.drain(..) {
                routes.push(vec![0, c, 0]);
                route_demand.push(ch.demands[c]);
            }
        }
    }
}

fn drop_empty(routes: &mut Vec<Vec<usize>>) {
    routes.retain(|r| r.len() > 2);
}

// Try to delete entire routes by reinserting every customer into other routes.
// Eliminating a route saves its two depot edges; even if individual reinsertions are
// marginally worse than the customer's current position, the sum can still beat the
// original route distance. Eliminates at most one route per call (indices shift after
// removal, and the outer loop will call us again next iteration).
fn eliminate_routes_pass(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    ch: &Challenge,
    start: &Instant,
) -> bool {
    let mut order: Vec<usize> = (0..routes.len()).collect();
    order.sort_by_key(|&i| routes[i].len());

    for r in order {
        if start.elapsed().as_millis() > TIME_LIMIT_MS {
            break;
        }
        if r >= routes.len() || routes[r].len() <= 2 {
            continue;
        }
        let route_dist: i32 = routes[r]
            .windows(2)
            .map(|w| ch.distance_matrix[w[0]][w[1]])
            .sum();
        let customers: Vec<usize> = routes[r][1..routes[r].len() - 1].to_vec();

        let mut sim_routes: Vec<Vec<usize>> = routes
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != r)
            .map(|(_, v)| v.clone())
            .collect();
        let mut sim_demand: Vec<i32> = route_demand
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != r)
            .map(|(_, &d)| d)
            .collect();

        let mut total_ins_cost = 0i32;
        let mut success = true;
        for &c in &customers {
            let mut best: Option<(usize, usize, i32)> = None;
            for sr in 0..sim_routes.len() {
                if sim_demand[sr] + ch.demands[c] > ch.max_capacity {
                    continue;
                }
                for pos in 1..sim_routes[sr].len() {
                    let a = sim_routes[sr][pos - 1];
                    let b = sim_routes[sr][pos];
                    let cost = ch.distance_matrix[a][c] + ch.distance_matrix[c][b]
                        - ch.distance_matrix[a][b];
                    if best.map_or(true, |(_, _, bc)| cost < bc)
                        && insertion_time_feasible(
                            &sim_routes[sr],
                            pos,
                            c,
                            &ch.distance_matrix,
                            ch.service_time,
                            &ch.ready_times,
                            &ch.due_times,
                        )
                    {
                        best = Some((sr, pos, cost));
                    }
                }
            }
            match best {
                Some((sr, pos, cost)) => {
                    sim_routes[sr].insert(pos, c);
                    sim_demand[sr] += ch.demands[c];
                    total_ins_cost += cost;
                }
                None => {
                    success = false;
                    break;
                }
            }
        }

        if success && total_ins_cost < route_dist {
            *routes = sim_routes;
            *route_demand = sim_demand;
            return true;
        }
    }
    false
}

// 2-2 segment swap between two routes: swap a length-2 consecutive pair in r1 with a
// length-2 consecutive pair in r2. Generalizes single-customer exchange and can resolve
// route cross-overs that single-swap can't.
fn segment_swap_pass(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    ch: &Challenge,
    start: &Instant,
) -> bool {
    let mut improved_any = false;
    let n = routes.len();
    for r1 in 0..n {
        if start.elapsed().as_millis() > TIME_LIMIT_MS {
            break;
        }
        for r2 in (r1 + 1)..n {
            let l1 = routes[r1].len();
            let l2 = routes[r2].len();
            if l1 < 4 || l2 < 4 {
                continue;
            }
            let mut best: Option<(usize, usize, i32)> = None;
            for i in 1..(l1 - 2) {
                let s1a = routes[r1][i];
                let s1b = routes[r1][i + 1];
                let p1 = routes[r1][i - 1];
                let q1 = routes[r1][i + 2];
                let d_old_1 = ch.distance_matrix[p1][s1a] + ch.distance_matrix[s1b][q1];
                let d_s1 = ch.demands[s1a] + ch.demands[s1b];
                for j in 1..(l2 - 2) {
                    let s2a = routes[r2][j];
                    let s2b = routes[r2][j + 1];
                    let p2 = routes[r2][j - 1];
                    let q2 = routes[r2][j + 2];
                    let d_s2 = ch.demands[s2a] + ch.demands[s2b];
                    let new_d1 = route_demand[r1] - d_s1 + d_s2;
                    let new_d2 = route_demand[r2] - d_s2 + d_s1;
                    if new_d1 > ch.max_capacity || new_d2 > ch.max_capacity {
                        continue;
                    }
                    let d_old_2 = ch.distance_matrix[p2][s2a] + ch.distance_matrix[s2b][q2];
                    let d_new_1 = ch.distance_matrix[p1][s2a] + ch.distance_matrix[s2b][q1];
                    let d_new_2 = ch.distance_matrix[p2][s1a] + ch.distance_matrix[s1b][q2];
                    let delta = (d_new_1 + d_new_2) - (d_old_1 + d_old_2);
                    if delta >= 0 {
                        continue;
                    }
                    if best.map_or(false, |(_, _, d)| delta >= d) {
                        continue;
                    }
                    // feasibility
                    let mut t1 = routes[r1].clone();
                    t1[i] = s2a;
                    t1[i + 1] = s2b;
                    let mut t2 = routes[r2].clone();
                    t2[j] = s1a;
                    t2[j + 1] = s1b;
                    if is_time_feasible(
                        &t1,
                        &ch.distance_matrix,
                        ch.service_time,
                        &ch.ready_times,
                        &ch.due_times,
                    ) && is_time_feasible(
                        &t2,
                        &ch.distance_matrix,
                        ch.service_time,
                        &ch.ready_times,
                        &ch.due_times,
                    ) {
                        best = Some((i, j, delta));
                    }
                }
            }
            if let Some((i, j, _)) = best {
                let s1a = routes[r1][i];
                let s1b = routes[r1][i + 1];
                let s2a = routes[r2][j];
                let s2b = routes[r2][j + 1];
                let d_s1 = ch.demands[s1a] + ch.demands[s1b];
                let d_s2 = ch.demands[s2a] + ch.demands[s2b];
                routes[r1][i] = s2a;
                routes[r1][i + 1] = s2b;
                routes[r2][j] = s1a;
                routes[r2][j + 1] = s1b;
                route_demand[r1] = route_demand[r1] - d_s1 + d_s2;
                route_demand[r2] = route_demand[r2] - d_s2 + d_s1;
                improved_any = true;
            }
        }
    }
    improved_any
}

// Cross-route 2-opt*: pick a split point in each of two routes and swap their tails.
// Can merge two short routes or rebalance customer assignments.
fn two_opt_star_pass(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    ch: &Challenge,
    start: &Instant,
) -> bool {
    let mut improved_any = false;
    let n = routes.len();

    for r1 in 0..n {
        if start.elapsed().as_millis() > TIME_LIMIT_MS {
            break;
        }
        for r2 in (r1 + 1)..n {
            let l1 = routes[r1].len();
            let l2 = routes[r2].len();
            if l1 < 2 || l2 < 2 {
                continue;
            }
            // cum demand prefix: cum[i] = sum of demands of positions 0..=i
            let cum1: Vec<i32> = routes[r1]
                .iter()
                .scan(0i32, |s, &node| {
                    *s += ch.demands[node];
                    Some(*s)
                })
                .collect();
            let cum2: Vec<i32> = routes[r2]
                .iter()
                .scan(0i32, |s, &node| {
                    *s += ch.demands[node];
                    Some(*s)
                })
                .collect();
            let tot1 = cum1[l1 - 1];
            let tot2 = cum2[l2 - 1];

            let mut best: Option<(usize, usize, i32)> = None;
            for i in 0..l1 - 1 {
                let a = routes[r1][i];
                let a_next = routes[r1][i + 1];
                let d_a_anext = ch.distance_matrix[a][a_next];
                for j in 0..l2 - 1 {
                    let b = routes[r2][j];
                    let b_next = routes[r2][j + 1];
                    let old_edges = d_a_anext + ch.distance_matrix[b][b_next];
                    let new_edges = ch.distance_matrix[a][b_next] + ch.distance_matrix[b][a_next];
                    let delta = new_edges - old_edges;
                    if delta >= 0 {
                        continue;
                    }
                    if best.map_or(false, |(_, _, d)| delta >= d) {
                        continue;
                    }
                    let new_d1 = cum1[i] + (tot2 - cum2[j]);
                    let new_d2 = cum2[j] + (tot1 - cum1[i]);
                    if new_d1 > ch.max_capacity || new_d2 > ch.max_capacity {
                        continue;
                    }
                    // Build trial routes and check time feasibility
                    let mut new_r1 = Vec::with_capacity(i + 1 + (l2 - j - 1));
                    new_r1.extend_from_slice(&routes[r1][..=i]);
                    new_r1.extend_from_slice(&routes[r2][j + 1..]);
                    let mut new_r2 = Vec::with_capacity(j + 1 + (l1 - i - 1));
                    new_r2.extend_from_slice(&routes[r2][..=j]);
                    new_r2.extend_from_slice(&routes[r1][i + 1..]);
                    if is_time_feasible(
                        &new_r1,
                        &ch.distance_matrix,
                        ch.service_time,
                        &ch.ready_times,
                        &ch.due_times,
                    ) && is_time_feasible(
                        &new_r2,
                        &ch.distance_matrix,
                        ch.service_time,
                        &ch.ready_times,
                        &ch.due_times,
                    ) {
                        best = Some((i, j, delta));
                    }
                }
            }

            if let Some((i, j, _)) = best {
                let mut new_r1 = Vec::with_capacity(i + 1 + (l2 - j - 1));
                new_r1.extend_from_slice(&routes[r1][..=i]);
                new_r1.extend_from_slice(&routes[r2][j + 1..]);
                let mut new_r2 = Vec::with_capacity(j + 1 + (l1 - i - 1));
                new_r2.extend_from_slice(&routes[r2][..=j]);
                new_r2.extend_from_slice(&routes[r1][i + 1..]);
                route_demand[r1] = cum1[i] + (tot2 - cum2[j]);
                route_demand[r2] = cum2[j] + (tot1 - cum1[i]);
                routes[r1] = new_r1;
                routes[r2] = new_r2;
                improved_any = true;
            }
        }
    }

    improved_any
}

// Regret-k parallel insertion: at each step, for every unassigned customer compute
// their best and k-th best feasible insertion cost across all current routes, then
// insert the customer with largest (sum of top-k gaps to best) at their best position.
// When no customer has any feasible insertion in existing routes, open a new route
// seeded by the farthest remaining customer from the depot.
fn regret_k_construction(ch: &Challenge, k: usize) -> Option<Solution> {
    let n = ch.num_nodes;
    if n < 2 {
        return Some(Solution { routes: Vec::new() });
    }

    let mut routes: Vec<Vec<usize>> = Vec::new();
    let mut route_demand: Vec<i32> = Vec::new();
    let mut remaining: Vec<bool> = vec![true; n];
    remaining[0] = false;

    // Seed first route with the unassigned customer farthest from the depot.
    if let Some(seed) = (1..n).max_by_key(|&i| ch.distance_matrix[0][i]) {
        routes.push(vec![0, seed, 0]);
        route_demand.push(ch.demands[seed]);
        remaining[seed] = false;
    }

    loop {
        let mut best_choice: Option<(usize, usize, usize)> = None; // (customer, route, pos)
        let mut best_regret: i64 = i64::MIN;
        let mut best_primary_cost: i32 = i32::MAX;

        for u in 1..n {
            if !remaining[u] {
                continue;
            }
            // For each route, find the best feasible insertion cost.
            let mut per_route_best: Vec<(usize, i32, usize)> = Vec::new(); // (route, cost, pos)
            for r in 0..routes.len() {
                if route_demand[r] + ch.demands[u] > ch.max_capacity {
                    continue;
                }
                let mut best_in_route: Option<(i32, usize)> = None;
                for ins in 1..routes[r].len() {
                    let a = routes[r][ins - 1];
                    let b = routes[r][ins];
                    let cost = ch.distance_matrix[a][u] + ch.distance_matrix[u][b]
                        - ch.distance_matrix[a][b];
                    if best_in_route.map_or(true, |(c, _)| cost < c)
                        && insertion_time_feasible(
                            &routes[r],
                            ins,
                            u,
                            &ch.distance_matrix,
                            ch.service_time,
                            &ch.ready_times,
                            &ch.due_times,
                        )
                    {
                        best_in_route = Some((cost, ins));
                    }
                }
                if let Some((cost, pos)) = best_in_route {
                    per_route_best.push((r, cost, pos));
                }
            }
            if per_route_best.is_empty() {
                continue;
            }
            per_route_best.sort_by_key(|&(_, c, _)| c);
            let (best_r, best_cost, best_pos) = per_route_best[0];
            let mut regret: i64 = 0;
            for i in 1..k.min(per_route_best.len()) {
                regret += (per_route_best[i].1 - best_cost) as i64;
            }
            // If fewer than k routes are feasible, penalize less — a customer with only
            // one feasible route is tight and should still be placed early.
            if per_route_best.len() < k {
                regret += 1_000_000 * (k - per_route_best.len()) as i64;
            }

            let pick = regret > best_regret
                || (regret == best_regret && best_cost < best_primary_cost);
            if pick {
                best_regret = regret;
                best_primary_cost = best_cost;
                best_choice = Some((u, best_r, best_pos));
            }
        }

        match best_choice {
            Some((u, r, pos)) => {
                routes[r].insert(pos, u);
                route_demand[r] += ch.demands[u];
                remaining[u] = false;
            }
            None => {
                // No feasible insertion in any existing route — open a new one with the
                // farthest remaining customer.
                let seed = (1..n)
                    .filter(|&i| remaining[i])
                    .max_by_key(|&i| ch.distance_matrix[0][i]);
                match seed {
                    Some(s) => {
                        routes.push(vec![0, s, 0]);
                        route_demand.push(ch.demands[s]);
                        remaining[s] = false;
                    }
                    None => break,
                }
            }
        }
    }

    // Safety: all customers must be placed
    for i in 1..n {
        if remaining[i] {
            return None;
        }
    }
    Some(Solution { routes })
}

// Check time-window feasibility of inserting `u` at position `ins` in `route`
// without materializing a new vector.
fn insertion_time_feasible(
    route: &[usize],
    ins: usize,
    u: usize,
    dm: &[Vec<i32>],
    service_time: i32,
    ready: &[i32],
    due: &[i32],
) -> bool {
    let new_len = route.len() + 1;
    let mut curr_time = 0i32;
    let mut curr_node = 0usize;
    for new_pos in 1..new_len {
        let next = if new_pos == ins {
            u
        } else if new_pos < ins {
            route[new_pos]
        } else {
            route[new_pos - 1]
        };
        curr_time += dm[curr_node][next];
        if curr_time > due[next] {
            return false;
        }
        curr_time = curr_time.max(ready[next]) + service_time;
        curr_node = next;
    }
    true
}

fn total_distance(routes: &[Vec<usize>], dm: &[Vec<i32>]) -> i32 {
    let mut t = 0;
    for route in routes {
        for w in route.windows(2) {
            t += dm[w[0]][w[1]];
        }
    }
    t
}

fn is_time_feasible(
    route: &[usize],
    dm: &[Vec<i32>],
    service_time: i32,
    ready: &[i32],
    due: &[i32],
) -> bool {
    let mut curr_time = 0i32;
    let mut curr_node = 0usize;
    for pos in 1..route.len() {
        let next = route[pos];
        curr_time += dm[curr_node][next];
        if curr_time > due[next] {
            return false;
        }
        curr_time = curr_time.max(ready[next]) + service_time;
        curr_node = next;
    }
    true
}

// Intra-route or-opt: for each segment of length 1..=3, try moving it to another
// position within the same route (without reversing). Complements 2-opt, which only
// reverses subsections. First-improvement with restart on change.
fn intra_or_opt_route(
    route: &mut Vec<usize>,
    dm: &[Vec<i32>],
    service_time: i32,
    ready: &[i32],
    due: &[i32],
) -> bool {
    let mut improved_any = false;
    let mut changed = true;
    while changed {
        changed = false;
        for seg_len in 1..=3usize {
            let n = route.len();
            if n < seg_len + 3 {
                continue;
            }
            let mut s = 1usize;
            while s + seg_len <= n - 1 {
                let e = s + seg_len - 1; // inclusive end of segment
                let prev = route[s - 1];
                let first = route[s];
                let last = route[e];
                let next = route[e + 1];
                let removed_saving = dm[prev][first] + dm[last][next] - dm[prev][next];
                let mut best_gain: i32 = 0;
                let mut best_ins: Option<usize> = None;
                // Try inserting before position `ins` in the route WITHOUT the segment.
                // In the route-without-segment (length n - seg_len), new insert position
                // is in 1..=new_n-1 (between depot and last). We iterate over insert
                // positions referring to the original route, skipping s..=e.
                for ins in 1..n {
                    if ins >= s && ins <= e + 1 {
                        // inserting inside or right after the segment's removed spot — skip
                        continue;
                    }
                    // Determine the two neighbors `a` and `b` surrounding the insert slot
                    // in the segment-removed route. If ins == s+seg_len we'd be inserting
                    // at the segment's tail — but we already excluded e+1. For ins > e+1,
                    // a = route[ins-1], b = route[ins]. For ins <= s-1, same.
                    let a = route[ins - 1];
                    let b = if ins < n { route[ins] } else { continue };
                    // Skip inserting "where it came from" (neighbor both ends).
                    if (a == prev && b == next) {
                        continue;
                    }
                    let added_cost = dm[a][first] + dm[last][b] - dm[a][b];
                    let gain = removed_saving - added_cost;
                    if gain > best_gain {
                        // Build trial: remove segment and insert
                        let mut trial = Vec::with_capacity(n);
                        for (idx, &v) in route.iter().enumerate() {
                            if idx >= s && idx <= e {
                                continue;
                            }
                            trial.push(v);
                        }
                        // Find the new insertion index in `trial`: it's `ins` if ins < s,
                        // else `ins - seg_len` (because we removed seg_len elements before it).
                        let new_ins = if ins < s { ins } else { ins - seg_len };
                        for k in 0..seg_len {
                            trial.insert(new_ins + k, route[s + k]);
                        }
                        if is_time_feasible(&trial, dm, service_time, ready, due) {
                            best_gain = gain;
                            best_ins = Some(ins);
                        }
                    }
                    s = s; // no-op; keep `s` stable across inner loop
                }
                if let Some(ins) = best_ins {
                    let segment: Vec<usize> = route[s..=e].to_vec();
                    // Remove segment
                    route.drain(s..=e);
                    let new_ins = if ins < s { ins } else { ins - seg_len };
                    for (k, v) in segment.iter().enumerate() {
                        route.insert(new_ins + k, *v);
                    }
                    improved_any = true;
                    changed = true;
                    // restart outer loop since route reshaped
                    break;
                }
                s += 1;
            }
            if changed {
                break;
            }
        }
    }
    improved_any
}

fn two_opt_route(
    route: &mut Vec<usize>,
    dm: &[Vec<i32>],
    service_time: i32,
    ready: &[i32],
    due: &[i32],
) -> bool {
    let n = route.len();
    if n < 5 {
        return false;
    }
    let mut improved_any = false;
    let mut changed = true;
    while changed {
        changed = false;
        for i in 1..n - 2 {
            for j in i + 1..n - 1 {
                let a = route[i - 1];
                let b = route[i];
                let c = route[j];
                let d = route[j + 1];
                let old_edges = dm[a][b] + dm[c][d];
                let new_edges = dm[a][c] + dm[b][d];
                if new_edges < old_edges {
                    route[i..=j].reverse();
                    if is_time_feasible(route, dm, service_time, ready, due) {
                        changed = true;
                        improved_any = true;
                    } else {
                        route[i..=j].reverse();
                    }
                }
            }
        }
    }
    improved_any
}

fn or_opt_pass(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    ch: &Challenge,
    neighbors: &[Vec<usize>],
    start: &Instant,
    seg_len: usize,
) -> bool {
    let mut improved_any = false;
    let n_nodes = ch.num_nodes;

    // customer -> (route_idx, pos_in_route); rebuilt fully when a move reshapes routes.
    let mut cust_pos: Vec<Option<(usize, usize)>> = vec![None; n_nodes];
    let rebuild = |cust_pos: &mut Vec<Option<(usize, usize)>>, routes: &Vec<Vec<usize>>| {
        for entry in cust_pos.iter_mut() {
            *entry = None;
        }
        for (r, route) in routes.iter().enumerate() {
            for (p, &c) in route.iter().enumerate() {
                if p > 0 && p + 1 < route.len() {
                    cust_pos[c] = Some((r, p));
                }
            }
        }
    };
    rebuild(&mut cust_pos, routes);

    for from_r in 0..routes.len() {
        if start.elapsed().as_millis() > TIME_LIMIT_MS {
            break;
        }
        let mut pos = 1;
        while pos + seg_len < routes[from_r].len() {
            let seg_end = pos + seg_len - 1;
            let prev = routes[from_r][pos - 1];
            let next = routes[from_r][seg_end + 1];
            let seg_first = routes[from_r][pos];
            let seg_last = routes[from_r][seg_end];
            let seg_demand: i32 = routes[from_r][pos..=seg_end]
                .iter()
                .map(|&n| ch.demands[n])
                .sum();

            let removal_gain = ch.distance_matrix[prev][seg_first]
                + ch.distance_matrix[seg_last][next]
                - ch.distance_matrix[prev][next];

            let mut best: Option<(usize, usize, i32, bool)> = None;

            // Collect candidate (to_r, ins) slots from neighbors of the segment's
            // endpoints. For each neighbor n with position (to_r, n_pos), try inserting
            // the segment at ins=n_pos (neighbor becomes 'b') and ins=n_pos+1 (neighbor
            // becomes 'a'). This restricts the trial set from O(routes * route_len) to
            // roughly O(K) per endpoint.
            let endpoints: [usize; 2] = [seg_first, seg_last];
            for &endpoint in &endpoints {
                for &n in &neighbors[endpoint] {
                    let (to_r, n_pos) = match cust_pos[n] {
                        Some(p) => p,
                        None => continue,
                    };
                    if to_r == from_r {
                        continue;
                    }
                    if route_demand[to_r] + seg_demand > ch.max_capacity {
                        continue;
                    }
                    let to_len = routes[to_r].len();
                    for &ins in &[n_pos, n_pos + 1] {
                        if ins == 0 || ins >= to_len {
                            continue;
                        }
                        let a = routes[to_r][ins - 1];
                        let b = routes[to_r][ins];
                        for &(head, tail, reversed) in &[
                            (seg_first, seg_last, false),
                            (seg_last, seg_first, true),
                        ] {
                            if seg_len == 1 && reversed {
                                continue;
                            }
                            let ins_cost = ch.distance_matrix[a][head]
                                + ch.distance_matrix[tail][b]
                                - ch.distance_matrix[a][b];
                            let delta = ins_cost - removal_gain;
                            if delta < 0 && best.map_or(true, |(_, _, d, _)| delta < d) {
                                let mut trial =
                                    Vec::with_capacity(routes[to_r].len() + seg_len);
                                trial.extend_from_slice(&routes[to_r][..ins]);
                                if reversed {
                                    for k in (pos..=seg_end).rev() {
                                        trial.push(routes[from_r][k]);
                                    }
                                } else {
                                    trial.extend_from_slice(&routes[from_r][pos..=seg_end]);
                                }
                                trial.extend_from_slice(&routes[to_r][ins..]);
                                if is_time_feasible(
                                    &trial,
                                    &ch.distance_matrix,
                                    ch.service_time,
                                    &ch.ready_times,
                                    &ch.due_times,
                                ) {
                                    best = Some((to_r, ins, delta, reversed));
                                }
                            }
                        }
                    }
                }
            }

            if let Some((to_r, ins, _, reversed)) = best {
                let segment: Vec<usize> = if reversed {
                    routes[from_r][pos..=seg_end].iter().rev().copied().collect()
                } else {
                    routes[from_r][pos..=seg_end].to_vec()
                };
                for (k, &node) in segment.iter().enumerate() {
                    routes[to_r].insert(ins + k, node);
                }
                routes[from_r].drain(pos..=seg_end);
                route_demand[from_r] -= seg_demand;
                route_demand[to_r] += seg_demand;
                improved_any = true;
                rebuild(&mut cust_pos, routes);
            } else {
                pos += 1;
            }
        }
    }

    improved_any
}

fn exchange_pass(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    ch: &Challenge,
    start: &Instant,
) -> bool {
    let mut improved_any = false;
    let n_routes = routes.len();

    for r1 in 0..n_routes {
        if start.elapsed().as_millis() > TIME_LIMIT_MS {
            break;
        }
        for r2 in (r1 + 1)..n_routes {
            let mut p1 = 1;
            while p1 + 1 < routes[r1].len() {
                let c1 = routes[r1][p1];
                let a1 = routes[r1][p1 - 1];
                let b1 = routes[r1][p1 + 1];

                let mut best: Option<(usize, i32)> = None;
                for p2 in 1..routes[r2].len() - 1 {
                    let c2 = routes[r2][p2];
                    let a2 = routes[r2][p2 - 1];
                    let b2 = routes[r2][p2 + 1];

                    let new_d1 = route_demand[r1] - ch.demands[c1] + ch.demands[c2];
                    let new_d2 = route_demand[r2] - ch.demands[c2] + ch.demands[c1];
                    if new_d1 > ch.max_capacity || new_d2 > ch.max_capacity {
                        continue;
                    }

                    let old_edges = ch.distance_matrix[a1][c1]
                        + ch.distance_matrix[c1][b1]
                        + ch.distance_matrix[a2][c2]
                        + ch.distance_matrix[c2][b2];
                    let new_edges = ch.distance_matrix[a1][c2]
                        + ch.distance_matrix[c2][b1]
                        + ch.distance_matrix[a2][c1]
                        + ch.distance_matrix[c1][b2];
                    let delta = new_edges - old_edges;
                    if delta < 0 && best.map_or(true, |(_, d)| delta < d) {
                        let mut trial1 = routes[r1].clone();
                        trial1[p1] = c2;
                        let mut trial2 = routes[r2].clone();
                        trial2[p2] = c1;
                        if is_time_feasible(
                            &trial1,
                            &ch.distance_matrix,
                            ch.service_time,
                            &ch.ready_times,
                            &ch.due_times,
                        ) && is_time_feasible(
                            &trial2,
                            &ch.distance_matrix,
                            ch.service_time,
                            &ch.ready_times,
                            &ch.due_times,
                        ) {
                            best = Some((p2, delta));
                        }
                    }
                }

                if let Some((p2, _)) = best {
                    let c2 = routes[r2][p2];
                    routes[r1][p1] = c2;
                    routes[r2][p2] = c1;
                    route_demand[r1] = route_demand[r1] - ch.demands[c1] + ch.demands[c2];
                    route_demand[r2] = route_demand[r2] - ch.demands[c2] + ch.demands[c1];
                    improved_any = true;
                }
                p1 += 1;
            }
        }
    }

    improved_any
}

