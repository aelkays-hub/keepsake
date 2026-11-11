// Name and find (ADR-040, step 3): name a face in a photo, then find the
// other photos that person is in.
//
// 1. The faces in the photo (it is searched first if it has not been).
// 2. Pick one (one face: it is picked) and type the name; names used
//    before are offered, so a second photo of someone adds to them.
// 3. Find: photos not searched yet are searched in the background; then the
//    faces most like that person's are suggested.
// 4. Review: the suggestions, best first, by year. Click one that is not
//    them; Confirm confirms the rest. Find more looks again with the faces
//    just confirmed: each time it reaches a few years further. Looser /
//    Stricter changes how alike a face must be (kept for next time).
// 5. Check <name>'s faces: their confirmed faces, least alike first, to
//    take mistakes off.
// Nothing is named without the user. Everything stays on this computer.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  api,
  fileUrl,
  type FaceDto,
  type FacesStatus,
  type PersonDto,
  type SuggestDto,
} from "../api";
import { byYear, cropStyle, searchLine } from "../faces";
import * as fmt from "../format";
import { useUi } from "../state";

type Phase = "faces" | "searching" | "review" | "done" | "check";

/** Faces shown at once in Check. */
const CHECK_MAX = 300;

/** One face, cut out of its photo's thumbnail. */
function FaceCrop({ face, width, label }: { face: FaceDto; width: number; label: string }) {
  const src = fileUrl(face.thumb);
  return (
    <span
      className="face-crop"
      role="img"
      aria-label={label}
      style={{
        ...cropStyle(face, width),
        backgroundImage: src ? `url("${src}")` : undefined,
      }}
    />
  );
}

export function NameFindWindow() {
  const { s, dispatch } = useUi();
  const job = s.nameFind;
  const [phase, setPhase] = useState<Phase>("faces");
  const [faces, setFaces] = useState<FaceDto[] | null>(null);
  const [searched, setSearched] = useState(false);
  const [picked, setPicked] = useState<number | null>(null);
  const [name, setName] = useState("");
  const [people, setPeople] = useState<PersonDto[]>([]);
  const [person, setPerson] = useState<PersonDto | null>(null);
  const [status, setStatus] = useState<FacesStatus | null>(null);
  const [found, setFound] = useState<SuggestDto | null>(null);
  const [review, setReview] = useState<FaceDto[]>([]);
  const [rejected, setRejected] = useState<Set<number>>(new Set());
  const [note, setNote] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** The cut-off, in percent (Looser / Stricter). */
  const [matchPct, setMatchPct] = useState<number | null>(null);
  const matchTimer = useRef(0);
  const close = useCallback(() => dispatch({ t: "nameFind", photo: null }), [dispatch]);

  /** Check <name>'s faces: their confirmed faces, least alike first. */
  const loadCheck = useCallback(async (who: PersonDto) => {
    setBusy(true);
    setProblem(null);
    try {
      const list = await api.facesCheck(who.id);
      setReview(list);
      setRejected(new Set());
      setPhase("check");
    } catch (e) {
      setProblem(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  // A new photo: start again.
  useEffect(() => {
    setPhase(job?.check ? "check" : "faces");
    setFaces(null);
    setSearched(false);
    setPicked(null);
    setName("");
    setPerson(null);
    setStatus(null);
    setFound(null);
    setReview([]);
    setRejected(new Set());
    setNote(null);
    setProblem(null);
    setBusy(false);
    if (job)
      api
        .facesPeople()
        .then(setPeople)
        .catch(() => setPeople([]));
    if (job?.check) {
      const who = { id: job.check.id, name: job.check.name, confirmed: 0 };
      setPerson(who);
      void loadCheck(who);
    }
  }, [job, loadCheck]);

  // The photo's faces, asked again until it has been searched.
  useEffect(() => {
    if (!job || phase !== "faces" || searched) return;
    let live = true;
    let timer = 0;
    const look = () =>
      api
        .facesOfPhoto(job.photo)
        .then((r) => {
          if (!live) return;
          setFaces(r.faces);
          setSearched(r.searched);
          if (r.faces.length > 0) {
            const asked = r.faces.find((f) => f.id === job.face);
            setPicked((p) => p ?? (asked ?? r.faces.find((f) => !f.person) ?? r.faces[0]).id);
          }
          if (!r.searched) timer = window.setTimeout(look, 1500);
        })
        .catch((e) => live && setProblem(String(e)));
    look();
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [job, phase, searched]);

  // The picked face's name, if it has one.
  useEffect(() => {
    const f = faces?.find((x) => x.id === picked);
    if (f?.person) setName(f.person);
  }, [faces, picked]);

  const suggest = useCallback(
    async (who: PersonDto, pct: number | null = null) => {
      setBusy(true);
      setProblem(null);
      try {
        const r = await api.facesSuggest(who.id, pct);
        setFound(r);
        setMatchPct(r.matchPct);
        const list = await api.facesReview(who.id);
        setReview(list);
        setRejected(new Set());
        setPhase(list.length > 0 ? "review" : "done");
        if (list.length === 0)
          setNote(
            `No more faces like ${who.name}'s were found` +
              (r.fromYear !== null
                ? ` (photos from ${r.fromYear} to ${r.toYear} looked at).`
                : "."),
          );
      } catch (e) {
        setProblem(String(e));
      } finally {
        setBusy(false);
      }
    },
    [setPhase],
  );

  // Searching: follow it; when every photo is searched, suggest.
  useEffect(() => {
    if (phase !== "searching" || !person) return;
    let live = true;
    let timer = 0;
    const poll = () =>
      api
        .facesStatus()
        .then((st) => {
          if (!live) return;
          setStatus(st);
          if (st.pending > 0 && !st.problem) timer = window.setTimeout(poll, 2000);
          else if (!st.problem) void suggest(person);
        })
        .catch((e) => live && setProblem(String(e)));
    poll();
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [phase, person, suggest]);

  const groups = useMemo(() => byYear(review), [review]);
  if (!job) return null;

  const start = async () => {
    if (picked === null || !name.trim()) return;
    setBusy(true);
    setProblem(null);
    try {
      const who = await api.facesName(picked, name.trim());
      setPerson(who);
      dispatch({ t: "edited" });
      await api.facesFind();
      setPhase("searching");
    } catch (e) {
      setProblem(String(e));
    } finally {
      setBusy(false);
    }
  };

  const confirm = async () => {
    if (!person) return;
    setBusy(true);
    setProblem(null);
    try {
      const yes = review.filter((f) => !rejected.has(f.id)).map((f) => f.id);
      const [c, r] = await api.facesDecide(person.id, yes, [...rejected]);
      dispatch({ t: "edited" });
      setNote(
        `${fmt.plural(c, "face")} confirmed as ${person.name}` +
          (r ? `; ${fmt.plural(r, "face")} marked as not ${person.name}.` : "."),
      );
      setReview([]);
      setPhase("done");
    } catch (e) {
      setProblem(String(e));
    } finally {
      setBusy(false);
    }
  };

  /** Check: take the faces clicked off the person. */
  const removeChecked = async () => {
    if (!person) return;
    setBusy(true);
    setProblem(null);
    try {
      const [, r] = await api.facesDecide(person.id, [], [...rejected]);
      dispatch({ t: "edited" });
      setNote(`${fmt.plural(r, "face")} taken off ${person.name}.`);
      await loadCheck(person);
    } catch (e) {
      setProblem(String(e));
      setBusy(false);
    }
  };

  /** Looser / Stricter: suggest again after a short pause. */
  const changeMatch = (pct: number) => {
    setMatchPct(pct);
    window.clearTimeout(matchTimer.current);
    if (person) matchTimer.current = window.setTimeout(() => void suggest(person, pct), 300);
  };

  const toggle = (id: number) =>
    setRejected((r) => {
      const n = new Set(r);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const who = person?.name ?? name.trim();
  return (
    <div
      className="modal-back"
      onMouseDown={(e) => e.target === e.currentTarget && !busy && close()}
    >
      <div
        className="modal name-find"
        role="dialog"
        aria-modal="true"
        aria-labelledby="nf-title"
        data-testid="name-find"
        onKeyDown={(e) => {
          if (e.key === "Escape" && !busy) close();
          e.stopPropagation();
        }}
      >
        <h2 id="nf-title">
          {phase === "faces"
            ? `Name and find: ${job.title}`
            : phase === "check"
              ? `Check ${who}'s faces`
              : `Finding ${who}`}
        </h2>

        {phase === "faces" && (
          <>
            {!searched && (
              <p role="status" data-testid="nf-looking">
                <span className="spinner" aria-hidden={true} /> Looking for faces in this photo…
              </p>
            )}
            {searched && faces?.length === 0 && (
              <p className="warn" data-testid="nf-none">
                No faces were found in this photo. (Very small or turned-away faces are not found.)
              </p>
            )}
            {faces && faces.length > 0 && (
              <>
                <p className="muted">
                  {faces.length === 1 ? "The face in this photo:" : "Pick a face, then name it:"}
                </p>
                <div className="face-row" role="radiogroup" aria-label="Faces in this photo">
                  {faces.map((f, i) => (
                    <button
                      key={f.id}
                      role="radio"
                      aria-checked={picked === f.id}
                      className="face-pick"
                      onClick={() => setPicked(f.id)}
                      data-testid="nf-face"
                    >
                      <FaceCrop face={f} width={88} label={f.person ?? `Face ${i + 1}`} />
                      <span className="face-name">{f.person ?? `Face ${i + 1}`}</span>
                    </button>
                  ))}
                </div>
                <label className="row">
                  Name{" "}
                  <input
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void start();
                    }}
                    list="nf-people"
                    placeholder="Who is it?"
                    autoFocus
                    data-testid="nf-name"
                  />
                  <datalist id="nf-people">
                    {people.map((p) => (
                      <option key={p.id} value={p.name} />
                    ))}
                  </datalist>
                </label>
                <p className="muted">
                  Then every photo is looked at for this person, on this computer. You check what is
                  found before anything is named.
                </p>
              </>
            )}
          </>
        )}

        {phase === "searching" && (
          <>
            <progress
              max={status?.photos || 1}
              value={status?.scanned ?? 0}
              aria-label="Photos searched"
            />
            <p role="status" data-testid="nf-status">
              {searchLine(status)}
            </p>
          </>
        )}

        {phase === "review" && (
          <>
            <p role="status" data-testid="nf-found">
              {fmt.plural(review.length, "face")} that may be {who}, best first
              {found?.fromYear != null && ` (photos from ${found.fromYear} to ${found.toYear})`}.
              Click any that is not {who}, then Confirm.
            </p>
            <div className="face-review" data-testid="nf-review">
              {groups.map(([year, list]) => (
                <section key={year} aria-label={year}>
                  <h3>{year}</h3>
                  <div className="face-row">
                    {list.map((f) => {
                      const no = rejected.has(f.id);
                      return (
                        <button
                          key={f.id}
                          className={`face-pick${no ? " rejected" : ""}`}
                          aria-pressed={no}
                          title={`${f.date ?? "No date"} · ${f.likeness ?? 0}% alike${no ? ` · not ${who}` : ""}`}
                          onClick={() => toggle(f.id)}
                          data-testid="nf-suggestion"
                        >
                          <FaceCrop
                            face={f}
                            width={72}
                            label={`${no ? "Not " : ""}${who}? ${f.date ?? "no date"}`}
                          />
                          {no && (
                            <span className="face-no" aria-hidden={true}>
                              ✕
                            </span>
                          )}
                        </button>
                      );
                    })}
                  </div>
                </section>
              ))}
            </div>
          </>
        )}

        {phase === "review" && (
          <label className="row nf-match">
            Looser
            <input
              type="range"
              min={40}
              max={75}
              step={5}
              value={matchPct ?? 55}
              onChange={(e) => changeMatch(Number(e.target.value))}
              aria-label="How alike a face must be to be suggested"
              data-testid="nf-match"
            />
            Stricter
          </label>
        )}

        {phase === "check" && (
          <>
            <p role="status" data-testid="nf-check-status">
              {review.length === 0
                ? `No faces confirmed as ${who} yet.`
                : `${fmt.plural(review.length, "face")} confirmed as ${who}, least alike first. ` +
                  `Click any that is not ${who}, then Remove.`}
            </p>
            {note && (
              <p className="muted" data-testid="nf-note">
                {note}
              </p>
            )}
            <div className="face-review" data-testid="nf-check">
              <div className="face-row">
                {review.slice(0, CHECK_MAX).map((f) => {
                  const no = rejected.has(f.id);
                  return (
                    <button
                      key={f.id}
                      className={`face-pick${no ? " rejected" : ""}`}
                      aria-pressed={no}
                      title={`${f.date ?? "No date"} · ${f.likeness ?? 0}% alike${no ? ` · not ${who}` : ""}`}
                      onClick={() => toggle(f.id)}
                      data-testid="nf-checked"
                    >
                      <FaceCrop
                        face={f}
                        width={72}
                        label={`${no ? "Not " : ""}${who}? ${f.likeness ?? 0}% alike`}
                      />
                      <span className="face-name">{f.likeness ?? 0}%</span>
                      {no && (
                        <span className="face-no" aria-hidden={true}>
                          ✕
                        </span>
                      )}
                    </button>
                  );
                })}
              </div>
              {review.length > CHECK_MAX && (
                <p className="muted">
                  And {fmt.count(review.length - CHECK_MAX)} more, more alike.
                </p>
              )}
            </div>
          </>
        )}

        {phase === "done" && note && (
          <p role="status" data-testid="nf-note">
            {note}
          </p>
        )}

        {problem && (
          <p className="error" role="alert">
            {problem}
          </p>
        )}

        <footer className="modal-foot">
          {phase === "faces" && faces && faces.length > 0 && (
            <button
              className="primary"
              disabled={busy || picked === null || !name.trim()}
              onClick={() => void start()}
              data-testid="nf-start"
            >
              Name and find
            </button>
          )}
          {phase === "searching" && person && (
            <button disabled={busy} onClick={() => void suggest(person)} data-testid="nf-so-far">
              Show matches so far
            </button>
          )}
          {phase === "review" && (
            <button
              className="primary"
              disabled={busy}
              onClick={() => void confirm()}
              data-testid="nf-confirm"
            >
              Confirm {fmt.plural(review.length - rejected.size, "face")}
            </button>
          )}
          {phase === "done" && person && (
            <button
              className="primary"
              disabled={busy}
              onClick={() => void suggest(person)}
              data-testid="nf-more"
            >
              Find more
            </button>
          )}
          {phase === "done" && person && (
            <button
              disabled={busy}
              onClick={() => void loadCheck(person)}
              data-testid="nf-check-open"
            >
              Check {person.name}&apos;s faces
            </button>
          )}
          {phase === "check" && person && (
            <button
              className="primary"
              disabled={busy || rejected.size === 0}
              onClick={() => void removeChecked()}
              data-testid="nf-remove"
            >
              {rejected.size === 0
                ? "Remove"
                : `Remove ${fmt.plural(rejected.size, "face")} from ${person.name}`}
            </button>
          )}
          <button onClick={close} disabled={busy} data-testid="nf-close">
            {phase === "faces" ? "Cancel" : "Close"}
          </button>
        </footer>
      </div>
    </div>
  );
}
