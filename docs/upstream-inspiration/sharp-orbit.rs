use super::*;
use anyhow::Result;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde_json::{Map, Value};
use std::time::{Duration, Instant};

pub fn solve_challenge(
    challenge: &Challenge,
    save_solution: &dyn Fn(&Solution) -> Result<()>,
    _hyperparameters: &Option<Map<String, Value>>,
) -> Result<()> {
    let start = Instant::now();
    let budget = Duration::from_millis(28_000);

    let dm = &challenge.distance_matrix;
    let rt = &challenge.ready_times;
    let dt = &challenge.due_times;
    let st = challenge.service_time;
    let demands = &challenge.demands;
    let cap = challenge.max_capacity;

    let mut solution = super::solomon::run(challenge)?;
    save_solution(&solution)?;

    let mut route_demand = compute_route_demand(&solution.routes, demands);

    run_local_search(&mut solution, &mut route_demand, dm, rt, dt, st, demands, cap, &start, &budget);
    save_solution(&solution)?;

    let mut best_solution = solution.clone();
    let mut best_cost = solution_cost(&solution, dm);
    let mut best_demand = route_demand.clone();

    let mut rng = SmallRng::seed_from_u64(0xC0FFEE_u64 ^ challenge.num_nodes as u64);
    let total_customers = challenge.num_nodes.saturating_sub(1);
    let mut stagnation: usize = 0;

    let d_max = max_pairwise_distance(dm).max(1);
    let tw_max = dt[0].max(1);
    let demand_max = demands.iter().copied().max().unwrap_or(1).max(1);

    while start.elapsed() < budget {
        let pct = rng.gen_range(8..=25) as f32 / 100.0;
        let scale = 1.0 + (stagnation.min(20) as f32) * 0.05;
        let destroy_count = ((total_customers as f32 * pct * scale) as usize)
            .max(5)
            .min(120);
        let strat = rng.gen_range(0..3);
        match strat {
            0 => destroy_and_repair(
                &mut solution, &mut route_demand, destroy_count, &mut rng,
                dm, rt, dt, st, demands, cap,
            ),
            1 => related_destroy_and_repair(
                &mut solution, &mut route_demand, destroy_count, &mut rng,
                dm, rt, dt, st, demands, cap,
                d_max, tw_max, demand_max,
            ),
            _ => route_destroy_and_repair(
                &mut solution, &mut route_demand, &mut rng,
                dm, rt, dt, st, demands, cap,
            ),
        }

        run_local_search(&mut solution, &mut route_demand, dm, rt, dt, st, demands, cap, &start, &budget);

        let cost = solution_cost(&solution, dm);
        if cost < best_cost {
            best_cost = cost;
            best_solution = solution.clone();
            best_demand = route_demand.clone();
            save_solution(&best_solution)?;
            stagnation = 0;
        } else {
            solution = best_solution.clone();
            route_demand = best_demand.clone();
            stagnation += 1;
        }
    }

    save_solution(&best_solution)?;
    Ok(())
}

fn run_local_search(
    solution: &mut Solution,
    route_demand: &mut Vec<i32>,
    dm: &[Vec<i32>],
    rt: &[i32],
    dt: &[i32],
    st: i32,
    demands: &[i32],
    cap: i32,
    start: &Instant,
    budget: &Duration,
) {
    for r in solution.routes.iter_mut() {
        if start.elapsed() >= *budget {
            return;
        }
        two_opt_route(r, dm, rt, dt, st);
        or_opt_intra_route(r, dm, rt, dt, st);
    }

    loop {
        if start.elapsed() >= *budget {
            return;
        }
        let moved = or_opt_between_routes(
            &mut solution.routes, route_demand, dm, rt, dt, st, demands, cap, start, budget,
        );
        let swapped = exchange_between_routes(
            &mut solution.routes, route_demand, dm, rt, dt, st, demands, cap, start, budget,
        );
        let tail_swapped = two_opt_star_between_routes(
            &mut solution.routes, route_demand, dm, rt, dt, st, demands, cap, start, budget,
        );
        let _ = tail_swapped;
        let mut any_intra = false;
        for r in solution.routes.iter_mut() {
            if start.elapsed() >= *budget {
                return;
            }
            if two_opt_route(r, dm, rt, dt, st) {
                any_intra = true;
            }
            if or_opt_intra_route(r, dm, rt, dt, st) {
                any_intra = true;
            }
        }
        if !moved && !swapped && !tail_swapped && !any_intra {
            break;
        }
    }
}

fn or_opt_intra_route(
    route: &mut Vec<usize>,
    dm: &[Vec<i32>],
    rt: &[i32],
    dt: &[i32],
    st: i32,
) -> bool {
    let mut any_improved = false;
    let mut improved = true;
    while improved {
        improved = false;
        let n = route.len();
        if n < 5 {
            break;
        }
        'seg: for seg_len in 1..=3usize {
            if n < seg_len + 3 {
                continue;
            }
            for i in 1..n - seg_len {
                let prev = route[i - 1];
                let s0 = route[i];
                let s1 = route[i + seg_len - 1];
                let nxt = route[i + seg_len];
                let remove_gain = dm[prev][s0] + dm[s1][nxt] - dm[prev][nxt];
                if remove_gain <= 0 {
                    continue;
                }

                let n_prime = n - seg_len;
                let mut best: Option<(usize, i32, bool)> = None;
                for j_prime in 1..n_prime {
                    if j_prime == i {
                        continue;
                    }
                    let a = if j_prime - 1 < i {
                        route[j_prime - 1]
                    } else {
                        route[j_prime - 1 + seg_len]
                    };
                    let b = if j_prime < i {
                        route[j_prime]
                    } else {
                        route[j_prime + seg_len]
                    };
                    let old_edge = dm[a][b];

                    let add_fw = dm[a][s0] + dm[s1][b] - old_edge;
                    let d_fw = add_fw - remove_gain;
                    if d_fw < 0 && best.map(|(_, d, _)| d_fw < d).unwrap_or(true) {
                        best = Some((j_prime, d_fw, false));
                    }
                    if seg_len > 1 {
                        let add_rv = dm[a][s1] + dm[s0][b] - old_edge;
                        let d_rv = add_rv - remove_gain;
                        if d_rv < 0 && best.map(|(_, d, _)| d_rv < d).unwrap_or(true) {
                            best = Some((j_prime, d_rv, true));
                        }
                    }
                }

                if let Some((j_prime, _, rev)) = best {
                    let backup = route.clone();
                    let seg: Vec<usize> = route.drain(i..i + seg_len).collect();
                    let to_insert: Vec<usize> = if rev {
                        seg.iter().rev().copied().collect()
                    } else {
                        seg
                    };
                    for (k, v) in to_insert.into_iter().enumerate() {
                        route.insert(j_prime + k, v);
                    }
                    if route_tw_feasible(route, dm, rt, dt, st) {
                        improved = true;
                        any_improved = true;
                        break 'seg;
                    } else {
                        *route = backup;
                    }
                }
            }
        }
    }
    any_improved
}

fn two_opt_star_between_routes(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    dm: &[Vec<i32>],
    rt: &[i32],
    dt: &[i32],
    st: i32,
    demands: &[i32],
    cap: i32,
    start: &Instant,
    budget: &Duration,
) -> bool {
    let mut any_improved = false;
    let mut improved = true;
    while improved {
        improved = false;
        if start.elapsed() >= *budget {
            break;
        }
        let nroutes = routes.len();
        'outer: for r1 in 0..nroutes {
            if r1 >= routes.len() {
                break;
            }
            for r2 in (r1 + 1)..nroutes {
                if r2 >= routes.len() {
                    continue;
                }
                let len1 = routes[r1].len();
                let len2 = routes[r2].len();
                if len1 < 2 || len2 < 2 {
                    continue;
                }
                for i in 0..len1 - 1 {
                    if start.elapsed() >= *budget {
                        break 'outer;
                    }
                    for j in 0..len2 - 1 {
                        let a = routes[r1][i];
                        let b = routes[r1][i + 1];
                        let c = routes[r2][j];
                        let d = routes[r2][j + 1];
                        let delta = dm[a][d] + dm[c][b] - dm[a][b] - dm[c][d];
                        if delta >= 0 {
                            continue;
                        }

                        let mut new_r1_demand = 0i32;
                        for k in 1..=i {
                            new_r1_demand += demands[routes[r1][k]];
                        }
                        for k in (j + 1)..len2.saturating_sub(1) {
                            new_r1_demand += demands[routes[r2][k]];
                        }
                        if new_r1_demand > cap {
                            continue;
                        }
                        let mut new_r2_demand = 0i32;
                        for k in 1..=j {
                            new_r2_demand += demands[routes[r2][k]];
                        }
                        for k in (i + 1)..len1.saturating_sub(1) {
                            new_r2_demand += demands[routes[r1][k]];
                        }
                        if new_r2_demand > cap {
                            continue;
                        }

                        let mut new_r1: Vec<usize> = routes[r1][..=i].to_vec();
                        new_r1.extend(routes[r2][(j + 1)..].iter().copied());
                        let mut new_r2: Vec<usize> = routes[r2][..=j].to_vec();
                        new_r2.extend(routes[r1][(i + 1)..].iter().copied());

                        let r1_ok = new_r1.len() <= 2 || route_tw_feasible(&new_r1, dm, rt, dt, st);
                        let r2_ok = new_r2.len() <= 2 || route_tw_feasible(&new_r2, dm, rt, dt, st);
                        if !(r1_ok && r2_ok) {
                            continue;
                        }
                        if new_r1.len() <= 2 && new_r2.len() <= 2 {
                            continue;
                        }

                        routes[r1] = new_r1;
                        routes[r2] = new_r2;
                        route_demand[r1] = new_r1_demand;
                        route_demand[r2] = new_r2_demand;
                        any_improved = true;
                        improved = true;

                        let mut k = 0;
                        while k < routes.len() {
                            if routes[k].len() <= 2 {
                                routes.remove(k);
                                route_demand.remove(k);
                            } else {
                                k += 1;
                            }
                        }
                        continue 'outer;
                    }
                }
            }
        }
    }
    any_improved
}

fn destroy_and_repair(
    solution: &mut Solution,
    route_demand: &mut Vec<i32>,
    destroy_count: usize,
    rng: &mut SmallRng,
    dm: &[Vec<i32>],
    rt: &[i32],
    dt: &[i32],
    st: i32,
    demands: &[i32],
    cap: i32,
) {
    let mut all_cust: Vec<(usize, usize)> = Vec::new();
    for (ri, r) in solution.routes.iter().enumerate() {
        for pi in 1..r.len() - 1 {
            all_cust.push((ri, pi));
        }
    }
    all_cust.shuffle(rng);
    let actual = destroy_count.min(all_cust.len());
    let chosen: Vec<(usize, usize)> = all_cust.into_iter().take(actual).collect();

    let mut removed_nodes: Vec<usize> = Vec::with_capacity(actual);
    let mut by_route: Vec<Vec<usize>> = vec![Vec::new(); solution.routes.len()];
    for (ri, pi) in &chosen {
        by_route[*ri].push(*pi);
    }
    for (ri, positions) in by_route.iter_mut().enumerate() {
        positions.sort_unstable_by(|a, b| b.cmp(a));
        for &p in positions.iter() {
            let node = solution.routes[ri].remove(p);
            route_demand[ri] -= demands[node];
            removed_nodes.push(node);
        }
    }

    let mut ri = 0;
    while ri < solution.routes.len() {
        if solution.routes[ri].len() <= 2 {
            solution.routes.remove(ri);
            route_demand.remove(ri);
        } else {
            ri += 1;
        }
    }

    removed_nodes.shuffle(rng);
    for node in removed_nodes {
        let d_node = demands[node];
        let mut best: Option<(usize, usize, i32)> = None;
        for (ri, r) in solution.routes.iter().enumerate() {
            if route_demand[ri] + d_node > cap {
                continue;
            }
            for p in 1..r.len() {
                let a = r[p - 1];
                let b = r[p];
                let cost = dm[a][node] + dm[node][b] - dm[a][b];
                if best.map(|(_, _, c)| cost < c).unwrap_or(true) {
                    if trial_insert_tw_feasible(r, p, node, dm, rt, dt, st) {
                        best = Some((ri, p, cost));
                    }
                }
            }
        }
        if let Some((ri, p, _)) = best {
            solution.routes[ri].insert(p, node);
            route_demand[ri] += d_node;
        } else {
            solution.routes.push(vec![0, node, 0]);
            route_demand.push(d_node);
        }
    }
}

fn max_pairwise_distance(dm: &[Vec<i32>]) -> i32 {
    let mut m = 0;
    for row in dm {
        for &v in row {
            if v > m {
                m = v;
            }
        }
    }
    m
}

fn related_destroy_and_repair(
    solution: &mut Solution,
    route_demand: &mut Vec<i32>,
    destroy_count: usize,
    rng: &mut SmallRng,
    dm: &[Vec<i32>],
    rt: &[i32],
    dt: &[i32],
    st: i32,
    demands: &[i32],
    cap: i32,
    d_max: i32,
    tw_max: i32,
    demand_max: i32,
) {
    let mut all_cust: Vec<(usize, usize, usize)> = Vec::new();
    for (ri, r) in solution.routes.iter().enumerate() {
        for pi in 1..r.len() - 1 {
            all_cust.push((ri, pi, r[pi]));
        }
    }
    if all_cust.is_empty() {
        return;
    }

    let seed_idx = rng.gen_range(0..all_cust.len());
    let mut removed_nodes: Vec<usize> = Vec::with_capacity(destroy_count);
    let mut removed_set: Vec<(usize, usize)> = Vec::with_capacity(destroy_count);
    let (sri, spi, snode) = all_cust.swap_remove(seed_idx);
    removed_nodes.push(snode);
    removed_set.push((sri, spi));
    let seed_node = snode;

    while removed_nodes.len() < destroy_count && !all_cust.is_empty() {
        let mut scored: Vec<(f32, usize)> = all_cust
            .iter()
            .enumerate()
            .map(|(idx, &(_, _, node))| {
                let d_part = dm[seed_node][node].max(dm[node][seed_node]) as f32 / d_max as f32;
                let tw_part = (dt[seed_node] - dt[node]).abs() as f32 / tw_max as f32
                    + (rt[seed_node] - rt[node]).abs() as f32 / tw_max as f32;
                let dem_part =
                    (demands[seed_node] - demands[node]).abs() as f32 / demand_max as f32;
                let rel = 9.0 * d_part + 3.0 * tw_part + 2.0 * dem_part;
                (rel, idx)
            })
            .collect();
        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let n = scored.len();
        let u: f32 = rng.gen();
        let rank = (u.powi(6) * n as f32).floor() as usize;
        let rank = rank.min(n - 1);
        let (_, pick_idx) = scored[rank];
        let (ri, pi, node) = all_cust.swap_remove(pick_idx);
        removed_nodes.push(node);
        removed_set.push((ri, pi));
    }

    let mut by_route: Vec<Vec<usize>> = vec![Vec::new(); solution.routes.len()];
    for (ri, pi) in &removed_set {
        by_route[*ri].push(*pi);
    }
    for (ri, positions) in by_route.iter_mut().enumerate() {
        positions.sort_unstable_by(|a, b| b.cmp(a));
        for &p in positions.iter() {
            let node = solution.routes[ri].remove(p);
            route_demand[ri] -= demands[node];
            debug_assert!(removed_nodes.contains(&node));
        }
    }

    let mut ri = 0;
    while ri < solution.routes.len() {
        if solution.routes[ri].len() <= 2 {
            solution.routes.remove(ri);
            route_demand.remove(ri);
        } else {
            ri += 1;
        }
    }

    removed_nodes.shuffle(rng);
    for node in removed_nodes {
        let d_node = demands[node];
        let mut best: Option<(usize, usize, i32)> = None;
        for (ri, r) in solution.routes.iter().enumerate() {
            if route_demand[ri] + d_node > cap {
                continue;
            }
            for p in 1..r.len() {
                let a = r[p - 1];
                let b = r[p];
                let cost = dm[a][node] + dm[node][b] - dm[a][b];
                if best.map(|(_, _, c)| cost < c).unwrap_or(true) {
                    if trial_insert_tw_feasible(r, p, node, dm, rt, dt, st) {
                        best = Some((ri, p, cost));
                    }
                }
            }
        }
        if let Some((ri, p, _)) = best {
            solution.routes[ri].insert(p, node);
            route_demand[ri] += d_node;
        } else {
            solution.routes.push(vec![0, node, 0]);
            route_demand.push(d_node);
        }
    }
}

fn route_destroy_and_repair(
    solution: &mut Solution,
    route_demand: &mut Vec<i32>,
    rng: &mut SmallRng,
    dm: &[Vec<i32>],
    rt: &[i32],
    dt: &[i32],
    st: i32,
    demands: &[i32],
    cap: i32,
) {
    if solution.routes.len() < 2 {
        return;
    }
    let mut idx_by_size: Vec<usize> = (0..solution.routes.len()).collect();
    idx_by_size.sort_by_key(|&i| solution.routes[i].len());
    let pool = (idx_by_size.len() / 3).max(1);
    let pick = rng.gen_range(0..pool);
    let ri = idx_by_size[pick];

    let removed: Vec<usize> = solution.routes[ri][1..solution.routes[ri].len() - 1].to_vec();
    solution.routes.remove(ri);
    route_demand.remove(ri);

    let mut removed = removed;
    removed.shuffle(rng);
    for node in removed {
        let d_node = demands[node];
        let mut best: Option<(usize, usize, i32)> = None;
        for (idx, r) in solution.routes.iter().enumerate() {
            if route_demand[idx] + d_node > cap {
                continue;
            }
            for p in 1..r.len() {
                let a = r[p - 1];
                let b = r[p];
                let cost = dm[a][node] + dm[node][b] - dm[a][b];
                if best.map(|(_, _, c)| cost < c).unwrap_or(true) {
                    if trial_insert_tw_feasible(r, p, node, dm, rt, dt, st) {
                        best = Some((idx, p, cost));
                    }
                }
            }
        }
        if let Some((idx, p, _)) = best {
            solution.routes[idx].insert(p, node);
            route_demand[idx] += d_node;
        } else {
            solution.routes.push(vec![0, node, 0]);
            route_demand.push(d_node);
        }
    }
}

fn compute_route_demand(routes: &[Vec<usize>], demands: &[i32]) -> Vec<i32> {
    routes
        .iter()
        .map(|r| r[1..r.len() - 1].iter().map(|&n| demands[n]).sum())
        .collect()
}

fn solution_cost(solution: &Solution, dm: &[Vec<i32>]) -> i32 {
    let mut total = 0;
    for r in &solution.routes {
        for w in r.windows(2) {
            total += dm[w[0]][w[1]];
        }
    }
    total
}

fn two_opt_route(route: &mut Vec<usize>, dm: &[Vec<i32>], rt: &[i32], dt: &[i32], st: i32) -> bool {
    let n = route.len();
    if n < 5 {
        return false;
    }
    let mut any_improved = false;
    let mut improved = true;
    while improved {
        improved = false;
        for i in 1..n - 2 {
            for j in i + 1..n - 1 {
                let a = route[i - 1];
                let b = route[i];
                let c = route[j];
                let d = route[j + 1];
                let delta = dm[a][c] + dm[b][d] - dm[a][b] - dm[c][d];
                if delta < 0 {
                    route[i..=j].reverse();
                    if route_tw_feasible(route, dm, rt, dt, st) {
                        improved = true;
                        any_improved = true;
                    } else {
                        route[i..=j].reverse();
                    }
                }
            }
        }
    }
    any_improved
}

fn or_opt_between_routes(
    routes: &mut Vec<Vec<usize>>,
    route_demand: &mut Vec<i32>,
    dm: &[Vec<i32>],
    rt: &[i32],
    dt: &[i32],
    st: i32,
    demands: &[i32],
    cap: i32,
    start: &Instant,
    budget: &Duration,
) -> bool {
    let mut any_improved = false;
    let mut improved = true;
    while improved {
        improved = false;
        if start.elapsed() >= *budget {
            break;
        }

        let mut src = 0;
        'outer: while src < routes.len() {
            for seg_len in 1..=3usize {
                let mut i = 1usize;
                loop {
                    if start.elapsed() >= *budget {
                        break 'outer;
                    }
                    if i + seg_len + 1 > routes[src].len() {
                        break;
                    }
                    let s0 = routes[src][i];
                    let s1 = routes[src][i + seg_len - 1];
                    let prev = routes[src][i - 1];
                    let next = routes[src][i + seg_len];
                    let remove_gain = dm[prev][s0] + dm[s1][next] - dm[prev][next];
                    if remove_gain <= 0 {
                        i += 1;
                        continue;
                    }
                    let seg_demand: i32 =
                        routes[src][i..i + seg_len].iter().map(|&n| demands[n]).sum();
                    let chain: Vec<usize> = routes[src][i..i + seg_len].to_vec();

                    let mut best: Option<(usize, usize, i32, bool)> = None;
                    for dst in 0..routes.len() {
                        if dst == src {
                            continue;
                        }
                        if route_demand[dst] + seg_demand > cap {
                            continue;
                        }
                        let dr_len = routes[dst].len();
                        for p in 1..dr_len {
                            let a = routes[dst][p - 1];
                            let b = routes[dst][p];
                            let old_edge = dm[a][b];

                            let add_fw = dm[a][s0] + dm[s1][b] - old_edge;
                            let d_fw = add_fw - remove_gain;
                            if d_fw < 0 && best.map(|(_, _, d, _)| d_fw < d).unwrap_or(true) {
                                if chain_insert_tw_feasible(
                                    &routes[dst], p, &chain, dm, rt, dt, st, false,
                                ) {
                                    best = Some((dst, p, d_fw, false));
                                }
                            }
                            if seg_len > 1 {
                                let add_rv = dm[a][s1] + dm[s0][b] - old_edge;
                                let d_rv = add_rv - remove_gain;
                                if d_rv < 0 && best.map(|(_, _, d, _)| d_rv < d).unwrap_or(true) {
                                    if chain_insert_tw_feasible(
                                        &routes[dst], p, &chain, dm, rt, dt, st, true,
                                    ) {
                                        best = Some((dst, p, d_rv, true));
                                    }
                                }
                            }
                        }
                    }

                    if let Some((dst, p, _, rev)) = best {
                        let seg: Vec<usize> = routes[src].drain(i..i + seg_len).collect();
                        route_demand[src] -= seg_demand;
                        route_demand[dst] += seg_demand;
                        let to_insert: Vec<usize> = if rev {
                            seg.iter().rev().copied().collect()
                        } else {
                            seg
                        };
                        for (k, v) in to_insert.into_iter().enumerate() {
                            routes[dst].insert(p + k, v);
                        }
                        any_improved = true;
                        improved = true;

                        if routes[src].len() <= 2 {
                            routes.remove(src);
                            route_demand.remove(src);
                            continue 'outer;
                        }
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
            src += 1;
        }
    }
    any_improved
}

fn exchange_between_routes(
    routes: &mut [Vec<usize>],
    route_demand: &mut [i32],
    dm: &[Vec<i32>],
    rt: &[i32],
    dt: &[i32],
    st: i32,
    demands: &[i32],
    cap: i32,
    start: &Instant,
    budget: &Duration,
) -> bool {
    let mut any_improved = false;
    let mut improved = true;
    while improved {
        improved = false;
        if start.elapsed() >= *budget {
            break;
        }

        let nroutes = routes.len();
        'outer: for r1 in 0..nroutes {
            for r2 in (r1 + 1)..nroutes {
                let mut i = 1usize;
                while i + 1 < routes[r1].len() {
                    if start.elapsed() >= *budget {
                        break 'outer;
                    }
                    let a = routes[r1][i - 1];
                    let x = routes[r1][i];
                    let b = routes[r1][i + 1];
                    let mut j = 1usize;
                    let mut advanced_i = false;
                    while j + 1 < routes[r2].len() {
                        let c = routes[r2][j - 1];
                        let y = routes[r2][j];
                        let d = routes[r2][j + 1];

                        let dx = demands[x];
                        let dy = demands[y];
                        if route_demand[r1] - dx + dy > cap {
                            j += 1;
                            continue;
                        }
                        if route_demand[r2] - dy + dx > cap {
                            j += 1;
                            continue;
                        }

                        let delta = dm[a][y] + dm[y][b] + dm[c][x] + dm[x][d]
                            - dm[a][x] - dm[x][b] - dm[c][y] - dm[y][d];
                        if delta >= 0 {
                            j += 1;
                            continue;
                        }

                        routes[r1][i] = y;
                        routes[r2][j] = x;
                        if route_tw_feasible(&routes[r1], dm, rt, dt, st)
                            && route_tw_feasible(&routes[r2], dm, rt, dt, st)
                        {
                            route_demand[r1] = route_demand[r1] - dx + dy;
                            route_demand[r2] = route_demand[r2] - dy + dx;
                            any_improved = true;
                            improved = true;
                            i += 1;
                            advanced_i = true;
                            break;
                        } else {
                            routes[r1][i] = x;
                            routes[r2][j] = y;
                            j += 1;
                        }
                    }
                    if !advanced_i {
                        i += 1;
                    }
                }
            }
        }
    }
    any_improved
}

fn route_tw_feasible(route: &[usize], dm: &[Vec<i32>], rt: &[i32], dt: &[i32], st: i32) -> bool {
    if route.len() < 3 {
        return false;
    }
    let mut t: i32 = 0;
    let mut prev = 0usize;
    for &node in &route[1..route.len() - 1] {
        t += dm[prev][node];
        if t > dt[node] {
            return false;
        }
        if t < rt[node] {
            t = rt[node];
        }
        t += st;
        prev = node;
    }
    t += dm[prev][0];
    if t > dt[0] {
        return false;
    }
    true
}

fn chain_insert_tw_feasible(
    route: &[usize],
    pos: usize,
    chain: &[usize],
    dm: &[Vec<i32>],
    rt: &[i32],
    dt: &[i32],
    st: i32,
    reverse: bool,
) -> bool {
    let n = route.len();
    let mut t: i32 = 0;
    let mut prev = 0usize;

    for i in 1..pos {
        let cur = route[i];
        t += dm[prev][cur];
        if t > dt[cur] {
            return false;
        }
        if t < rt[cur] {
            t = rt[cur];
        }
        t += st;
        prev = cur;
    }

    let chain_len = chain.len();
    for k in 0..chain_len {
        let cur = if reverse { chain[chain_len - 1 - k] } else { chain[k] };
        t += dm[prev][cur];
        if t > dt[cur] {
            return false;
        }
        if t < rt[cur] {
            t = rt[cur];
        }
        t += st;
        prev = cur;
    }

    for i in pos..n {
        let cur = route[i];
        t += dm[prev][cur];
        if i == n - 1 {
            if t > dt[0] {
                return false;
            }
        } else {
            if t > dt[cur] {
                return false;
            }
            if t < rt[cur] {
                t = rt[cur];
            }
            t += st;
        }
        prev = cur;
    }
    true
}

fn trial_insert_tw_feasible(
    route: &[usize],
    pos: usize,
    node: usize,
    dm: &[Vec<i32>],
    rt: &[i32],
    dt: &[i32],
    st: i32,
) -> bool {
    chain_insert_tw_feasible(route, pos, &[node], dm, rt, dt, st, false)
}

