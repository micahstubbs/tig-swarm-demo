use super::*;
use anyhow::Result;
use serde_json::{Map, Value};
use std::time::Instant;

const K_NEIGHBORS: usize = 40;
const TIME_BUDGET_MS: u128 = 29_000;

fn xorshift64(s: &mut u64) -> u64 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *s = x;
    x
}

struct Ctx {
    n: usize,
    d: Vec<Vec<i32>>,
    demand: Vec<i32>,
    ready: Vec<i32>,
    due: Vec<i32>,
    srv: i32,
    cap: i32,
    fleet: usize,
    neighbors: Vec<Vec<u32>>,
    depot_due: i32,
}

impl Ctx {
    fn build(ch: &Challenge) -> Self {
        let n = ch.num_nodes;
        let k = K_NEIGHBORS.min(n.saturating_sub(2));
        let mut neighbors = vec![Vec::with_capacity(k); n];
        for i in 0..n {
            let mut buf: Vec<(i32, u32)> = (1..n).filter(|&j| j != i)
                .map(|j| (ch.distance_matrix[i][j], j as u32)).collect();
            buf.sort_by_key(|x| x.0);
            neighbors[i] = buf.iter().take(k).map(|x| x.1).collect();
        }
        Self {
            n, d: ch.distance_matrix.clone(), demand: ch.demands.clone(),
            ready: ch.ready_times.clone(), due: ch.due_times.clone(),
            srv: ch.service_time, cap: ch.max_capacity, fleet: ch.fleet_size,
            neighbors, depot_due: ch.due_times[0],
        }
    }
    #[inline]
    fn service_of(&self, node: usize) -> i32 { if node == 0 { 0 } else { self.srv } }
}

#[derive(Clone)]
struct Route {
    seq: Vec<u32>,
    arr: Vec<i32>,
    lat: Vec<i32>,
    demand_sum: i32,
    dist: i32,
}

impl Route {
    fn empty(ctx: &Ctx) -> Self {
        Self { seq: vec![0, 0], arr: vec![0, 0], lat: vec![ctx.depot_due, ctx.depot_due], demand_sum: 0, dist: 0 }
    }
    #[inline] fn len(&self) -> usize { self.seq.len() }
    #[inline] fn num_customers(&self) -> usize { self.seq.len().saturating_sub(2) }

    fn recompute(&mut self, ctx: &Ctx) {
        let n = self.seq.len();
        self.arr.resize(n, 0);
        self.lat.resize(n, 0);
        self.arr[0] = 0;
        let mut dsum = 0i32;
        let mut demsum = 0i32;
        for p in 1..n {
            let prev = self.seq[p-1] as usize;
            let cur = self.seq[p] as usize;
            let dep = self.arr[p-1].max(ctx.ready[prev]) + ctx.service_of(prev);
            self.arr[p] = dep + ctx.d[prev][cur];
            dsum += ctx.d[prev][cur];
            demsum += ctx.demand[cur];
        }
        self.dist = dsum;
        self.demand_sum = demsum;
        self.lat[n-1] = ctx.depot_due;
        for p in (0..n-1).rev() {
            let cur = self.seq[p] as usize;
            let nxt = self.seq[p+1] as usize;
            let bound = self.lat[p+1] - ctx.service_of(cur) - ctx.d[cur][nxt];
            self.lat[p] = ctx.due[cur].min(bound);
        }
    }

    #[inline]
    fn try_insert(&self, ctx: &Ctx, u: usize, p: usize) -> Option<i32> {
        if self.demand_sum + ctx.demand[u] > ctx.cap { return None; }
        let a = self.seq[p-1] as usize;
        let b = self.seq[p] as usize;
        let dep_a = self.arr[p-1].max(ctx.ready[a]) + ctx.service_of(a);
        let arr_u = dep_a + ctx.d[a][u];
        if arr_u > ctx.due[u] { return None; }
        let dep_u = arr_u.max(ctx.ready[u]) + ctx.srv;
        let arr_b = dep_u + ctx.d[u][b];
        if arr_b > self.lat[p] { return None; }
        Some(ctx.d[a][u] + ctx.d[u][b] - ctx.d[a][b])
    }

    fn insert_at(&mut self, ctx: &Ctx, u: usize, p: usize) {
        self.seq.insert(p, u as u32);
        self.recompute(ctx);
    }

    fn remove_at(&mut self, ctx: &Ctx, p: usize) -> usize {
        let node = self.seq.remove(p) as usize;
        self.recompute(ctx);
        node
    }

    fn is_feasible(&self, ctx: &Ctx) -> bool {
        let n = self.seq.len();
        for p in 1..n {
            if self.arr[p] > ctx.due[self.seq[p] as usize] { return false; }
        }
        true
    }
}

#[derive(Clone)]
struct Sol {
    routes: Vec<Route>,
    total_dist: i32,
}

impl Sol {
    fn recompute_total(&mut self) { self.total_dist = self.routes.iter().map(|r| r.dist).sum(); }
    fn to_solution(&self) -> Solution {
        Solution {
            routes: self.routes.iter().filter(|r| r.num_customers() > 0)
                .map(|r| r.seq.iter().map(|&x| x as usize).collect()).collect(),
        }
    }
    fn cleanup_empty(&mut self) { self.routes.retain(|r| r.num_customers() > 0); }
}

#[derive(Clone)]
struct NodeLoc { route: u32, pos: u32 }

fn build_locator(ctx: &Ctx, sol: &Sol) -> Vec<NodeLoc> {
    let mut loc = vec![NodeLoc { route: u32::MAX, pos: u32::MAX }; ctx.n];
    for (ri, r) in sol.routes.iter().enumerate() {
        for (p, &u) in r.seq.iter().enumerate() {
            if u != 0 { loc[u as usize] = NodeLoc { route: ri as u32, pos: p as u32 }; }
        }
    }
    loc
}

fn rebuild_locator(ctx: &Ctx, sol: &Sol, loc: &mut Vec<NodeLoc>) {
    for x in loc.iter_mut() { x.route = u32::MAX; x.pos = u32::MAX; }
    for (ri, r) in sol.routes.iter().enumerate() {
        for (p, &u) in r.seq.iter().enumerate() {
            if u != 0 { loc[u as usize] = NodeLoc { route: ri as u32, pos: p as u32 }; }
        }
    }
}

fn refresh_route_loc(loc: &mut [NodeLoc], sol: &Sol, ri: usize) {
    for (p, &u) in sol.routes[ri].seq.iter().enumerate() {
        if u != 0 { loc[u as usize] = NodeLoc { route: ri as u32, pos: p as u32 }; }
    }
}

// ─── Local Search Operators ───

fn relocate_pass(ctx: &Ctx, sol: &mut Sol, loc: &mut Vec<NodeLoc>) -> bool {
    let mut improved = false;
    for u in 1..ctx.n as u32 {
        let ul = &loc[u as usize];
        if ul.route == u32::MAX { continue; }
        let src_ri = ul.route as usize;
        let src_p = ul.pos as usize;
        let r = &sol.routes[src_ri];
        let a = r.seq[src_p-1] as usize;
        let b = r.seq[src_p+1] as usize;
        let remove_gain = ctx.d[a][u as usize] + ctx.d[u as usize][b] - ctx.d[a][b];

        let mut best_delta = 0i32;
        let mut best_tri = usize::MAX;
        let mut best_tp = 0usize;

        for &v in &ctx.neighbors[u as usize] {
            let vl = &loc[v as usize];
            if vl.route == u32::MAX { continue; }
            let tri = vl.route as usize;
            let vp = vl.pos as usize;
            for &p in &[vp, vp + 1] {
                if tri == src_ri && (p == src_p || p == src_p + 1) { continue; }
                let target = &sol.routes[tri];
                if p < 1 || p >= target.len() { continue; }
                if tri == src_ri {
                    let mut tmp = sol.routes[src_ri].clone();
                    let removed = tmp.remove_at(ctx, src_p);
                    let adj_p = if p > src_p { p - 1 } else { p };
                    if adj_p < 1 || adj_p >= tmp.len() { continue; }
                    if let Some(ins_delta) = tmp.try_insert(ctx, removed, adj_p) {
                        let total = ins_delta - remove_gain;
                        if total < best_delta { best_delta = total; best_tri = tri; best_tp = p; }
                    }
                } else {
                    if let Some(ins_delta) = target.try_insert(ctx, u as usize, p) {
                        let total = ins_delta - remove_gain;
                        if total < best_delta { best_delta = total; best_tri = tri; best_tp = p; }
                    }
                }
            }
        }

        if best_tri != usize::MAX && best_delta < 0 {
            if best_tri == src_ri {
                let removed = sol.routes[src_ri].remove_at(ctx, src_p);
                let adj_p = if best_tp > src_p { best_tp - 1 } else { best_tp };
                sol.routes[src_ri].insert_at(ctx, removed, adj_p);
                refresh_route_loc(loc, sol, src_ri);
            } else {
                let removed = sol.routes[src_ri].remove_at(ctx, src_p);
                sol.routes[best_tri].insert_at(ctx, removed, best_tp);
                refresh_route_loc(loc, sol, src_ri);
                refresh_route_loc(loc, sol, best_tri);
            }
            improved = true;
        }
    }
    improved
}

fn exchange_pass(ctx: &Ctx, sol: &mut Sol, loc: &mut Vec<NodeLoc>) -> bool {
    let mut improved = false;
    for u in 1..ctx.n as u32 {
        let ul = loc[u as usize].clone();
        if ul.route == u32::MAX { continue; }
        let r1i = ul.route as usize;
        let p1 = ul.pos as usize;
        let mut best_delta = 0i32;
        let mut best_v = 0u32;

        for &v in &ctx.neighbors[u as usize] {
            let vl = &loc[v as usize];
            if vl.route == u32::MAX { continue; }
            let r2i = vl.route as usize;
            if r1i == r2i { continue; }
            let p2 = vl.pos as usize;
            let r1 = &sol.routes[r1i];
            let r2 = &sol.routes[r2i];
            if r1.demand_sum - ctx.demand[u as usize] + ctx.demand[v as usize] > ctx.cap { continue; }
            if r2.demand_sum - ctx.demand[v as usize] + ctx.demand[u as usize] > ctx.cap { continue; }
            let old = ctx.d[r1.seq[p1-1] as usize][u as usize] + ctx.d[u as usize][r1.seq[p1+1] as usize]
                + ctx.d[r2.seq[p2-1] as usize][v as usize] + ctx.d[v as usize][r2.seq[p2+1] as usize];
            let new_c = ctx.d[r1.seq[p1-1] as usize][v as usize] + ctx.d[v as usize][r1.seq[p1+1] as usize]
                + ctx.d[r2.seq[p2-1] as usize][u as usize] + ctx.d[u as usize][r2.seq[p2+1] as usize];
            if new_c - old < best_delta { best_delta = new_c - old; best_v = v; }
        }

        if best_v != 0 && best_delta < 0 {
            let vl = loc[best_v as usize].clone();
            let r2i = vl.route as usize;
            let p2 = vl.pos as usize;
            sol.routes[r1i].seq[p1] = best_v;
            sol.routes[r2i].seq[p2] = u;
            sol.routes[r1i].recompute(ctx);
            sol.routes[r2i].recompute(ctx);
            // Verify feasibility
            let ok = sol.routes[r1i].is_feasible(ctx)
                && sol.routes[r2i].is_feasible(ctx);
            if ok {
                refresh_route_loc(loc, sol, r1i);
                refresh_route_loc(loc, sol, r2i);
                improved = true;
            } else {
                sol.routes[r1i].seq[p1] = u;
                sol.routes[r2i].seq[p2] = best_v;
                sol.routes[r1i].recompute(ctx);
                sol.routes[r2i].recompute(ctx);
            }
        }
    }
    improved
}

fn two_opt_intra_pass(ctx: &Ctx, sol: &mut Sol) -> bool {
    let mut improved = false;
    for ri in 0..sol.routes.len() {
        loop {
            let mut best_delta = 0;
            let mut best_i = 0;
            let mut best_j = 0;
            let r = &sol.routes[ri];
            let len = r.len();
            if len < 5 { break; }
            for i in 0..len-2 {
                let a = r.seq[i] as usize;
                for j in i+2..len-1 {
                    let c = r.seq[j] as usize;
                    let b = r.seq[i+1] as usize;
                    let dn = r.seq[j+1] as usize;
                    let delta = ctx.d[a][c] + ctx.d[b][dn] - ctx.d[a][b] - ctx.d[c][dn];
                    if delta >= best_delta { continue; }
                    if is_two_opt_feasible(ctx, r, i, j) {
                        best_delta = delta; best_i = i; best_j = j;
                    }
                }
            }
            if best_delta < 0 {
                sol.routes[ri].seq[best_i+1..=best_j].reverse();
                sol.routes[ri].recompute(ctx);
                improved = true;
            } else { break; }
        }
    }
    improved
}

fn is_two_opt_feasible(ctx: &Ctx, r: &Route, i: usize, j: usize) -> bool {
    let a = r.seq[i] as usize;
    let dep_a = r.arr[i].max(ctx.ready[a]) + ctx.service_of(a);
    let mut cur = a;
    let mut t = dep_a;
    for k in (i+1..=j).rev() {
        let nxt = r.seq[k] as usize;
        t += ctx.d[cur][nxt];
        if t > ctx.due[nxt] { return false; }
        t = t.max(ctx.ready[nxt]) + ctx.srv;
        cur = nxt;
    }
    t += ctx.d[cur][r.seq[j+1] as usize];
    t <= r.lat[j+1]
}

fn or_opt_pass(ctx: &Ctx, sol: &mut Sol, loc: &mut Vec<NodeLoc>) -> bool {
    let mut improved = false;
    for u in 1..ctx.n as u32 {
        let ul = loc[u as usize].clone();
        if ul.route == u32::MAX { continue; }
        let src_ri = ul.route as usize;
        let src_p = ul.pos as usize;
        let src_len = sol.routes[src_ri].len();
        for seg_len in 1..=3usize {
            if src_p + seg_len + 1 > src_len { break; }
            let seg_front = sol.routes[src_ri].seq[src_p] as usize;
            let seg_back = sol.routes[src_ri].seq[src_p + seg_len - 1] as usize;
            let a = sol.routes[src_ri].seq[src_p-1] as usize;
            let b = sol.routes[src_ri].seq[src_p + seg_len] as usize;
            let remove_gain = ctx.d[a][seg_front] + ctx.d[seg_back][b] - ctx.d[a][b];
            let seg_demand: i32 = (src_p..src_p+seg_len).map(|k| ctx.demand[sol.routes[src_ri].seq[k] as usize]).sum();
            let mut best_delta = 0i32;
            let mut best_tri = usize::MAX;
            let mut best_tp = 0usize;
            for &v in &ctx.neighbors[seg_front] {
                let vl = &loc[v as usize];
                if vl.route == u32::MAX { continue; }
                let tri = vl.route as usize;
                let vp = vl.pos as usize;
                for &p in &[vp, vp+1] {
                    if tri == src_ri && p >= src_p && p <= src_p + seg_len { continue; }
                    let target = &sol.routes[tri];
                    if p < 1 || p >= target.len() { continue; }
                    if tri != src_ri && target.demand_sum + seg_demand > ctx.cap { continue; }
                    let ta = target.seq[p-1] as usize;
                    let tb = target.seq[p] as usize;
                    let ins_cost = ctx.d[ta][seg_front] + ctx.d[seg_back][tb] - ctx.d[ta][tb];
                    let net = ins_cost - remove_gain;
                    if net < best_delta { best_delta = net; best_tri = tri; best_tp = p; }
                }
            }
            if best_tri != usize::MAX && best_delta < 0 {
                let seg: Vec<u32> = sol.routes[src_ri].seq[src_p..src_p+seg_len].to_vec();
                sol.routes[src_ri].seq.drain(src_p..src_p+seg_len);
                sol.routes[src_ri].recompute(ctx);
                let adj_p = if best_tri == src_ri && best_tp > src_p { best_tp - seg_len } else { best_tp };
                for (si, &node) in seg.iter().enumerate() {
                    sol.routes[best_tri].seq.insert(adj_p + si, node);
                }
                sol.routes[best_tri].recompute(ctx);
                let ok = sol.routes[best_tri].is_feasible(ctx)
                    && (best_tri == src_ri || sol.routes[src_ri].is_feasible(ctx));
                if ok {
                    rebuild_locator(ctx, sol, loc);
                    improved = true;
                    break;
                } else {
                    for _ in 0..seg_len { sol.routes[best_tri].seq.remove(adj_p); }
                    sol.routes[best_tri].recompute(ctx);
                    for (si, &node) in seg.iter().enumerate() {
                        sol.routes[src_ri].seq.insert(src_p + si, node);
                    }
                    sol.routes[src_ri].recompute(ctx);
                    rebuild_locator(ctx, sol, loc);
                }
            }
        }
    }
    improved
}

fn route_elim_pass(ctx: &Ctx, sol: &mut Sol, loc: &mut Vec<NodeLoc>) -> bool {
    if sol.routes.len() <= 1 { return false; }
    let smallest = sol.routes.iter().enumerate()
        .filter(|(_, r)| r.num_customers() > 0)
        .min_by_key(|(_, r)| r.num_customers()).map(|(i, _)| i);
    let Some(ri) = smallest else { return false };
    if sol.routes[ri].num_customers() > 8 { return false; }
    let customers: Vec<u32> = sol.routes[ri].seq[1..sol.routes[ri].len()-1].to_vec();
    let mut tmp = sol.clone();
    let mut tmp_loc = loc.to_vec();
    for &u in &customers {
        let ul = &tmp_loc[u as usize];
        if ul.route == u32::MAX { return false; }
        let sri = ul.route as usize;
        let sp = ul.pos as usize;
        let removed = tmp.routes[sri].remove_at(ctx, sp);
        rebuild_locator(ctx, &tmp, &mut tmp_loc);
        let mut bd = i32::MAX; let mut br = usize::MAX; let mut bp = 0;
        for (r2i, r) in tmp.routes.iter().enumerate() {
            if r.num_customers() == 0 { continue; }
            for p in 1..r.len() {
                if let Some(d) = r.try_insert(ctx, removed, p) {
                    if d < bd { bd = d; br = r2i; bp = p; }
                }
            }
        }
        if br == usize::MAX { return false; }
        tmp.routes[br].insert_at(ctx, removed, bp);
        rebuild_locator(ctx, &tmp, &mut tmp_loc);
    }
    tmp.cleanup_empty();
    tmp.recompute_total();
    if tmp.total_dist < sol.total_dist {
        *sol = tmp;
        rebuild_locator(ctx, sol, loc);
        return true;
    }
    false
}

// 2-opt*: swap tails between two routes (neighbor-based for speed)
fn two_opt_star_pass(ctx: &Ctx, sol: &mut Sol, loc: &mut Vec<NodeLoc>) -> bool {
    for u in 1..ctx.n as u32 {
        let ul = &loc[u as usize];
        if ul.route == u32::MAX { continue; }
        let r1i = ul.route as usize;
        let up = ul.pos as usize;
        for &v in &ctx.neighbors[u as usize] {
            let vl = &loc[v as usize];
            if vl.route == u32::MAX { continue; }
            let r2i = vl.route as usize;
            if r1i == r2i { continue; }
            let vp = vl.pos as usize;
            for &i in &[up, up + 1] {
                if i < 1 || i >= sol.routes[r1i].len() { continue; }
                for &j in &[vp, vp + 1] {
                    if j < 1 || j >= sol.routes[r2i].len() { continue; }
                    let a1 = sol.routes[r1i].seq[i-1] as usize;
                    let b1 = sol.routes[r1i].seq[i] as usize;
                    let a2 = sol.routes[r2i].seq[j-1] as usize;
                    let b2 = sol.routes[r2i].seq[j] as usize;
                    if ctx.d[a1][b2] + ctx.d[a2][b1] >= ctx.d[a1][b1] + ctx.d[a2][b2] { continue; }
                    let mut nr1: Vec<u32> = sol.routes[r1i].seq[..i].to_vec();
                    nr1.extend_from_slice(&sol.routes[r2i].seq[j..]);
                    let mut nr2: Vec<u32> = sol.routes[r2i].seq[..j].to_vec();
                    nr2.extend_from_slice(&sol.routes[r1i].seq[i..]);
                    let mut tr1 = Route { seq: nr1, arr: vec![], lat: vec![], demand_sum: 0, dist: 0 };
                    let mut tr2 = Route { seq: nr2, arr: vec![], lat: vec![], demand_sum: 0, dist: 0 };
                    tr1.recompute(ctx);
                    tr2.recompute(ctx);
                    if tr1.demand_sum <= ctx.cap && tr2.demand_sum <= ctx.cap
                        && tr1.is_feasible(ctx) && tr2.is_feasible(ctx) {
                        sol.routes[r1i] = tr1;
                        sol.routes[r2i] = tr2;
                        rebuild_locator(ctx, sol, loc);
                        return true;
                    }
                }
            }
        }
    }
    false
}

// ─── VND ───

fn vnd(ctx: &Ctx, sol: &mut Sol, loc: &mut Vec<NodeLoc>, start: &Instant, rng: &mut u64) {
    let mut improved = true;
    while improved {
        if start.elapsed().as_millis() > TIME_BUDGET_MS { break; }
        improved = false;
        let mut ops: [u8; 6] = [0, 1, 2, 3, 4, 5];
        for i in (1..6).rev() { let j = (xorshift64(rng) as usize) % (i + 1); ops.swap(i, j); }
        for &op in &ops {
            if start.elapsed().as_millis() > TIME_BUDGET_MS { break; }
            let found = match op {
                0 => relocate_pass(ctx, sol, loc),
                1 => or_opt_pass(ctx, sol, loc),
                2 => exchange_pass(ctx, sol, loc),
                3 => { let r = two_opt_intra_pass(ctx, sol); if r { rebuild_locator(ctx, sol, loc); } r },
                4 => two_opt_star_pass(ctx, sol, loc),
                _ => route_elim_pass(ctx, sol, loc),
            };
            if found { improved = true; break; }
        }
    }
    sol.cleanup_empty();
    sol.recompute_total();
    rebuild_locator(ctx, sol, loc);
}

// ─── Destroy + Repair ───

fn random_destroy(ctx: &Ctx, loc: &[NodeLoc], rng: &mut u64, count: usize) -> Vec<usize> {
    let mut chosen = Vec::with_capacity(count);
    let mut attempts = 0;
    while chosen.len() < count && attempts < count * 20 {
        attempts += 1;
        let u = 1 + (xorshift64(rng) as usize % (ctx.n - 1));
        if loc[u].route == u32::MAX || chosen.contains(&u) { continue; }
        chosen.push(u);
    }
    chosen
}

fn shaw_destroy(ctx: &Ctx, loc: &[NodeLoc], rng: &mut u64, count: usize) -> Vec<usize> {
    let seed = 1 + (xorshift64(rng) as usize % (ctx.n - 1));
    if loc[seed].route == u32::MAX { return vec![]; }
    let mut chosen = vec![seed];
    while chosen.len() < count {
        let pivot = chosen[(xorshift64(rng) as usize) % chosen.len()];
        let mut best_u = 0usize;
        let mut best_score = i32::MAX;
        for &v in &ctx.neighbors[pivot] {
            let vu = v as usize;
            if loc[vu].route == u32::MAX || chosen.contains(&vu) { continue; }
            let s = ctx.d[pivot][vu] + (ctx.ready[pivot] - ctx.ready[vu]).abs()
                + (ctx.due[pivot] - ctx.due[vu]).abs() / 8 + (xorshift64(rng) & 0x1F) as i32;
            if s < best_score { best_score = s; best_u = vu; }
        }
        if best_u == 0 { break; }
        chosen.push(best_u);
    }
    chosen
}

fn worst_destroy(ctx: &Ctx, sol: &Sol, rng: &mut u64, count: usize) -> Vec<usize> {
    let mut scores: Vec<(i32, usize)> = Vec::new();
    for r in &sol.routes {
        for p in 1..r.len().saturating_sub(1) {
            let u = r.seq[p] as usize;
            let a = r.seq[p-1] as usize;
            let b = r.seq[p+1] as usize;
            scores.push((-(ctx.d[a][u] + ctx.d[u][b] - ctx.d[a][b]), u));
        }
    }
    scores.sort_by_key(|x| x.0);
    let mut chosen = Vec::with_capacity(count);
    while chosen.len() < count && !scores.is_empty() {
        let r = (xorshift64(rng) as f64) / (u64::MAX as f64);
        let idx = (r.powf(4.0) * scores.len() as f64) as usize;
        let (_, u) = scores.remove(idx.min(scores.len()-1));
        if !chosen.contains(&u) { chosen.push(u); }
    }
    chosen
}

fn destroy_and_repair(ctx: &Ctx, sol: &mut Sol, loc: &mut Vec<NodeLoc>, rng: &mut u64, destroy_count: usize) {
    let to_remove = match xorshift64(rng) % 3 {
        0 => random_destroy(ctx, loc, rng, destroy_count),
        1 => shaw_destroy(ctx, loc, rng, destroy_count),
        _ => worst_destroy(ctx, sol, rng, destroy_count),
    };
    // Batch removal: group by route, remove in reverse position order
    let mut by_route = vec![Vec::new(); sol.routes.len()];
    for &u in &to_remove {
        if loc[u].route != u32::MAX {
            by_route[loc[u].route as usize].push(loc[u].pos as usize);
        }
    }
    for (ri, positions) in by_route.iter_mut().enumerate() {
        if positions.is_empty() { continue; }
        positions.sort_unstable();
        for &p in positions.iter().rev() {
            sol.routes[ri].seq.remove(p);
        }
        sol.routes[ri].recompute(ctx);
    }
    for &u in &to_remove { loc[u] = NodeLoc { route: u32::MAX, pos: u32::MAX }; }
    sol.cleanup_empty();
    rebuild_locator(ctx, sol, loc);

    let mut removed: Vec<usize> = to_remove;
    let big = i32::MAX / 4;
    while !removed.is_empty() {
        let mut pick = usize::MAX;
        let mut pick_r = 0;
        let mut pick_p = 0;
        let mut best_regret = i64::MIN;
        let mut pick_delta = 0i32;
        for (idx, &u) in removed.iter().enumerate() {
            let mut b1 = big; let mut b1_r = 0; let mut b1_p = 0; let mut b2 = big;
            for (ri, r) in sol.routes.iter().enumerate() {
                for p in 1..r.len() {
                    if let Some(delta) = r.try_insert(ctx, u, p) {
                        if delta < b1 { b2 = b1; b1 = delta; b1_r = ri; b1_p = p; }
                        else if delta < b2 { b2 = delta; }
                    }
                }
            }
            if b1 >= big { continue; }
            let regret = if b2 >= big { big as i64 - b1 as i64 } else { (b2 - b1) as i64 };
            if regret > best_regret || (regret == best_regret && b1 < pick_delta) {
                best_regret = regret; pick = idx; pick_r = b1_r; pick_p = b1_p; pick_delta = b1;
            }
        }
        if pick == usize::MAX {
            if sol.routes.len() < ctx.fleet { sol.routes.push(Route::empty(ctx)); continue; }
            break;
        }
        let u = removed.remove(pick);
        sol.routes[pick_r].insert_at(ctx, u, pick_p);
        refresh_route_loc(loc, sol, pick_r);
    }
    sol.cleanup_empty();
    sol.recompute_total();
    rebuild_locator(ctx, sol, loc);
}

// ─── Main ───

pub fn solve_challenge(
    challenge: &Challenge,
    save_solution: &dyn Fn(&Solution) -> Result<()>,
    _hyperparameters: &Option<Map<String, Value>>,
) -> Result<()> {
    let start = Instant::now();
    let ctx = Ctx::build(challenge);

    let initial = super::solomon::run(challenge)?;
    let mut sol = Sol {
        routes: initial.routes.iter().map(|r| {
            let mut route = Route { seq: r.iter().map(|&x| x as u32).collect(),
                arr: vec![], lat: vec![], demand_sum: 0, dist: 0 };
            route.recompute(&ctx);
            route
        }).collect(),
        total_dist: 0,
    };
    sol.recompute_total();
    save_solution(&sol.to_solution())?;

    let mut rng: u64 = 0x9E3779B97F4A7C15 ^ (ctx.n as u64) ^ ((challenge.seed[0] as u64) << 8);
    if rng == 0 { rng = 0xDEADBEEF; }

    let mut loc = build_locator(&ctx, &sol);
    let mut best = sol.clone();
    let mut best_dist = best.total_dist;

    vnd(&ctx, &mut sol, &mut loc, &start, &mut rng);
    if sol.total_dist < best_dist {
        best = sol.clone(); best_dist = best.total_dist;
        save_solution(&best.to_solution())?;
    }

    let mut current = best.clone();
    let mut current_dist = best_dist;
    let mut since_improve = 0u32;
    let sa_t0 = best_dist as f64 * 0.012;
    let sa_t_end = best_dist as f64 * 0.001;

    loop {
        if start.elapsed().as_millis() > TIME_BUDGET_MS { break; }
        let r = (xorshift64(&mut rng) & 0xFFFF) as u32;
        let base_pct = if r < 0x1800 { 15 + (xorshift64(&mut rng) & 0xF) as usize }
                       else if r < 0x4000 { 8 + (xorshift64(&mut rng) & 0x7) as usize }
                       else { 4 + (xorshift64(&mut rng) & 0x7) as usize };
        let destroy = (ctx.n * base_pct / 100 + (since_improve.min(20) as usize) / 4 * 3)
            .max(6).min(ctx.n / 3);

        if since_improve > 40 { current = best.clone(); current_dist = best_dist; since_improve = 0; }

        let mut cand = current.clone();
        rebuild_locator(&ctx, &cand, &mut loc);
        destroy_and_repair(&ctx, &mut cand, &mut loc, &mut rng, destroy);
        vnd(&ctx, &mut cand, &mut loc, &start, &mut rng);

        if cand.total_dist < best_dist {
            best = cand.clone(); best_dist = cand.total_dist;
            save_solution(&best.to_solution())?;
            current = cand; current_dist = best_dist; since_improve = 0;
        } else {
            let delta = cand.total_dist - current_dist;
            let accept = delta < 0 || {
                let frac = (start.elapsed().as_millis() as f64 / TIME_BUDGET_MS as f64).min(1.0);
                let temp = sa_t0 * (sa_t_end / sa_t0).powf(frac);
                let rand01 = (xorshift64(&mut rng) & 0xFFFFFF) as f64 / 0xFFFFFF_u64 as f64;
                (-(delta as f64) / temp).exp() > rand01
            };
            if accept { current_dist = cand.total_dist; current = cand; }
            since_improve = since_improve.saturating_add(1);
        }
    }
    save_solution(&best.to_solution())?;
    Ok(())
}

