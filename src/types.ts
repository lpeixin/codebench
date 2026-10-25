// Frontend mirrors of the Rust DTOs (src-tauri/src/models.rs).
// All fields are camelCase because the Rust side uses #[serde(rename_all = "camelCase")].

export interface Example {
  input: string;
  output: string;
  explanation?: string | null;
}

export interface Problem {
  id: string;
  title: string;
  statement: string;
  constraints?: string | null;
  examples: Example[];
  notes?: string | null;
  language: string;
  createdAt: string;
  updatedAt: string;
}

export interface CreateProblemReq {
  title: string;
  statement: string;
  constraints?: string | null;
  examples: Example[];
  notes?: string | null;
  language?: string | null;
}

export interface UpdateProblemReq {
  title: string;
  statement: string;
  constraints?: string | null;
  notes?: string | null;
}

export interface TestCase {
  id: string;
  problemId: string;
  idx: number;
  input: string;
  expectedOutput: string;
  isEdgeCase: boolean;
  source: string; // 'ai' | 'mock' | 'manual'
  explanation?: string | null;
}

export interface Solution {
  id: string;
  problemId: string;
  language: string;
  code: string;
  updatedAt: string;
}

export type CaseStatus = "passed" | "wrong_answer" | "timeout" | "runtime_error" | "internal";

export interface CaseResult {
  caseId: string;
  idx: number;
  status: CaseStatus;
  expected: string;
  actual: string;
  stderr?: string | null;
  timeMs?: number | null;
}

export type RunMode = "sandbox" | "logic" | "mock";
export type RunStatus = "passed" | "failed" | "error";

export interface RunSummary {
  id: string;
  problemId: string;
  mode: RunMode;
  status: RunStatus;
  startedAt: string;
  results: CaseResult[];
}

export interface ProblemDetail {
  problem: Problem;
  testCases: TestCase[];
  solution: Solution | null;
  /** AI-generated reference, stored separately from the user's own solution. */
  reference: ReferenceSolution | null;
  lastRun: RunSummary | null;
}

export interface GeneratedCase {
  input: string;
  expectedOutput: string;
  explanation?: string | null;
  isEdgeCase: boolean;
  validation: "validated" | "unvalidated" | "invalid";
  invalidReason?: string | null;
}

export interface ReferenceSolution {
  language: string;
  code: string;
}

export interface GenerationDraft {
  provider: string; // "ollama:<model>" | "mock"
  constraintsSummary?: string | null;
  referenceSolution?: ReferenceSolution | null;
  cases: GeneratedCase[];
  warnings: string[];
}

export interface SaveCaseInput {
  input: string;
  expectedOutput: string;
  explanation?: string | null;
  isEdgeCase: boolean;
}

export type AiMode = "auto" | "ollama" | "mock";
export type SandboxMode = "auto" | "native" | "logic" | "mock";

export interface AppSettings {
  aiEndpoint: string;
  aiModel: string;
  aiMode: AiMode;
  numTestCases: number;
  timeLimitSec: number;
  memLimitMb: number;
  sandboxMode: SandboxMode;
}

export interface SandboxReport {
  requestedMode: string;
  backend: string;
  canExecute: boolean;
  pythonAvailable: boolean;
  warnings: string[];
  detail: string;
}

export interface AiReport {
  provider: string;
  ok: boolean;
  detail: string;
  models: string[];
}
