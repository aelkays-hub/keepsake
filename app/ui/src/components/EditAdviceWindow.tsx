// Edit suggestions (ADR-042): for one photo, what Keepsake measured on its
// preview and what to do about it in Photoshop and Topaz Photo, then (if
// the user asks) what Claude, ChatGPT or Gemini suggest on seeing it. The
// suggestions are kept, so opening this again is instant and costs
// nothing. Open in Photoshop / Topaz Photo opens the original, which
// Keepsake never changes.

import { useCallback, useEffect, useRef, useState } from "react";

import {
  api,
  type CloudInfo,
  type EditAdviceDto,
  type EditSuggestionDto,
  type OpenChoice,
} from "../api";
import {
  askChoice,
  askingLine,
  askLine,
  choiceFor,
  PROGRAM_NAME,
  type Program,
} from "../editAdvice";
import { useUi } from "../state";

/** How often to look again while the preview is made or an AI answers. */
const POLL_MS = 1500;

function SuggestionList({ list, testId }: { list: EditSuggestionDto[]; testId: string }) {
  return (
    <ol className="advice-list" data-testid={testId}>
      {list.map((s, i) => (
        <li key={i} data-testid="advice-item">
          <strong>{s.title}</strong>
          {s.why && <p className="muted">{s.why}</p>}
          {s.photoshop && (
            <p>
              <span className="advice-app">Photoshop:</span> {s.photoshop}
            </p>
          )}
          {s.topaz && (
            <p>
              <span className="advice-app">Topaz Photo:</span> {s.topaz}
            </p>
          )}
        </li>
      ))}
    </ol>
  );
}

export function EditAdviceWindow() {
  const { s, dispatch } = useUi();
  const job = s.editAdvice;
  const [advice, setAdvice] = useState<EditAdviceDto | null>(null);
  const [choices, setChoices] = useState<OpenChoice[]>([]);
  const [info, setInfo] = useState<CloudInfo | null>(null);
  const [provider, setProvider] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const close = useCallback(() => dispatch({ t: "editAdvice", photo: null }), [dispatch]);
  const id = job?.photo ?? null;
  const box = useRef<HTMLDivElement>(null);
  // Focus the window when it opens, so Esc closes it.
  useEffect(() => {
    if (id) box.current?.focus();
  }, [id]);

  const load = useCallback(
    (again: boolean) => {
      if (!id) return;
      setProblem(null);
      setBusy(true);
      api
        .editSuggestions(id, again)
        .then(setAdvice)
        .catch((e) => setProblem(String(e)))
        .finally(() => setBusy(false));
    },
    [id],
  );

  // A new photo: start again.
  useEffect(() => {
    setAdvice(null);
    setChoices([]);
    setNote(null);
    if (!id) return;
    load(false);
    let live = true;
    api
      .openChoices(id, "primary")
      .then((c) => live && setChoices(c))
      .catch(() => undefined);
    api
      .aiCloudInfo()
      .then((i) => {
        if (!live) return;
        setInfo(i);
        setProvider(askChoice(i));
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [id, load]);

  // Look again while the preview is made, or an AI is being asked.
  const waiting = advice?.waiting || advice?.asking?.pending;
  useEffect(() => {
    if (!id || !waiting) return;
    const t = window.setTimeout(() => {
      api
        .editSuggestions(id)
        .then(setAdvice)
        .catch((e) => setProblem(String(e)));
    }, POLL_MS);
    return () => window.clearTimeout(t);
  }, [id, waiting, advice]);

  if (!job) return null;

  const ask = () => {
    if (!provider || !id) return;
    setProblem(null);
    setBusy(true);
    api
      .editSuggestionsAsk(id, provider)
      .then(setAdvice)
      .catch((e) => setProblem(String(e)))
      .finally(() => setBusy(false));
  };
  const stop = () => {
    if (!id) return;
    api
      .editSuggestionsStop(id)
      .then(() => api.editSuggestions(id))
      .then(setAdvice)
      .catch((e) => setProblem(String(e)));
  };
  const openIn = (c: OpenChoice) => {
    setProblem(null);
    api
      .openExternal(job.photo, "primary", c.app)
      .then((app) => setNote(`Opened with ${app}`))
      .catch((e) => setProblem(String(e)));
  };

  const programs: { program: Program; choice: OpenChoice | null }[] = (
    ["photoshop", "topaz"] as Program[]
  ).map((program) => ({ program, choice: choiceFor(choices, program) }));
  const ready = info?.providers.filter((p) => p.hasKey) ?? [];
  const asking = advice?.asking ?? null;
  const ai = advice?.ai ?? null;

  return (
    <div className="modal-back" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        className="modal edit-advice"
        ref={box}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-labelledby="ea-title"
        data-testid="edit-advice"
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          e.stopPropagation();
        }}
      >
        <h2 id="ea-title">Edit suggestions: {job.title}</h2>

        {(!advice || advice.waiting) && !problem && (
          <p role="status" data-testid="ea-looking">
            <span className="spinner" aria-hidden={true} />{" "}
            {advice?.waiting ? "Making the preview first…" : "Measuring the photo…"}
          </p>
        )}

        {advice && !advice.waiting && (
          <>
            <p data-testid="ea-summary">{advice.summary}</p>
            {advice.suggestions.length > 0 && (
              <SuggestionList list={advice.suggestions} testId="ea-rules" />
            )}
            <details className="advice-measured">
              <summary>What Keepsake measured</summary>
              <dl data-testid="ea-measured">
                {advice.measured.map((m) => (
                  <div key={m.label} className="row">
                    <dt>{m.label}</dt>
                    <dd>{m.value}</dd>
                  </div>
                ))}
              </dl>
              <p className="muted">
                Measured on this computer from the photo&apos;s 1024-pixel preview, so fine noise
                and focus are only roughly judged. It cannot tell what the photo is of: a night
                scene is meant to be dark.
              </p>
            </details>

            <section className="advice-ai" aria-label="Ask an AI">
              {ai && (
                <>
                  <h3 data-testid="ea-ai-title">
                    From {ai.providerName}
                    {ai.model ? ` (${ai.model})` : ""}
                  </h3>
                  {ai.summary && <p data-testid="ea-ai-summary">{ai.summary}</p>}
                  {ai.suggestions.length > 0 ? (
                    <SuggestionList list={ai.suggestions} testId="ea-ai" />
                  ) : (
                    <p className="muted">{ai.providerName} suggested no edits.</p>
                  )}
                </>
              )}
              {asking && (
                <p
                  role="status"
                  className={asking.pending ? undefined : "error"}
                  data-testid="ea-asking"
                >
                  {asking.pending && <span className="spinner" aria-hidden={true} />}{" "}
                  {askingLine(asking)}
                </p>
              )}
              {asking?.pending ? (
                <button onClick={stop} data-testid="ea-stop">
                  Stop
                </button>
              ) : ready.length === 0 ? (
                <p className="muted" data-testid="ea-no-key">
                  For suggestions that take in what the photo shows (faces, things to remove, a
                  better crop), add an API key for Claude, ChatGPT or Gemini in Settings → Tag with
                  AI.
                </p>
              ) : (
                <div className="row">
                  {ready.length > 1 && (
                    <select
                      aria-label="AI to ask"
                      value={provider ?? ""}
                      onChange={(e) => setProvider(e.target.value)}
                      data-testid="ea-provider"
                    >
                      {ready.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                    </select>
                  )}
                  <button onClick={ask} disabled={busy || !provider} data-testid="ea-ask">
                    {`Ask ${info?.providers.find((p) => p.id === provider)?.name ?? "AI"}${ai ? " again" : ""}`}
                  </button>
                  {info && provider && (
                    <span className="muted" data-testid="ea-ask-line">
                      {askLine(info, provider)}
                    </span>
                  )}
                </div>
              )}
            </section>
          </>
        )}

        {problem && (
          <p className="error" role="alert" data-testid="ea-problem">
            {problem}
          </p>
        )}
        {note && (
          <p role="status" data-testid="ea-note">
            {note}
          </p>
        )}

        <div className="modal-foot">
          {programs.map(({ program, choice }) => (
            <button
              key={program}
              disabled={!choice}
              title={
                choice
                  ? choice.app
                  : `Add ${PROGRAM_NAME[program]} for this file type in Settings → Open in`
              }
              onClick={() => choice && openIn(choice)}
              data-testid={`ea-open-${program}`}
            >
              Open in {PROGRAM_NAME[program]}
            </button>
          ))}
          <button
            onClick={() => load(true)}
            disabled={busy || !advice || advice.waiting}
            title="Measure the photo again (an AI's answer to the old measurements is dropped)"
            data-testid="ea-again"
          >
            Measure again
          </button>
          <span className="grow" />
          <button onClick={close} data-testid="ea-close">
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
