# Session Summary: VRPTW Solver Literature Research and Skill Hardening

## Summary

Produced a comprehensive research report on state-of-the-art VRPTW (Vehicle
Routing Problem with Time Windows) algorithms, with 17 prioritized Rust-
implementable improvements for this project's single-threaded ALNS solver.
During PDF generation, encountered repeated `/m2p` failures on Unicode
math symbols and `&` in code blocks — captured this as a lesson, created a
new `mds` (m2p-sanitize) skill, and updated the `m2p` and `tsr` skills to
reference it.

## Completed Work

- **tig-swarm-demo-vlv** — Research VRPTW literature and suggest solver
  improvements. Closed with commit [78b2260](.) and follow-up commits on `main`.
- Report produced in three formats: `docs/research/2026-04-18-142524-vrptw-solver-improvements.{md,tex,pdf}` (214 KB PDF, 8 pages, two-column).
- Lesson captured in `LESSONS.md`: m2p + Unicode + `&`-in-code failure modes.
- New user-level skill `mds` / `m2p-sanitize` at `~/.claude/skills/` with
  deterministic Python script at `~/.claude/scripts/m2p-sanitize.py`.
- Updated user-level skills `m2p` and `tsr` to reference the new sanitize step.

## Key Changes

### Project (tig-swarm-demo)
- `docs/research/2026-04-18-142524-vrptw-solver-improvements.{md,tex,pdf}` — the report, 8 page PDF with pages dir.
- `LESSONS.md` — appended "m2p fails on Unicode math symbols and on `&` inside code spans" lesson.

### User home (~/.claude)
- `scripts/m2p-sanitize.py` (new) — maps 70+ Unicode symbols to ASCII, strips `&` from code spans and fenced blocks, idempotent, `--dry-run` flag.
- `skills/mds/SKILL.md` (new) — primary skill.
- `skills/m2p-sanitize/SKILL.md` (new) — alias.
- `skills/m2p/SKILL.md` — added "ASCII-safe math notation" and "Avoid `&` inside code" content guidelines, plus pre-flight `/mds` reference.
- `skills/tsr/SKILL.md` — inserted Step 7 (sanitize) between write and PDF build; fixed commit step to include `.tex`.

## Key Findings (from the research)

The current hybrid ALNS solver spends only **~4.5 s of the 30 s budget** per
instance and lacks three high-ROI techniques:

1. **Vidal-style concatenation-based move evaluation** (Vidal 2014/2022): amortized O(1) per move via `(duration, earliest_departure, latest_arrival, time_warp)` segment descriptors. 10–30× speedup on 400-node instances.
2. **SISRs destroy operator** (Christiaens & Vanden Berghe 2020): contiguous string removal preserves slack, consistently beats random/worst/Shaw.
3. **SWAP\*** (Vidal 2022): swap customers with best-position reinsertion in the other route, pruned by geometric sector overlap.

The one-line fix with highest ROI: extend the internal deadline from 4500 ms
to ~27 000 ms. Most of the 30 s budget is currently wasted.

## Pending / Blocked

- `git push` from the project: SSH key issue prevented push to the project's
  remote; commits saved locally. Not blocking — fixable separately.
- 1 overfull `vbox` warning (46.9 pt) in the PDF that m2p's auto-fix loop
  couldn't resolve. The PDF renders fine visually; the warning is cosmetic.

## Next Session Context

- If revisiting the solver: start with the 17-item priority list in the
  report; items P0–P5 are the no-regret picks (time budget, 2-opt\*,
  bitset cleanups, neighbor lists, Vidal concatenation).
- The `mds` skill is now the front-of-pipeline step for any technical
  markdown → PDF workflow. `/tsr` now invokes it automatically for reports.
- An open beads issue `tig-swarm-demo-vlv` was closed this session.
- The research hypotheses are tagged and ready to feed into the swarm
  coordination server via the Step 5 `publish.py` flow if the agent wants
  to iterate.
