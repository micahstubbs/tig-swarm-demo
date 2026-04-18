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
