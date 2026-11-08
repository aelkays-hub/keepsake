// The inspector's People for one photo (ADR-040, step 5): who is named in
// it, how many faces have no name yet, and Name and find…. It never starts
// a search: a photo not searched yet says so. Hidden when Recognise faces
// is off.

import { useEffect, useState } from "react";

import { api, type FaceDto } from "../api";
import * as fmt from "../format";
import { useUi } from "../state";

export function PeopleBlock({ id, title }: { id: string; title: string }) {
  const { s, dispatch } = useUi();
  const [faces, setFaces] = useState<FaceDto[] | null>(null);
  const [searched, setSearched] = useState(false);
  const [off, setOff] = useState(false);
  useEffect(() => {
    let live = true;
    api
      .facesOfPhoto(id, false)
      .then((r) => {
        if (!live) return;
        setFaces(r.faces);
        setSearched(r.searched);
        setOff(false);
      })
      .catch(() => live && setOff(true));
    return () => {
      live = false;
    };
  }, [id, s.revision]);
  if (off || !faces) return null;
  const named = [
    ...new Set(faces.filter((f) => f.state === "confirmed" && f.person).map((f) => f.person!)),
  ];
  const unnamed = faces.filter((f) => !f.person).length;
  return (
    <div className="block" data-testid="people">
      People
      {named.length > 0 && (
        <ul className="chips" aria-label="People in this photo">
          {named.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
      <p className="muted" data-testid="people-note">
        {!searched
          ? "Not searched for faces yet."
          : faces.length === 0
            ? "No faces found."
            : unnamed > 0
              ? `${fmt.plural(unnamed, "face")} with no name.`
              : named.length === 0
                ? "No one named yet."
                : ""}
      </p>
      <button
        onClick={() => dispatch({ t: "nameFind", photo: { photo: id, title } })}
        title="Name a person in this photo and find the other photos they are in"
        data-testid="people-name-find"
      >
        Name and find…
      </button>
    </div>
  );
}
