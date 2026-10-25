# local-leet

A local-first, offline LeetCode-style practice app. Paste a problem statement,
let a local LLM generate test cases and a reference solution, write your solution
in the built-in editor, and judge it locally — in a sandbox when one is
available, or in **logic verification mode** (you run the code, paste the output)
when it is not. Nothing leaves your machine.

- Tauri 2 shell · React + TypeScript + Vite · CodeMirror 6 · Rust core
- SQLite storage (one file in the OS app-data directory)
- Ollama for generation, with a deterministic mock provider for offline work
- Python-only MVP, but the runner sits behind a trait so more languages are a
  small addition

Design notes, invariants, and trade-offs: [`DESIGN.md`](./DESIGN.md).

---

## Requirements

| | |
|---|---|
| Node | 20+ |
| Rust | 1.77+ (edition 2021) |
| Python | 3.8+ on `PATH` as `python3` (or `python` on Windows) — only needed to actually judge code |
| Ollama | optional; without it the app uses the mock provider |

Platform toolchains for Tauri: see
[tauri.app prerequisites](https://tauri.app/start/prerequisites/). On Debian/Ubuntu
that is `libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`.

## Run it

```bash
npm install
npm run tauri dev
```

The app starts with **mock AI** and **auto sandbox** settings, so it is fully
usable offline before you install anything else:

1. **Import problem** — paste a statement (title + body are enough).
2. Open it → **Test cases → Generate cases**. With `aiMode = auto` and no Ollama
   running, you get the mock provider's cases, clearly badged `mock`, with a
   warning that they belong to a built-in demo problem.
3. Review, tick/untick cases, edit inputs or expected outputs, then **Save**.
4. **Solution** tab — write a program that reads stdin and prints to stdout.
5. **Run** — per-case pass/fail, expected vs actual, timing, stderr excerpt.

## Use a real model

```bash
ollama serve            # in another terminal
ollama pull qwen2.5-coder:7b
```

In **Settings**, set the endpoint (`http://localhost:11434`), the model name, and
provider mode `ollama` (or `auto` to fall back to mock when Ollama is down). Use
**Test connection** to check.

Generation asks for strict JSON, repairs code fences, validates field by field,
retries once with a stricter reminder, and — when a sandbox exists — runs the
model's own reference solution against every case, marking disagreements
`invalid` and excluding them by default.

## Sandbox matrix

| OS | Backend | Status |
|---|---|---|
| Linux | `nsjail` | Isolation + rlimits. Install: `apt install nsjail` (or build from source). Disabled automatically if the dry-run probe fails (no user namespaces, e.g. some containers). |
| macOS | `sandbox-exec` | Best effort: network denied, writes confined to the run's temp dir, wall-clock limit enforced by the supervisor. Apple deprecated it; memory limits are **not** enforced. |
| Windows | — | No native backend in the MVP → logic verification mode. |
| any | `mock` | Simulated results for UI development. Never executes code. |
| any | `logic` | Never executes. Export inputs, run the code yourself, paste outputs. |

`detect()` does not just look for a binary: it dry-runs the sandbox. If the
sandbox cannot be applied (nested sandbox, restricted container), the app falls
back to logic mode and says why — it never silently runs unsandboxed code.

## Scripts

| command | what it does |
|---|---|
| `npm run tauri dev` | dev app (Vite HMR + Rust) |
| `npm run tauri build` | platform installers |
| `npm run build` | typecheck + frontend bundle |
| `npm run icon` | regenerate the icon set from `scripts/make-icon.mjs` |
| `cargo test --manifest-path src-tauri/Cargo.toml` | core tests (db, AI parsing, judge, supervisor) |

## Layout

```
src/                  React UI
  api.ts              typed wrappers around Tauri commands
  types.ts            mirrors of the Rust DTOs (camelCase)
  pages/              ProblemList · ProblemImport · ProblemDetail · SettingsPage
  components/         SandboxBanner · CaseReview · RunResults · LogicVerify · CodeEditor
src-tauri/src/
  commands.rs         the only Tauri-aware module
  db.rs               schema + all SQL
  models.rs           DTOs
  ai/                 ollama.rs · mock.rs · prompt + JSON validation
  sandbox/            proc.rs (supervisor) · macos.rs · linux.rs · logic.rs · mock.rs
  judge.rs            run orchestration, output comparison, classification
```

## Troubleshooting

**"Logic verification mode" on macOS.** The sandbox dry-run failed. Run the app
from a normal terminal, not from inside another sandbox or a CI runner.

**"Logic verification mode" on Linux.** `nsjail` is missing, or namespaces are
blocked. Check `nsjail -Mo -R / -- /bin/true` manually.

**`python3` not found.** The app probes `python3 --version` at detection time.
Install Python 3 and make sure it is on the `PATH` that launches the app.

**Generated cases look unrelated to my problem.** The mock provider is active
(the badge in the review panel says `mock`). Start Ollama and switch the provider.

## Non-goals (MVP)

Accounts, cloud sync, online judge, leaderboards; languages beyond Python;
anti-cheat; container orchestration. See `DESIGN.md` §10.
