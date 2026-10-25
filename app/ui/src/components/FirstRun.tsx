// First run: create or open a library.

import { useState } from "react";

import { api, pickFolder, type AppInfo } from "../api";

export function FirstRun({ onReady, uiTest }: { onReady: (i: AppInfo) => void; uiTest: boolean }) {
  const [path, setPath] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const go = (f: (p: string) => Promise<AppInfo>) => {
    if (!path.trim()) return;
    setBusy(true);
    setError(null);
    f(path.trim())
      .then(onReady)
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  };
  return (
    <main className="firstrun">
      <h1>Welcome to Keepsake</h1>
      <p>
        A library is a folder where Keepsake keeps its catalog. Your photos and videos stay where
        you import them, and are never changed.
      </p>
      <div className="row">
        <input
          aria-label="Library folder"
          placeholder="Folder for the library, e.g. D:\Keepsake"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          data-testid="library-path"
        />
        {!uiTest && (
          <button
            onClick={() =>
              pickFolder("Choose a folder for the library").then((p) => {
                if (p) setPath(p);
              })
            }
          >
            Browse…
          </button>
        )}
      </div>
      <div className="row">
        <button
          className="primary"
          disabled={busy || !path.trim()}
          onClick={() => go(api.libraryCreate)}
          data-testid="create-library"
        >
          Create or open library
        </button>
      </div>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </main>
  );
}
