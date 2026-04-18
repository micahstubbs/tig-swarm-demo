# Lessons Learned

Append-only log of debugging insights and non-obvious patterns discovered while working on this project.

---

## 2026-04-18T21:31 - Conventional dev ports collide on multi-project machines

**Problem**: Project docs and Dockerfile advertised `uvicorn server:app --port 8080`, but port 8080 was already bound by another process on the dev machine (registered in the local port-registry as `unknown-8080`, PID unknown). Starting the server would have failed silently or taken over a conflicting service.

**Root Cause**: 8080 is the most-reused "alternative HTTP" port in dev tooling — FastAPI/Express/Tomcat/Jenkins/countless demos all default to it. On a single-user dev machine that hosts many projects, collisions are the rule, not the exception. The `Dockerfile` hardcoded `EXPOSE 8080` and the CMD defaulted `${PORT:-8080}`, and the README/CLAUDE.md/DEPENDENCIES.md followed the Dockerfile without checking the local machine's actual port map.

**Lesson**: On a multi-project dev machine, **never copy a conventional default port from upstream docs without checking the local registry**. The registry (`portctl list` / `portctl get <port>`) is the source of truth, not the framework's default. For new projects, allocate from a project-specific range (8090-8099 for tig-swarm-demo here) and register the assignment so the next project sees it as taken.

**Solution**:
- `portctl allocate -s tig-swarm-demo-server --preferred 8090 --range-min 8090 --range-max 8099` → got 8090
- `portctl register -p 5173 -s tig-swarm-demo-dashboard` → claimed the Vite dev port explicitly
- Updated `Dockerfile`, `CLAUDE.md`, `README.md`, `DEPENDENCIES.md` to 8090
- Added a **Port Assignments** section to `CLAUDE.md` documenting both registrations so future sessions don't re-collide

**Prevention**:
- Before starting work on a project that exposes HTTP endpoints, run `/pcr` (resolve-port-conflicts) to scan references, check the registry, and reassign conflicts in one pass.
- When scaffolding a new project, pick ports from a free range and register them immediately — don't wait for the first collision to find out.
- Treat ports in `Dockerfile` / `docker-compose.yml` / framework CLI flags as project configuration that must match the local registry, not as upstream defaults to preserve verbatim.

---

## 2026-04-18T14:42 - m2p.py crashed on non-UTF8 pdflatex log bytes

**Problem**: `/m2p docs/security-audit.md` crashed with `UnicodeDecodeError: 'utf-8' codec can't decode byte 0xe2 in position 22754` when trying to read the pdflatex `.log` file. No PDF was produced even though the LaTeX document itself was valid UTF-8.

**Root Cause**: pdflatex writes log files in a mix of encodings — filenames, TeX primitives, and error messages can contain bytes from the system locale or legacy TeX encodings (often Latin-1 fragments embedded in what is mostly ASCII). The m2p.py script opened the log with a plain `open(log_file)` call, which defaults to UTF-8 strict decoding and fails the moment it hits one non-UTF8 byte. The log parsing step is for **layout verification** (checking for overfull hbox warnings), not for user-facing text, so strict decoding adds no value.

**Code Issue**:
```python
# Before (broken) — m2p.py line 546
with open(log_file) as f:
    log_content = f.read()

# After (fixed)
with open(log_file, errors='replace') as f:
    log_content = f.read()
```

**Solution**: Changed the single `open()` call to use `errors='replace'`. Log is still scanned for overfull warnings — the regex matches ASCII patterns so replacement characters never appear in the patterns we care about. Rebuilt, PDF generated cleanly on retry.

**Lesson**: When you're reading a log file purely to scan for patterns (not to display verbatim to the user), **always open it with `errors='replace'` or `errors='ignore'`**. pdflatex, cargo, npm, and most toolchain logs will eventually emit a non-UTF8 byte — a stray encoded filename, a locale-dependent error message, a copy-pasted terminal escape. Strict UTF-8 on read is a latent crash that will fire at the worst time (long build, big document, tight deadline).

**Prevention**: Apply the rule broadly to any Python script that does `open(log_file).read()` for pattern scanning. If the script matters to a pipeline, audit all `open()` calls that read from tool-produced files and add `errors='replace'`. Do NOT add it to files you'll write back (that silently corrupts data).

---

## 2026-04-18T14:42 - m2p fails on Unicode math symbols and on `&` inside code spans

**Problem**: `/m2p docs/research/…-vrptw-solver-improvements.md` (a technical report with Greek letters in formulas and Rust borrow syntax in code blocks) looped through every auto-fix level and never produced a PDF. Errors included `Unicode character − (U+2212)`, `Unicode character Γ (U+0393)`, combining macron U+0304 (from `c̄`), and `Misplaced alignment tab character &` pointing into `is\_feasible(&route)` and `fn concat(a: &Seg, b: &Seg)` inside a Rust fenced code block. On each failure the script deleted its temp `.tex`, hiding the exact line.

**Root Cause**: Two independent issues in the m2p → pdflatex pipeline:

1. **pdflatex is not Unicode-native.** The m2p template omits `\usepackage{inputenc}` for modern math glyphs, so Greek letters (φ χ ψ ω α β γ δ η ξ μ σ), operators (− ≈ ≤ ≥ ∈ ∪ ⇔ Δ Σ), middle-dot (·), superscripts (² ³), ellipsis (…), em/en-dash (— –), and combining diacritics (̄ in `c̄`) all throw "Unicode character undefined."
2. **`&` is the LaTeX alignment-tab character and survives m2p's escaping inside code.** m2p.py pre-escapes `&` → `\&` globally, then unescapes it back to `&` inside inline code spans (line 123) and leaves it untouched inside fenced code blocks that become `\begin{lstlisting}`. In both contexts LaTeX reads the `&` as a column separator and errors out. Rust borrow syntax (`&route`, `&mut`, `&Seg`) is the most common trigger in this project's docs.

**Code Issue**:
```
// Source markdown (breaks pdflatex):
- Shaw weights: φ=9, χ=3, ψ=2, ω=5; blink probability β ≈ 0.01
- `is_feasible(&route)` on every move
```rust
fn concat(a: &Seg, b: &Seg) -> Seg { ... }
```

// Sanitized markdown (renders):
- Shaw weights: phi=9, chi=3, psi=2, omega=5; blink probability beta ~ 0.01
- `is_feasible(ref route)` on every move
```rust
fn concat(a: Seg, b: Seg) -> Seg { ... }
```

**Solution**: Pre-sanitize the markdown before `/m2p`:

```python
import re
replacements = {
    '−':'-','≈':'~','≥':'>=','≤':'<=','∈':'in','∪':'U','⇔':'<=>',
    'φ':'phi','χ':'chi','ψ':'psi','ω':'omega','α':'alpha','β':'beta',
    'γ':'gamma','δ':'delta','η':'eta','ξ':'xi','μ':'mu','σ':'sigma',
    'Γ':'Gamma','Δ':'Delta','Σ':'Sigma',
    '·':'*','×':'x','²':'^2','³':'^3','…':'...','—':'--','–':'-',
    'ĉ':'c_hat','c̄':'c_avg','̄':'','§':'Sec.','ä':'ae','ℓ':'l','→':'->',
}
for k,v in replacements.items(): text = text.replace(k,v)
# Strip & from code spans and fenced code blocks
text = re.sub(r'`([^`\n]*)`',
              lambda m: '`' + m.group(1).replace('&','ref ') + '`', text)
text = re.sub(r'```[^\n]*\n.*?```',
              lambda m: m.group(0).replace('&',''), text, flags=re.DOTALL)
```

**Lesson**: `/m2p` inherits all of `pdflatex`'s Unicode limitations silently — the build just fails with cryptic errors, and the auto-fix loop can't rescue it because the issues are at the character level, not the layout level. The markdown author is responsible for producing pdflatex-safe source.

**Prevention**:
- When drafting technical markdown destined for `/m2p`, use **ASCII-only math notation from the start**: `phi` not `φ`, `~` not `≈`, `Gamma` not `Γ`, `*` not `·`. The urge to write `φ` looks nicer in the editor but costs 30 minutes of sanitization later.
- Avoid `&` inside inline code spans and fenced code blocks. For Rust borrow syntax in prose snippets, use `ref` or elide — it rarely adds comprehension.
- If m2p fails with Unicode/alignment errors and the auto-fix loop burns through levels, run it once with `--no-verify` and intercept the generated `.tex` via `python -c "import m2p; m2p.generate_latex(m2p.parse_markdown(open('...').read()))"` so you can see the exact failing line.
- Long-term fix at the tool level: `m2p.py` should either switch to `lualatex`/`xelatex` (Unicode-native) or add a canonicalization pass that maps common math Unicode to LaTeX commands. Neither exists yet.

---

## 2026-04-18T22:00 - SA/ALNS cooling rate must scale with time budget

**Problem**: After extending the VRPTW solver's time budget from 4.5s to 27s (~6x), the algorithm should improve proportionally with the extra search time. But if the cooling rate is left unchanged, the simulated annealing temperature decays to near-zero far too early, and the remaining ~80% of the budget is wasted on greedy-only search that can't escape local optima.

**Root Cause**: The cooling rate `c` is tuned to reach a target final temperature after N iterations. If the iteration count scales by factor k (from a longer time budget), the same cooling rate produces `c^(kN)` = `(c^N)^k`, which is exponentially smaller than the intended final temperature. For example, with c=0.9995 and 6x more iterations, the final temperature is `(0.9995^N)^6` — essentially zero if the original final temp was already small.

**Lesson**: When scaling a metaheuristic's time budget by factor k, adjust the cooling rate to `c_new = c_old^(1/k)`. This preserves the same annealing schedule (same initial and final temperatures) stretched over the longer run. The formula: `c_new = exp(ln(c_old) / k)`.

**Code Issue**:
```rust
// Before — tuned for 3.2s ALNS phase
let cooling = 0.9995;

// After — tuned for 19s ALNS phase (~6x longer)
// c_new = 0.9995^(1/6) ≈ 0.99993
let cooling = 0.99993;
```

Same adjustment for the SA fine-tuning phase: 0.9998 → 0.99997.

**Solution**: Applied `c^(1/k)` scaling to both cooling rates. Also scaled the ALNS weight-update segment length (80 → 500) and reheat interval (1200 → 8000) proportionally so those mechanisms fire at the same relative frequency in the search.

**Prevention**: Whenever changing a solver's time budget, audit all time-dependent parameters: cooling rates, segment lengths, reheat intervals, stagnation thresholds. A checklist: `cooling`, `segment_length`, `reheat_every`, `stag_threshold`, `initial_temperature`. Scale each by the budget ratio.

---

## 2026-04-18T15:30 - Benchmark noise dwarfs single-run optimizer deltas

**Problem**: Added SWAP* (a known-strong VRPTW neighborhood) to the SA operator mix and saw a "+1.29% regression" on the first benchmark comparison. Reflex was to call the experiment failed and revert. But a second pair of runs showed the opposite sign, and three baseline-only runs spanned 7354–7548 (Δ ≈ 2.6%) — wider than the claimed effect.

**Root Cause**: The benchmark harness runs 24 Solomon instances under a 27s wall-clock budget each, with thread-scheduling, system load, and RNG-path variance all contributing to run-to-run score differences. A single A/B comparison with n=1 per arm has no way to distinguish a real ±1% effect from noise; the standard error of the mean is ~1.5% with n=1 and only drops to ~0.7% at n=5.

**Lesson**: Before declaring an optimizer change "worse" (or "better") based on a benchmark delta, measure the baseline's own run-to-run variance. If the variance band overlaps the claimed delta, the comparison is inconclusive — don't revert, don't merge; characterize more runs or design a noise-robust test.

**Code Issue** (decision logic, not source):
```
// Before — single-run comparison
bench(baseline) -> 7354
bench(new)      -> 7449   # +1.29% "regression"
-> revert / reject

// After — paired multi-run with variance estimate
bench(baseline) x3 -> [7354, 7548, 7457]   # sd ≈ 80, CV ≈ 1.3%
bench(new)      x2 -> [7449, 7582]         # mean 7516
delta 7516 - 7454 = +62 (+0.8%), inside 1-sigma band
-> inconclusive; needs more runs OR seed-fixed harness
```

**Solution**: For this session, reported as "within noise" and committed the code on a branch for future investigation rather than merging or reverting. Logged both arm's full run set in the benchmark-result JSON so a follow-up can extend without re-running from scratch.

**Prevention**:
- Always run the baseline at least 2–3 times on the same commit before comparing anything. The first delta of the session is the least trustworthy number you will see.
- When the harness has stochastic components (parallelism, RNG seeds, wall-clock deadlines), a seed-fixed or deadline-iteration-count harness would collapse most of the variance. Worth building once the portfolio of candidate operators grows.
- In decision-making: "improvement > 1-sigma" is a weak signal; "improvement > 2-sigma with n≥3" is the minimum before shipping a change. Anything below that is a "keep the branch, run more trials later" call, not a merge or revert.
