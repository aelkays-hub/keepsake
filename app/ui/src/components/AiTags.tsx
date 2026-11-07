// The inspector's AI tags for one item: what the on-device AI thinks the
// photo shows. ✕ removes a tag for good (it is not given again); ✓ makes
// it your own keyword. Searching finds AI tags like any keyword. When there
// are none, it says why: AI is off, the item is not analysed yet, or the
// AI found nothing it was sure of.

import { useEffect, useState } from "react";

import { api, type AiTagsDto } from "../api";
import { useUi } from "../state";

export function AiTags({ id }: { id: string }) {
  const { s, dispatch } = useUi();
  const [info, setInfo] = useState<AiTagsDto | null>(null);
  useEffect(() => {
    let live = true;
    api
      .aiTags(id)
      .then((t) => live && setInfo(t))
      .catch(() => live && setInfo(null));
    return () => {
      live = false;
    };
  }, [id, s.revision]);
  if (!info) return null;
  const decide = (tag: string, keep: boolean) =>
    api
      .aiTagDecide(id, tag, keep)
      .then(() => dispatch({ t: "edited" }))
      .catch(console.error);
  return (
    <div className="block" data-testid="ai-tags">
      AI tags
      {info.tags.length > 0 ? (
        <ul className="chips ai" aria-label="AI tags">
          {info.tags.map((t) => (
            <li key={t.tag} title={`${t.confidence}% sure`}>
              {t.tag}
              <button aria-label={`Keep ${t.tag} as a keyword`} onClick={() => decide(t.tag, true)}>
                ✓
              </button>
              <button aria-label={`Remove AI tag ${t.tag}`} onClick={() => decide(t.tag, false)}>
                ✕
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="muted" data-testid="ai-tags-none">
          {!info.enabled && !info.analysed
            ? "On-device AI is off (Settings)."
            : !info.analysed
              ? "Not analysed yet: right-click its date in the calendar and choose Find duplicates and tags."
              : "None: the AI was not sure what this shows."}
        </p>
      )}
    </div>
  );
}
