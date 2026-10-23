import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import SpikePlayback from "./SpikePlayback";

function App() {
  const [version, setVersion] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hash, setHash] = useState(window.location.hash);

  useEffect(() => {
    const onHashChange = () => setHash(window.location.hash);
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  useEffect(() => {
    invoke<string>("get_version")
      .then(setVersion)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  if (hash === "#spike") {
    return <SpikePlayback />;
  }

  return (
    <main className="placeholder">
      <h1>Keepsake</h1>
      {version !== null && <p className="version">v{version}</p>}
      {error !== null && <p className="error">Could not load version: {error}</p>}
      <p className="subtitle">Your local-first photo and video library.</p>
      <p style={{ fontSize: 11, marginTop: 24, color: "#666" }}>
        <a href="#spike" style={{ color: "#888" }}>
          spike →
        </a>
      </p>
    </main>
  );
}

export default App;
