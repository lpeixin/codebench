import { useState } from "react";
import { ProblemDetail } from "./pages/ProblemDetail";
import { ProblemImport } from "./pages/ProblemImport";
import { ProblemList } from "./pages/ProblemList";
import { SettingsPage } from "./pages/SettingsPage";

export type View =
  | { name: "list" }
  | { name: "import" }
  | { name: "detail"; problemId: string }
  | { name: "settings" };

export default function App() {
  const [view, setView] = useState<View>({ name: "list" });

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          local<span className="accent">-leet</span>
        </div>
        <nav>
          <button
            className={view.name === "list" ? "active" : ""}
            onClick={() => setView({ name: "list" })}
          >
            Problems
          </button>
          <button onClick={() => setView({ name: "import" })}>＋ Import problem</button>
          <button
            className={view.name === "settings" ? "active" : ""}
            onClick={() => setView({ name: "settings" })}
          >
            Settings
          </button>
        </nav>
        <div className="sidebar-foot">offline · local-first · sandboxed</div>
      </aside>
      <main className="content">
        {view.name === "list" && (
          <ProblemList
            onOpen={(id) => setView({ name: "detail", problemId: id })}
            onImport={() => setView({ name: "import" })}
          />
        )}
        {view.name === "import" && (
          <ProblemImport
            onCreated={(id) => setView({ name: "detail", problemId: id })}
            onCancel={() => setView({ name: "list" })}
          />
        )}
        {view.name === "detail" && (
          <ProblemDetail
            key={view.problemId}
            problemId={view.problemId}
            onDeleted={() => setView({ name: "list" })}
          />
        )}
        {view.name === "settings" && <SettingsPage />}
      </main>
    </div>
  );
}
