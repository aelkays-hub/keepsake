// Settings → Tag with Claude (ADR-036): the API key and the model. The key
// is kept in Windows Credential Manager or the macOS Keychain, never shown
// again once saved. Photos are sent only when the user picks them.

import { useEffect, useState } from "react";

import { api, type ClaudeInfo } from "../api";
import { costPerPhoto } from "../claude";

export function ClaudeSettings() {
  const [info, setInfo] = useState<ClaudeInfo | null>(null);
  const [key, setKey] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  useEffect(() => {
    api
      .aiClaudeInfo()
      .then(setInfo)
      .catch((e) => setProblem(String(e)));
  }, []);
  const run = (p: Promise<ClaudeInfo>, said: string) => {
    setProblem(null);
    setSaved(null);
    p.then((x) => {
      setInfo(x);
      setKey("");
      setSaved(said);
    }).catch((e) => setProblem(String(e)));
  };
  if (!info) return null;
  return (
    <div className="card" data-testid="claude-settings">
      <h2>Tag with Claude</h2>
      <p className="muted">
        For the photos you pick (right-click them and choose Tag with Claude), Keepsake sends a
        1024-pixel copy of each to Anthropic&apos;s Claude, which tags it more precisely than the
        on-device AI and can add tags of its own (under AI). The tags become your keywords. Your
        originals are never sent or changed. You need your own API key from console.anthropic.com;
        Anthropic charges you for each photo.
      </p>
      <label>
        Model{" "}
        <select
          value={info.model}
          onChange={(e) => run(api.aiClaudeSetModel(e.target.value), "Saved.")}
          data-testid="claude-model"
        >
          {info.models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name} ({costPerPhoto(m.centsPerPhoto)})
            </option>
          ))}
        </select>
      </label>
      <p role="status" data-testid="claude-key-state">
        {info.hasKey
          ? "An API key is saved in this computer's credential store."
          : "No API key is saved yet."}
      </p>
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          run(api.aiClaudeSetKey(key), "The key is saved.");
        }}
      >
        <input
          type="password"
          autoComplete="off"
          spellCheck={false}
          placeholder={info.hasKey ? "Paste a new key to replace it" : "sk-ant-…"}
          aria-label="Claude API key"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          data-testid="claude-key"
        />
        <button type="submit" disabled={!key.trim()} data-testid="claude-key-save">
          Save key
        </button>
        {info.hasKey && (
          <button
            type="button"
            onClick={() => run(api.aiClaudeSetKey(null), "The key is removed.")}
            data-testid="claude-key-remove"
          >
            Remove key
          </button>
        )}
      </form>
      {saved && (
        <p className="muted" data-testid="claude-saved">
          {saved}
        </p>
      )}
      {problem && (
        <p className="error" role="alert">
          {problem}
        </p>
      )}
    </div>
  );
}
