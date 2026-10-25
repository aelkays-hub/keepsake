import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import { ImportWindow } from "./import/ImportWindow";
import "./index.css";

async function boot() {
  // Browser tests (and `pnpm dev:mock`) run against an in-memory backend.
  if (import.meta.env.VITE_KEEPSAKE_MOCK === "1") {
    const { installMock } = await import("./mock/backend");
    installMock();
  }
  const isImport = window.location.hash.startsWith("#/import");
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>{isImport ? <ImportWindow /> : <App />}</React.StrictMode>,
  );
}

void boot();
