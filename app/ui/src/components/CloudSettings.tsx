// Settings → Tag with AI (ADR-036, ADR-038): for Claude, ChatGPT and
// Gemini, the API key and the model. Keys are kept in Windows Credential
// Manager or the macOS Keychain, never shown again once saved. Photos are
// sent only when the user picks them.

import { useEffect, useState } from "react";

import { api, type CloudInfo, type CloudProviderDto } from "../api";
import { costPerPhoto, help } from "../cloud";

export function CloudSettings() {
  const [info, setInfo] = useState<CloudInfo | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    api
      .aiCloudInfo()
      .then(setInfo)
      .catch((e) => setProblem(String(e)));
  }, []);
  if (!info)
    return problem ? (
      <p className="error" role="alert">
        {problem}
      </p>
    ) : null;
  return (
    <div className="card" data-testid="cloud-settings">
      <h2>Tag with AI</h2>
      <p className="muted">
        For the photos you pick (right-click them and choose Tag with AI), Keepsake sends a
        1024-pixel copy of each to the AI you choose: Claude, ChatGPT or Gemini. It tags them more
        precisely than the on-device AI and can add tags of its own (under AI). The tags become your
        keywords. Your originals are never sent or changed. Each AI needs your own API key, and its
        company charges you for each photo.
      </p>
      {info.providers.map((p) => (
        <Provider key={p.id} p={p} onInfo={setInfo} />
      ))}
    </div>
  );
}

function Provider({ p, onInfo }: { p: CloudProviderDto; onInfo: (i: CloudInfo) => void }) {
  const [key, setKey] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const h = help(p.id);
  const run = (call: Promise<CloudInfo>, said: string) => {
    setProblem(null);
    setSaved(null);
    call
      .then((x) => {
        onInfo(x);
        setKey("");
        setSaved(said);
      })
      .catch((e) => setProblem(String(e)));
  };
  return (
    <section className="cloud-provider" aria-label={p.name} data-testid={`cloud-${p.id}`}>
      <h3>
        {p.name} <span className="muted">({p.company})</span>
      </h3>
      <label>
        Model{" "}
        <select
          value={p.model}
          onChange={(e) => run(api.aiCloudSetModel(e.target.value), "Saved.")}
          data-testid="cloud-model"
        >
          {p.models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name} ({costPerPhoto(m.centsPerPhoto)})
            </option>
          ))}
        </select>
      </label>
      <p role="status" data-testid="cloud-key-state">
        {p.hasKey
          ? "An API key is saved in this computer's credential store."
          : `No API key is saved yet. Get one at ${h.site}.`}
      </p>
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          run(api.aiCloudSetKey(p.id, key), "The key is saved.");
        }}
      >
        <input
          type="password"
          autoComplete="off"
          spellCheck={false}
          placeholder={p.hasKey ? "Paste a new key to replace it" : h.placeholder}
          aria-label={`${p.name} API key`}
          value={key}
          onChange={(e) => setKey(e.target.value)}
          data-testid="cloud-key"
        />
        <button type="submit" disabled={!key.trim()} data-testid="cloud-key-save">
          Save key
        </button>
        {p.hasKey && (
          <button
            type="button"
            onClick={() => run(api.aiCloudSetKey(p.id, null), "The key is removed.")}
            data-testid="cloud-key-remove"
          >
            Remove key
          </button>
        )}
      </form>
      {h.note && (
        <p className="muted" data-testid="cloud-note">
          {h.note}
        </p>
      )}
      {saved && (
        <p className="muted" data-testid="cloud-saved">
          {saved}
        </p>
      )}
      {problem && (
        <p className="error" role="alert">
          {problem}
        </p>
      )}
    </section>
  );
}
