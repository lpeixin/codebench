# local-leet — Design Document

A local-first, offline LeetCode-style practice platform. Paste a problem statement,
let a local LLM (Ollama) generate test cases and a reference solution, write your
solution in a built-in editor, and judge it locally in a sandbox — or, when no safe
sandbox exists, fall back to logic-verification mode that never executes your code.

Status: MVP design. Scope guard rails: Python only, single user, no cloud anything.

---

## 1. Architecture

```
┌──────────────────────────────────────────────────────────────────────────┐
│  Tauri 2 Window (WebView)                                                │
│                                                                          │
│  React + TypeScript + Vite                                               │
│  ┌────────────┐ ┌────────────┐ ┌───────────────────────┐ ┌────────────┐  │
│  │ Problem    │ │ Problem    │ │ Problem Detail        │ │ Settings   │  │
│  │ List       │ │ Import     │ │ tabs: statement /     │ │            │  │
│  │            │ │ (paste)    │ │ tests / editor /      │ │            │  │
│  │            │ │            │ │ results               │ │            │  │
│  └────────────┘ └────────────┘ └───────────────────────┘ └────────────┘  │
│         │  invoke("cmd", args)   (typed wrappers in src/api.ts)          │
└─────────┼────────────────────────────────────────────────────────────────┘
          ▼
┌──────────────────────────────────────────────────────────────────────────┐
│  Rust core (src-tauri, runs in the app process)                          │
│                                                                          │
│  commands.rs   thin Tauri commands: CRUD, generate, run, verify, settings│
│       │                                                                  │
│       ├─► db.rs          SQLite (rusqlite, bundled) — single source of   │
│       │                  truth; app_data_dir/local-leet.db              │
│       │                                                                  │
│       ├─► ai/            AiProvider trait                                │
│       │     ├─ ollama.rs     POST /api/chat (strict JSON mode)          │
│       │     └─ mock.rs       deterministic, offline, dev-only           │
│       │                                                                  │
│       ├─► sandbox/        Sandbox trait + backend registry               │
│       │     ├─ logic.rs       never executes (verification-only)        │
│       │     ├─ linux.rs       nsjail (detect at runtime)                │
│       │     ├─ macos.rs       sandbox-exec profile (best-effort)        │
│       │     └─ mock.rs        simulated results, dev-only               │
│       │                                                                  │
│       └─► judge.rs       orchestrates a run: write temp workdir, run     │
│                          each case under limits, compare, classify,     │
│                          persist Run                                     │
│                                                                          │
│  External processes: python3 (inside sandbox), nsjail / sandbox-exec,    │
│  Ollama HTTP server on localhost                                         │
└──────────────────────────────────────────────────────────────────────────┘
```

Key decisions:

- **No sidecar daemon in MVP.** The Rust core spawns short-lived child processes
  (`python3`, `nsjail`, `sandbox-exec`) per run. A persistent sidecar adds packaging
  complexity without MVP value; the `Sandbox` trait keeps that door open.
- **stdin/stdout convention.** A solution is a complete program that reads the case
  input from stdin and prints the answer to stdout. This avoids function-signature
  parsing/marshalling, works for any language we add later, and matches how AI models
  most reliably generate test data. The AI prompt contract enforces the same
  convention for the reference solution.
- **Mocks are first-class.** `MockAiProvider` and `MockSandbox` ship in the same
  trait slots as the real ones, so the app is fully usable with zero external
  dependencies (no Ollama, no sandbox tooling) for development and demos.

---

## 2. Module Boundaries

| Module | Owns | Must not |
|---|---|---|
| `commands.rs` | Tauri command surface, arg/result DTOs, state wiring | business logic |
| `db.rs` | schema, migrations, all SQL | Tauri/HTTP/process concerns |
| `models.rs` | domain types + JSON DTO shapes (serde) | behavior |
| `ai/` | prompt building, Ollama transport, JSON repair, mock generation | knowing about sandboxes (validation is driven from the command layer) |
| `sandbox/` | detection, per-OS executors, limits, timeout kill | comparing outputs (that's judge's job) |
| `judge.rs` | run orchestration, output comparison, status classification, run persistence | spawning processes directly (always goes through a `Sandbox`) |
| frontend `api.ts` | typed invoke wrappers + error surfacing | state management |

Rules of thumb: anything that touches the filesystem outside the DB lives in
`sandbox/` or `judge.rs`; anything that talks HTTP lives in `ai/`.

---

## 3. Data Model (SQLite)

```sql
CREATE TABLE problems (
  id          TEXT PRIMARY KEY,   -- uuid v4
  title       TEXT NOT NULL,
  statement   TEXT NOT NULL,
  constraints TEXT,               -- free text
  examples    TEXT,               -- JSON: [{input, output, explanation}]
  notes       TEXT,
  language    TEXT NOT NULL DEFAULT 'python',
  created_at  TEXT NOT NULL,      -- RFC3339
  updated_at  TEXT NOT NULL
);

CREATE TABLE test_cases (
  id              TEXT PRIMARY KEY,
  problem_id      TEXT NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
  idx             INTEGER NOT NULL,
  input           TEXT NOT NULL,
  expected_output TEXT NOT NULL,
  is_edge_case    INTEGER NOT NULL DEFAULT 0,
  source          TEXT NOT NULL,  -- 'ai' | 'mock' | 'manual'
  explanation     TEXT
);

CREATE TABLE solutions (
  id         TEXT PRIMARY KEY,
  problem_id TEXT NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
  language   TEXT NOT NULL,
  code       TEXT NOT NULL,
  updated_at TEXT NOT NULL
  -- one solution per (problem_id, language); upsert semantics
);

CREATE TABLE runs (
  id           TEXT PRIMARY KEY,
  problem_id   TEXT NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
  solution_id  TEXT,
  mode         TEXT NOT NULL,     -- 'sandbox' | 'logic' | 'mock'
  status       TEXT NOT NULL,     -- 'passed' | 'failed' | 'error'
  started_at   TEXT NOT NULL,
  results_json TEXT NOT NULL      -- full per-case detail (see RunResults)
);

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
```

Settings keys: `aiEndpoint`, `aiModel`, `aiMode` (`auto|ollama|mock`),
`numTestCases`, `timeLimitSec`, `memLimitMb`, `sandboxMode` (`auto|native|logic|mock`).

---

## 4. Main User Flows

### 4.1 Import a problem
List page → "Import" → paste title + statement (+ optional constraints,
examples, notes) → saved to SQLite → redirected to problem detail.

### 4.2 Generate test cases (AI)
Problem detail → Test Cases tab → "Generate".
1. Command layer picks provider: `aiMode=auto` pings Ollama, falls back to mock
   with a visible warning; `ollama`/`mock` are forced.
2. Provider returns strict-JSON draft: cases, reference solution, constraints summary.
3. Draft JSON is repaired (strip code fences), schema-validated.
4. **Validation step:** if an *executing* sandbox is available for Python, the
   reference solution runs against every generated case; cases that error or
   mismatch are marked `invalid` (excluded by default in review). Without a
   sandbox, all cases are marked `unvalidated`.
5. Draft is shown for review — nothing is persisted yet. User can edit,
   toggle include/exclude, mark edge cases, then **Save** (replaces the
   problem's AI/mock cases; manual cases are preserved).

### 4.3 Write & run a solution
Solution tab → CodeMirror editor (auto-saved solution row) → "Run".
1. Command layer resolves the sandbox (see §5). Result of resolution is always
   surfaced in the UI banner *before* anything executes.
2. `judge.rs` writes a temp workdir (`solution.py`, `case_N.txt`), runs each case
   through the sandbox backend with time/memory limits, trims output, compares,
   classifies: `passed | wrong_answer | timeout | runtime_error | internal`.
3. Results tab shows per-case status, expected vs actual, stderr (truncated),
   timing; a Run row is persisted.

### 4.4 Logic verification mode (no sandbox)
When the resolved mode is `logic` (or the user forces it):
- The Run button is replaced by an explanatory banner + a two-step flow:
  1. "Export inputs" produces a copyable text block (delimited per case) so the
     user can run their code outside the app.
  2. The user pastes actual outputs (one block per case) into per-case text
     fields (or marks each case pass/fail manually); "Verify" compares pasted vs
     expected and records a `mode=logic` Run.
- The UI states plainly *why* code is not executed (which sandbox check failed on
  this OS) and links to Settings.

---

## 5. Sandbox Interface & Fallback Strategy

```rust
pub struct SandboxReport {          // result of detect(): always shown in UI
    pub backend: String,            // "nsjail" | "macos-sandbox-exec" | "logic" | "mock"
    pub can_execute: bool,
    pub warnings: Vec<String>,      // e.g. "sandbox-exec is deprecated by Apple; best-effort"
    pub detail: String,             // human-readable detection result
}

pub trait Sandbox: Send + Sync {
    fn name(&self) -> &'static str;
    fn detect(&self) -> SandboxReport;
    fn can_run(&self, language: &str) -> bool;          // MVP: only "python"
    fn run(&self, code: &str, cases: &[Case], limits: &Limits) -> Vec<CaseResult>;
}
```

Resolution order (`sandboxMode` setting):

| setting | resolution |
|---|---|
| `auto` | Linux: nsjail if on PATH → else logic. macOS: sandbox-exec (present on all installs) with best-effort warning → else logic. Windows: logic (no native backend in MVP). |
| `native` | same detection, but if detection fails → logic **with a warning** (never silently execute unsandboxed) |
| `logic` | never execute, always logic mode |
| `mock` | dev-only simulated results, prominently labeled "MOCK — no code executed" |

Hard invariants:

1. **User code is only ever executed through a `Sandbox` backend that reported
   `can_execute` via `detect()`.** There is no code path from a Tauri command to
   `std::process` for user code except `sandbox/*/run`.
2. Time limit is enforced by the supervisor (poll + SIGKILL after deadline) on
   every backend, in addition to any backend-native limits (nsjail rlimits).
3. Each run gets a fresh temp dir; nothing from a run persists except DB rows.
4. Network is denied where the backend supports it (nsjail netns; sandbox-exec
   `deny network*`).

Per-backend specifics:

- **linux (nsjail):** `nsjail --mode o` with clone-newnet (loopback only),
  uid/gid 65534, `--rlimit_as`, `--rlimit_cpu`, `--time_limit`, workdir bind-mounted
  read-only except a scratch subdir, `--chroot /`. Requires `nsjail` on PATH.
- **macos (sandbox-exec):** profile allows read of system paths + workdir,
  write only inside workdir, denies network. Limits: time via supervisor kill;
  **no reliable memory cap** — surfaced as a warning. `sandbox-exec` is deprecated
  by Apple → labeled best-effort everywhere it appears.
- **windows:** no backend in MVP → auto-resolves to logic mode. (WSL2/Docker
  backend is a post-MVP slot in the registry.)

---

## 6. AI Prompt Contract

System prompt (sent to Ollama with `format: "json"`, `temperature: 0.2`):

```
You generate test data for programming problems. Reply with ONE JSON object and
nothing else — no markdown, no prose. Schema:
{
  "testCases": [
    {"input": "<full stdin text>", "expectedOutput": "<full stdout text>",
     "explanation": "<short>", "isEdgeCase": <bool>}
  ],
  "referenceSolution": {"language": "python", "code": "<complete program>"},
  "constraintsSummary": "<one paragraph>"
}
Hard rules:
- A solution reads the case input from stdin and prints the answer to stdout.
  "input" is the exact stdin; "expectedOutput" is the exact stdout (print a
  trailing newline).
- referenceSolution.code is a COMPLETE, runnable Python 3 program (imports
  allowed) following the same stdin/stdout convention, with no input() prompts
  or debugging output.
- Produce {N} cases: mostly normal cases plus at least 2 edge cases
  (minimum/maximum constraints, empty/single-element input, duplicates,
  overflow-ish values).
- expectedOutput MUST be what the reference solution would print for input.
```

User message: problem title + statement + constraints + examples (formatted).

Response handling (`ai/mod.rs`):
1. Strip code fences / extract the outermost `{ … }` before parsing.
2. Schema-validate field-by-field (types, non-empty, `isEdgeCase` coercion).
   Cases failing validation are dropped, not fatal.
3. If parsing fails entirely: retry once with an appended strictness reminder,
   then surface a typed error including the raw model output (truncated) so the
   user can see what the model actually said.
4. Validation-by-execution (§4.2 step 4) happens in the command layer because it
   needs the sandbox — the provider itself stays sandbox-agnostic.

Mock provider contract: same output type, generated deterministically from a
builtin "sum of integers" mini-problem (3 normal + 2 edge cases, internally
consistent stdin/stdout, plus a working Python reference). Clearly tagged
`provider: "mock"` so the review UI can badge it.

---

## 7. Error Handling

| Failure | Detection | Behavior |
|---|---|---|
| Ollama unreachable / timeout | ping `/api/tags` (3s) before generation | typed error; in `auto` mode fall back to mock with warning banner |
| Model returns non-JSON / wrong schema | parse + field validation | one strict retry, then error showing raw output (truncated to ~2 KB) |
| Reference solution inconsistent with a case | validation run | case marked `invalid`, excluded from save-by-default, kept visible for review |
| Sandbox tool missing (`nsjail`) | `detect()` PATH probe | auto → logic mode; UI explains what to install |
| User code timeout | supervisor deadline | case status `timeout`, process killed, remaining cases still run |
| User code crash / nonzero exit | exit status + stderr | `runtime_error` with stderr excerpt (first 1 KB) |
| Output mismatch | trimmed compare | `wrong_answer`, shows expected vs actual side by side |
| Internal errors (DB, IO) | `Result` propagation | user-facing message + `log::error!` trace; never a silent pass |
| Python3 missing on host | preflight probe of `python3 --version` | run aborted before executing anything; instructions shown |

Comparison rule: compare line-by-line after trimming trailing whitespace on each
line and dropping trailing empty lines (generated and hand-written outputs
routinely differ on trailing newlines; we do not want false negatives).

---

## 8. MVP Milestones

- **M0 — Scaffold (this repo):** app builds on all 3 OSes; mock AI + mock runner;
  problem CRUD; settings page; logic-verification compare works end-to-end.
- **M1 — Real AI:** Ollama provider + review flow + validation-by-execution where
  a sandbox exists.
- **M2 — Linux real judge:** nsjail backend, limits enforced, per-case results.
- **M3 — macOS + Windows polish:** best-effort sandbox-exec; Windows logic-mode
  UX (export/paste/mark) fully polished.
- **M4 — Packaging:** per-OS installers via `tauri build`.

Status: **M0 done** (all four tabs, mock AI + mock runner, CRUD, settings, logic
verification compare). **M1 code-complete**, needs a live Ollama to be called
done. **M2 written**, not yet run on Linux. **M3 partial**: macOS backend +
Windows logic-mode UX exist; both need a real-machine pass.

---

## 9. Acceptance Criteria (MVP)

1. `npm run tauri dev` launches on Linux, macOS, Windows.
2. Paste a problem → saved → appears in list → reopenable with all fields.
3. With Ollama configured: Generate → review (incl. invalid-case flags) → Save.
4. With a supported sandbox: Run executes Python and shows per-case
   passed/failed/timeout/runtime-error with expected vs actual.
5. Without a safe sandbox: app switches to logic mode, never executes user code,
   shows why, and Verify-on-pasted-outputs produces per-case results.
6. Everything above works with networking fully disabled (mock providers).

---

## 10. Non-Goals & Risks

Non-goals (MVP): accounts/cloud sync/online judge/leaderboards; languages beyond
Python; anti-cheat; container orchestration; concurrent multi-user anything.

Risks:

| Risk | Mitigation |
|---|---|
| Small local models produce broken JSON | `format:"json"` + fence-stripping repair + one strict retry + raw-output error surface |
| Generated expected outputs are wrong | validation-by-execution drops inconsistent cases; user reviews before save; `source` tracked |
| macOS sandbox-exec deprecated / weak isolation | labeled best-effort; logic mode is one click; no silent execution |
| nsjail not installed / namespaces blocked (some CI containers) | detection is a PATH + dry probe; graceful logic fallback |
| Hostile "problem statement" prompt-injects the LLM | prompt treats statement as untrusted data; model output is schema-validated and only ever *data* (code it produces runs sandboxed like user code) |
| Scope creep (more languages, sidecars, containers) | traits are the extension points; MVP ships Python + direct child processes |

---

## 11. Implementation revisions

Deviations from the design above, recorded as they were made. Each one changes
something a reader of §1–§10 would otherwise assume.

| # | Change | Why |
|---|---|---|
| R1 | Added a `reference_solutions` table (`problem_id` PK, `language`, `code`, `created_at`); `ProblemDetail` gained a `reference` field. | §3 has nowhere to put the AI's reference solution. Storing it in `solutions` would overwrite the user's work every time cases are regenerated, which is unacceptable. Kept separate and surfaced read-only in the Test cases tab. |
| R2 | `Sandbox::run` returns `Vec<ExecOutcome>` (idx, exit code, stdout, stderr, elapsed, infra error) instead of `Vec<CaseResult>`. | §2 says the sandbox must not compare outputs. Classification (`passed`/`wrong_answer`/`timeout`/…) now happens only in `judge.rs`, so every backend gets identical semantics. |
| R3 | `detect()` performs a **dry run**, not just a PATH probe: `sandbox-exec -p '(version 1)(allow default)' /usr/bin/true`, `nsjail -Mo -R / -- /bin/true`. | Discovered while building: `sandbox_apply` returns `EPERM` when the app itself runs inside another sandbox (nested seatbelt, container, CI runner) even though the binary exists. A PATH probe would report "safe" and then fail on the first real run. Now the fallback happens up front, with the real error text in the banner. |
| R4 | Case replacement is scoped by `source`: `ai`/`mock` replace only AI-produced rows, `manual` replaces the manual set. | §4.2 promises "manual cases are preserved". Index assignment keeps manual cases first so their indices stay stable across regenerations. |
| R5 | AI transport is `ureq` (synchronous), not `reqwest`. | `reqwest::blocking` builds its own Tokio runtime; dropping it inside `spawn_blocking` panics ("Cannot drop a runtime in a context where blocking is not allowed"). `ureq` has no runtime to drop. |
| R6 | The mock runner reports every case as passed with `actual = expected` and a stderr note; the run is persisted with `mode = mock`. | It exists to exercise the results UI offline. Labelled `MOCK` in the banner, the tab and the history; it never claims to have executed anything. |
| R7 | macOS memory limit is not attempted at all (no `setrlimit`); only the wall-clock limit is enforced. | macOS does not honour `RLIMIT_AS` for this purpose, and a limit that silently does nothing is worse than a stated limitation. Surfaced as a warning in every macOS report. |
| R8 | `list_runs` is capped at 20 rows in the command layer. | Unbounded history is a memory/rendering problem with no MVP benefit. |

Known gaps at the end of M0:

- The macOS profile's write confinement is **best-effort and unverified on this
  machine**: the development environment is itself a seatbelt sandbox, so
  `sandbox-exec` cannot be applied there at all (see R3). The profile syntax was
  validated (it parses; only `sandbox_apply` is refused), but its runtime
  behaviour must be checked on an unsandboxed Mac.
- The nsjail backend has not been executed on Linux yet; only its argument
  construction is unit-tested.
- Windows is logic-mode only, by design.

