// The inspector's AI tags for one item: what the on-device AI thinks the
// photo shows. ✕ removes a tag for good (it is not given again); ✓ makes
// it your own keyword. Searching finds AI tags like any keyword.

import { useEffect, useState } from "react";

import { api, type AiTagDto } from "../api";
import { useUi } from "../state";

export function AiTags({ id }: { id: string }) {
  const { s, dispatch } = useUi();
  const [tags, setTags] = useState<AiTagDto[]>([]);
  useEffect(() => {
    let live = true;
    api
      .aiTags(id)
      .then((t) => live && setTags(t))
      .catch(() => live && setTags([]));
    return () => {
      live = false;
    };
  }, [id, s.revision]);
  if (tags.length === 0) return null;
  const decide = (tag: string, keep: boolean) =>
    api
      .aiTagDecide(id, tag, keep)
      .then(() => dispatch({ t: "edited" }))
      .catch(console.error);
  return (
    <div className="block" data-testid="ai-tags">
      AI tags
      <ul className="chips ai" aria-label="AI tags">
        {tags.map((t) => (
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
    </div>
  );
}
