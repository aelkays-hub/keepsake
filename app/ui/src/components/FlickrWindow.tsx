// Upload to Flickr (ADR-046): right-click photos → Upload to Flickr…. The
// user chooses who may see them (public, friends and family, friends,
// family or private) and checks each photo's tags (its keywords, and its
// camera and lens) before anything is sent. The uploads run in the
// background, one photo at a time; this window shows how each went and
// links to it on Flickr. A photo already uploaded says so and is not
// ticked.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { api, fileUrl } from "../api";
import type { FlickrItemDto } from "../bindings/FlickrItemDto";
import type { FlickrPrepareDto } from "../bindings/FlickrPrepareDto";
import type { FlickrProgressDto } from "../bindings/FlickrProgressDto";
import {
  addTags,
  DEFAULT_PRIVACY,
  finalTags,
  isMachineTag,
  noEdits,
  parseTags,
  PRIVACY,
  privacyLabel,
  removeTag,
  type PrivacyId,
  type TagEdits,
} from "../flickr";
import { useUi } from "../state";

const POLL_MS = 1500;

interface Options {
  camera: boolean;
  machine: boolean;
  parents: boolean;
}

export function FlickrWindow() {
  const { s, dispatch } = useUi();
  const job = s.flickr;
  const [opts, setOpts] = useState<Options>({ camera: true, machine: false, parents: false });
  const [data, setData] = useState<FlickrPrepareDto | null>(null);
  const [privacy, setPrivacy] = useState<PrivacyId>(DEFAULT_PRIVACY);
  const [ticked, setTicked] = useState<Set<string>>(new Set());
  const [edits, setEdits] = useState<Map<string, TagEdits>>(new Map());
  const [titles, setTitles] = useState<Map<string, string>>(new Map());
  const [forAll, setForAll] = useState("");
  const [sent, setSent] = useState<string[] | null>(null);
  const [progress, setProgress] = useState<FlickrProgressDto | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const box = useRef<HTMLDivElement>(null);
  const primed = useRef(false);
  const close = useCallback(() => dispatch({ t: "flickr", job: null }), [dispatch]);

  // A new set of photos: start again.
  useEffect(() => {
    setData(null);
    setEdits(new Map());
    setTitles(new Map());
    setSent(null);
    setProgress(null);
    setProblem(null);
    setForAll("");
    setPrivacy(DEFAULT_PRIVACY);
    primed.current = false;
    if (job) box.current?.focus();
  }, [job]);

  // The photos and their suggested tags (again when an option changes).
  useEffect(() => {
    if (!job) return;
    let live = true;
    api
      .flickrPrepare(job.selection, opts)
      .then((r) => {
        if (!live) return;
        // Tick once: those not on Flickr yet (and not videos).
        if (!primed.current) {
          primed.current = true;
          setTicked(
            new Set(r.items.filter((i) => !i.video && i.uploads.length === 0).map((i) => i.id)),
          );
        }
        setData(r);
      })
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [job, opts]);

  // Uploading: how each photo is getting on.
  useEffect(() => {
    if (!sent) return;
    let live = true;
    api
      .flickrUploadStatus(sent)
      .then((r) => live && setProgress(r))
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [sent, tick]);
  const pending = progress?.pending ?? 1;
  useEffect(() => {
    if (!sent || pending === 0) return;
    const t = window.setTimeout(() => setTick((n) => n + 1), POLL_MS);
    return () => window.clearTimeout(t);
  }, [sent, pending, progress]);

  const items = useMemo(() => data?.items ?? [], [data]);
  const tagsOf = useCallback(
    (it: FlickrItemDto) => finalTags(it.tags, edits.get(it.id) ?? noEdits),
    [edits],
  );
  if (!job) return null;
  const status = data?.status;
  const ready = !!status?.hasApp && !!status?.signedIn;
  const chosen = items.filter((i) => ticked.has(i.id) && !i.video);
  const edit = (id: string, f: (e: TagEdits) => TagEdits) =>
    setEdits((m) => new Map(m).set(id, f(m.get(id) ?? noEdits)));

  const upload = () => {
    setProblem(null);
    const req = chosen.map((i) => ({
      id: i.id,
      title: titles.get(i.id) ?? i.title,
      description: i.description,
      tags: tagsOf(i),
    }));
    api
      .flickrUpload(req, privacy)
      .then(() => setSent(req.map((r) => r.id)))
      .catch((e) => setProblem(String(e)));
  };
  const stop = () => {
    if (!sent) return;
    api
      .flickrUploadStop(sent)
      .then(() => setTick((n) => n + 1))
      .catch((e) => setProblem(String(e)));
  };
  const open = (url: string) => api.flickrOpen(url).catch((e) => setProblem(String(e)));
  const progressOf = (id: string) => progress?.items.find((p) => p.id === id);
  const done = progress?.items.filter((p) => p.state === "done").length ?? 0;
  const failed = progress?.items.filter((p) => p.state === "failed").length ?? 0;

  return (
    <div className="modal-back" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        className="modal route-window flickr-window"
        role="dialog"
        aria-modal="true"
        aria-labelledby="fl-title"
        tabIndex={-1}
        ref={box}
        data-testid="flickr-window"
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          e.stopPropagation();
        }}
      >
        <h2 id="fl-title">Upload to Flickr: {job.title}</h2>
        {status && !status.hasApp && (
          <p className="error" role="alert" data-testid="fl-no-app">
            Add your Flickr API key and secret in Settings → Flickr first, then sign in.{" "}
            <button onClick={() => dispatch({ t: "screen", screen: "settings" })}>
              Open Settings
            </button>
          </p>
        )}
        {status && status.hasApp && !status.signedIn && (
          <p className="error" role="alert" data-testid="fl-signed-out">
            Sign in to Flickr in Settings → Flickr first.{" "}
            <button onClick={() => dispatch({ t: "screen", screen: "settings" })}>
              Open Settings
            </button>
          </p>
        )}
        {status?.signedIn && (
          <p className="muted" data-testid="fl-account">
            To the Flickr account of {status.username}. Each photo goes as it is (a RAW or HEIF
            photo as a full-size JPEG); your originals are not changed.
          </p>
        )}

        {!sent && (
          <>
            <fieldset className="fl-privacy" data-testid="fl-privacy">
              <legend>Who may see them on Flickr</legend>
              {PRIVACY.map((p) => (
                <label key={p.id} title={p.hint}>
                  <input
                    type="radio"
                    name="fl-privacy"
                    value={p.id}
                    checked={privacy === p.id}
                    onChange={() => setPrivacy(p.id)}
                    data-testid={`fl-privacy-${p.id}`}
                  />{" "}
                  {p.label}
                </label>
              ))}
            </fieldset>
            <div className="fl-options">
              <label>
                <input
                  type="checkbox"
                  checked={opts.camera}
                  onChange={(e) => setOpts({ ...opts, camera: e.target.checked })}
                  data-testid="fl-opt-camera"
                />{" "}
                Camera and lens as tags
              </label>
              <label title="aircraft:registration=C-FEJC, for keywords of three parts or more">
                <input
                  type="checkbox"
                  checked={opts.machine}
                  onChange={(e) => setOpts({ ...opts, machine: e.target.checked })}
                  data-testid="fl-opt-machine"
                />{" "}
                Machine tags too
              </label>
              <label title="Aircraft and Registration as tags too, for Aircraft|Registration|C-FEJC">
                <input
                  type="checkbox"
                  checked={opts.parents}
                  onChange={(e) => setOpts({ ...opts, parents: e.target.checked })}
                  data-testid="fl-opt-parents"
                />{" "}
                Parent keywords too
              </label>
            </div>
            <form
              className="row fl-for-all"
              onSubmit={(e) => {
                e.preventDefault();
                const t = parseTags(forAll);
                if (t.length === 0) return;
                for (const i of chosen) edit(i.id, (x) => addTags(x, t));
                setForAll("");
              }}
            >
              <input
                value={forAll}
                onChange={(e) => setForAll(e.target.value)}
                placeholder="Tags for every ticked photo, separated by commas"
                aria-label="Tags for every ticked photo"
                data-testid="fl-tag-all"
              />
              <button type="submit" disabled={!forAll.trim()} data-testid="fl-tag-all-add">
                Add to all
              </button>
            </form>
          </>
        )}

        <p role="status" data-testid="fl-progress">
          {!data
            ? problem
              ? ""
              : "Reading the photos…"
            : !sent
              ? `${chosen.length} of ${items.length} ticked, ${privacyLabel(privacy).toLowerCase()}.`
              : !progress
                ? "Starting…"
                : progress.pending > 0
                  ? `Uploading… ${done} of ${sent.length} done.`
                  : `Done: ${done} of ${sent.length} uploaded${failed ? `, ${failed} not` : ""}.`}
        </p>
        {problem && (
          <p className="error" role="alert" data-testid="fl-problem">
            {problem}
          </p>
        )}

        <ul className="rt-list">
          {items.map((it) => {
            const thumb = it.thumb ? fileUrl(it.thumb) : null;
            const pr = progressOf(it.id);
            const tags = tagsOf(it);
            const locked = !!sent;
            if (sent && !sent.includes(it.id)) return null;
            return (
              <li key={it.id} className="rt-photo" data-testid="fl-item">
                {thumb ? (
                  <img src={thumb} alt="" className="rt-thumb" loading="lazy" />
                ) : (
                  <span className="rt-thumb" aria-hidden={true} />
                )}
                <div className="rt-body">
                  <div className="rt-head">
                    {!locked && (
                      <input
                        type="checkbox"
                        checked={ticked.has(it.id)}
                        disabled={it.video}
                        aria-label={`Upload ${it.name}`}
                        onChange={(e) =>
                          setTicked((t) => {
                            const n = new Set(t);
                            if (e.target.checked) n.add(it.id);
                            else n.delete(it.id);
                            return n;
                          })
                        }
                        data-testid="fl-tick"
                      />
                    )}
                    <strong className="rt-name">{it.name}</strong>
                  </div>
                  {it.video && <p className="muted">Videos are not uploaded to Flickr.</p>}
                  {it.uploads.length > 0 && (
                    <p className="fl-already" data-testid="fl-already">
                      On Flickr already ({privacyLabel(it.uploads[0].privacy).toLowerCase()},{" "}
                      {it.uploads[0].date}
                      {it.uploads.length > 1 ? `, ${it.uploads.length} times` : ""}):{" "}
                      <button className="link" onClick={() => open(it.uploads[0].url)}>
                        open it
                      </button>
                      . Tick it to upload it again.
                    </p>
                  )}
                  {!it.video && (
                    <>
                      <label className="fl-title">
                        Title{" "}
                        <input
                          value={titles.get(it.id) ?? it.title}
                          disabled={locked}
                          onChange={(e) => setTitles((m) => new Map(m).set(it.id, e.target.value))}
                          data-testid="fl-item-title"
                        />
                      </label>
                      <ul className="chips" aria-label="Tags" data-testid="fl-tags">
                        {tags.map((t) => (
                          <li key={t} className={isMachineTag(t) ? "fl-machine" : undefined}>
                            {t}
                            {!locked && (
                              <button
                                aria-label={`Remove tag ${t}`}
                                onClick={() => edit(it.id, (x) => removeTag(x, t))}
                                data-testid="fl-tag-remove"
                              >
                                ×
                              </button>
                            )}
                          </li>
                        ))}
                        {tags.length === 0 && <li className="muted">No tags</li>}
                      </ul>
                      {!locked && <AddTag onAdd={(t) => edit(it.id, (x) => addTags(x, t))} />}
                    </>
                  )}
                  {pr && (
                    <p data-testid="fl-item-state" className={pr.state === "failed" ? "error" : ""}>
                      {pr.state === "waiting" && "Waiting…"}
                      {pr.state === "uploading" && "Uploading…"}
                      {pr.state === "retrying" && `Trying again shortly: ${pr.error ?? ""}`}
                      {pr.state === "failed" && `Not uploaded: ${pr.error ?? ""}`}
                      {pr.state === "none" && "Stopped."}
                      {pr.state === "done" && pr.url && (
                        <>
                          Uploaded.{" "}
                          <button
                            className="link"
                            onClick={() => open(pr.url ?? "")}
                            data-testid="fl-item-open"
                          >
                            Open on Flickr
                          </button>
                        </>
                      )}
                    </p>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
        <div className="modal-foot">
          {!sent ? (
            <button
              className="primary"
              onClick={upload}
              disabled={!ready || chosen.length === 0}
              data-testid="fl-upload"
            >
              Upload {chosen.length === 1 ? "1 photo" : `${chosen.length} photos`}
            </button>
          ) : (
            pending > 0 && (
              <button onClick={stop} data-testid="fl-stop">
                Stop
              </button>
            )
          )}
          <span className="grow" />
          <button onClick={close} data-testid="fl-close">
            {sent && pending > 0 ? "Close (uploads go on)" : "Close"}
          </button>
        </div>
      </div>
    </div>
  );
}

function AddTag({ onAdd }: { onAdd: (tags: string[]) => void }) {
  const [v, setV] = useState("");
  return (
    <form
      className="row fl-add"
      onSubmit={(e) => {
        e.preventDefault();
        const t = parseTags(v);
        if (t.length) onAdd(t);
        setV("");
      }}
    >
      <input
        value={v}
        onChange={(e) => setV(e.target.value)}
        placeholder="Add tags, separated by commas"
        aria-label="Add tags"
        data-testid="fl-tag-add"
      />
      <button type="submit" disabled={!v.trim()} data-testid="fl-tag-add-button">
        Add
      </button>
    </form>
  );
}
