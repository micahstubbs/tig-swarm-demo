# Cargo Audit — Accepted Risks

Last audit: 2026-04-18

Running `cargo audit` on this workspace surfaces two warnings. Both originate in
the upstream `tig-challenges` crate and its transitive dependencies (`nalgebra`,
`statrs`, `rand_distr`) — we don't pin these directly and can't upgrade them
without a coordinated release from the tig-foundation project. Both are accepted
risks for the demo; neither is reachable on a public attack surface.

## RUSTSEC-2024-0436 — `paste` 1.0.15 (unmaintained)

Advisory category: unmaintained crate. Not a vulnerability — the original
author stopped publishing updates. The crate performs compile-time macro
expansion only; it has no runtime attack surface and no exploitable defect is
known.

Dependency tree:

```
paste 1.0.15
├── tig-challenges 0.1.0
└── simba 0.9.1 → nalgebra 0.33.2 → statrs 0.18.0 → tig-challenges 0.1.0
```

Action: accept and monitor. Track `tig-challenges` for a release that drops
`paste` or picks up a maintained fork.

## RUSTSEC-2026-0097 — `rand` 0.8.5 (unsound with custom logger)

Advisory category: unsound. Triggered only by a specific interaction between
`rand::rng()` and a user-supplied `log`-crate backend. The solver runs
headless in a sandboxed benchmark harness with no custom logger wired in, so
the unsound code path is unreachable in practice.

Dependency tree:

```
rand 0.8.5
├── tig-challenges 0.1.0
├── statrs 0.18.0 → tig-challenges 0.1.0
├── rand_distr 0.4.3 → tig-challenges / nalgebra → statrs → tig-challenges
└── nalgebra 0.33.2
```

Action: accept and monitor. Upgrade to `rand` 0.9 when `tig-challenges` and its
transitive stack (`statrs`, `nalgebra`, `rand_distr`) move to the 0.9 line.

## Re-running the audit

```bash
cargo install cargo-audit --locked   # one-time
cargo audit
```
