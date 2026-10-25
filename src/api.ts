// Typed wrappers around Tauri commands. Every backend failure surfaces as a
// rejected promise with a human-readable string — pages render it as-is.
import { invoke } from "@tauri-apps/api/core";
import type {
  AiReport,
  AppSettings,
  CreateProblemReq,
  GenerationDraft,
  Problem,
  ProblemDetail,
  ReferenceSolution,
  RunSummary,
  SandboxReport,
  SaveCaseInput,
  Solution,
  TestCase,
  UpdateProblemReq,
} from "./types";

export const api = {
  listProblems: () => invoke<Problem[]>("list_problems"),

  createProblem: (req: CreateProblemReq) => invoke<Problem>("create_problem", { req }),

  getProblem: (problemId: string) => invoke<ProblemDetail>("get_problem", { problemId }),

  updateProblem: (problemId: string, req: UpdateProblemReq) =>
    invoke<Problem>("update_problem", { problemId, req }),

  deleteProblem: (problemId: string) => invoke<void>("delete_problem", { problemId }),

  saveSolution: (problemId: string, language: string, code: string) =>
    invoke<Solution>("save_solution", { problemId, language, code }),

  generateCases: (problemId: string) => invoke<GenerationDraft>("generate_cases", { problemId }),

  saveTestCases: (
    problemId: string,
    source: string,
    cases: SaveCaseInput[],
    reference: ReferenceSolution | null,
  ) => invoke<TestCase[]>("save_test_cases", { problemId, source, cases, reference }),

  runSolution: (problemId: string) => invoke<RunSummary>("run_solution", { problemId }),

  verifyOutputs: (problemId: string, outputs: string[]) =>
    invoke<RunSummary>("verify_outputs", { problemId, outputs }),

  exportCaseInputs: (problemId: string) => invoke<string>("export_case_inputs", { problemId }),

  listRuns: (problemId: string) => invoke<RunSummary[]>("list_runs", { problemId }),

  detectSandbox: () => invoke<SandboxReport>("detect_sandbox"),

  checkAi: () => invoke<AiReport>("check_ai"),

  getSettings: () => invoke<AppSettings>("get_settings"),

  updateSettings: (settings: AppSettings) => invoke<AppSettings>("update_settings", { settings }),
};
