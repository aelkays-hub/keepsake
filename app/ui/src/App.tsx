import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

function App() {
  const [version, setVersion] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<string>("get_version")
      .then(setVersion)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  return (
    <main className="placeholder">
      <h1>Keepsake</h1>
      {version !== null && <p className="version">v{version}</p>}
      {error !== null && <p className="error">Could not load version: {error}</p>}
      <p className="subtitle">Your local-first photo and video library.</p>
    </main>
  );
}

export default App;
