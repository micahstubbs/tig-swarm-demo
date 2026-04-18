# Dependencies

This document lists every runtime and build dependency used by the project and explains how to install each one.

The repository has three components with distinct toolchains:

| Component | Language | Manifest |
|-----------|----------|----------|
| Solver (what swarm agents optimize) | Rust 2021 | `Cargo.toml` |
| Coordination server | Python 3.12 | `server/requirements.txt` |
| Dashboard | TypeScript / Node 20 | `dashboard/package.json` |
| Agent / benchmark scripts | Python 3 (stdlib only) | `scripts/` |

---

## 1. System-Level Toolchains

### Rust (required for agents and the solver)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

This installs `rustup`, `cargo`, and a current stable `rustc`. The project uses edition 2021 and has no `rust-toolchain.toml`, so any recent stable release works.

### Python 3.12 (for server + scripts)

The Dockerfile pins `python:3.12-slim`. Locally, 3.10+ is sufficient for the scripts directory; 3.12 is recommended to match production. On Ubuntu:

```bash
sudo apt install python3 python3-pip python3-venv
```

### Node.js 20 (for dashboard)

The Dockerfile uses `node:20-slim`. Install via `nvm` to avoid version conflicts:

```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash
nvm install 20
nvm use 20
```

---

## 2. Rust Crates (`Cargo.toml`)

All Rust dependencies are fetched automatically when you run `cargo build`. No manual install required.

| Crate | Version | Purpose |
|-------|---------|---------|
| `anyhow` | 1.0.81 | Error type used throughout the solver |
| `clap` | 4.5.4 | CLI argument parsing for the three binaries |
| `blake3` | 1.5.4 | Hashing (seeds, fingerprints) |
| `paste` | 1.0.15 | Macro utility used by the challenge macros |
| `ndarray` | 0.15.6 | N-dimensional arrays (instance generation) |
| `rand` | 0.8.5 | RNG (`SmallRng`, `StdRng`; no `default-features`) |
| `rand_distr` | 0.4.3 | Probability distributions |
| `rayon` | 1.10 | Data parallelism — optional, used only by `tig_generator` |
| `serde` | 1.0.196 | Derive-based serialization framework |
| `serde_json` | 1.0.113 | JSON codec |
| `statrs` | 0.18.0 | Statistical functions (erf, erf_inv) |

Build the three binaries (feature flags are required):

```bash
cargo build -r --bin tig_solver    --features solver,vehicle_routing
cargo build -r --bin tig_evaluator --features evaluator,vehicle_routing
cargo build -r --bin tig_generator --features generator,vehicle_routing
```

> **Important:** `solver` does NOT imply `vehicle_routing` — you must specify both. `evaluator` and `generator` already pull in `vehicle_routing` via feature unification.

Run tests:

```bash
cargo test --features vehicle_routing
```

---

## 3. Python Packages — Server (`server/requirements.txt`)

| Package | Version | Purpose |
|---------|---------|---------|
| `fastapi` | 0.115.0 | HTTP + WebSocket framework |
| `uvicorn[standard]` | 0.30.0 | ASGI server (includes `uvloop`, `httptools`, `websockets`) |
| `websockets` | 13.0 | WebSocket protocol library |
| `aiosqlite` | 0.20.0 | Async SQLite driver |

Install into a virtualenv:

```bash
cd server
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

Run the server locally:

```bash
uvicorn server:app --port 8080
```

---

## 4. Python Packages — Scripts (`scripts/`)

The agent-facing scripts (`benchmark.py`, `publish.py`, `bootstrap_seed.py`, `tig.py`) use **only the Python 3 standard library** — `json`, `urllib.request`, `subprocess`, `concurrent.futures`, `pathlib`, `tempfile`, `os`, `sys`, `time`.

No `pip install` is required to run them. If you're using a system Python 3.10+, you're done.

---

## 5. Node Packages — Dashboard (`dashboard/package.json`)

### Runtime dependencies

| Package | Version | Purpose |
|---------|---------|---------|
| `d3` | ^7.9.0 | Data visualization (charts, routes, scales) |
| `@types/d3` | ^7.4.3 | TypeScript types for D3 |
| `qrcode` | ^1.5.4 | QR code overlay (attendees scan to join) |

### Dev dependencies

| Package | Version | Purpose |
|---------|---------|---------|
| `typescript` | ~6.0.2 | Type checker and compiler |
| `vite` | ^8.0.4 | Dev server and bundler |

Install and run:

```bash
cd dashboard
npm install
npm run dev      # dev server on localhost:5173 — append ?mock=true for no-server mode
npm run build    # production build into dashboard/dist/
npm run preview  # preview production build
```

---

## 6. Full Project Setup from a Clean Clone

```bash
# 1. Toolchains (one-time, per machine)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
nvm install 20 && nvm use 20          # or: apt install nodejs
sudo apt install python3 python3-venv python3-pip

# 2. Rust solver (cargo fetches crates automatically)
cargo build -r --bin tig_solver --features solver,vehicle_routing

# 3. Python server (optional — only needed for local server dev)
cd server
python3 -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
cd ..

# 4. Dashboard (optional — only needed for local dashboard dev)
cd dashboard
npm install
cd ..

# 5. Verify end-to-end
python3 scripts/benchmark.py | head -5
```

---

## 7. Docker (all-in-one)

For reproducible production builds the `Dockerfile` ships the server + pre-built dashboard:

```bash
docker build -t tig-swarm-demo .
docker run -p 8080:8080 tig-swarm-demo
```

Stage 1 builds the dashboard with `node:20-slim`; stage 2 installs Python deps from `server/requirements.txt` on `python:3.12-slim` and mounts the dashboard build as static files.

---

## 8. What Swarm Agents Actually Need

If you're running **only as a swarm agent** (editing `src/vehicle_routing/algorithm/mod.rs` and benchmarking), the minimum is:

- Rust (`cargo`)
- Python 3 (stdlib only — no pip install needed)

The server and dashboard are not required; agents talk to the hosted server at `https://demo.discoveryatscale.com`.
