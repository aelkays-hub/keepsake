// Settings → Flickr (ADR-046): the user's own Flickr API key and secret
// (a non-commercial key from Flickr's App Garden) and signing in. The key,
// secret and sign-in are kept in Windows Credential Manager or the macOS
// Keychain and never shown again once saved.

import { useEffect, useState } from "react";

import { api } from "../api";
import type { FlickrStatusDto } from "../bindings/FlickrStatusDto";

export function FlickrSettings() {
  const [st, setSt] = useState<FlickrStatusDto | null>(null);
  const [key, setKey] = useState("");
  const [secret, setSecret] = useState("");
  const [code, setCode] = useState("");
  const [signInUrl, setSignInUrl] = useState<string | null>(null);
  const [said, setSaid] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api
      .flickrStatus()
      .then(setSt)
      .catch((e) => setProblem(String(e)));
  }, []);
  const run = <T,>(call: Promise<T>, then: (v: T) => void) => {
    setProblem(null);
    setSaid(null);
    setBusy(true);
    call
      .then(then)
      .catch((e) => setProblem(String(e)))
      .finally(() => setBusy(false));
  };
  if (!st)
    return problem ? (
      <p className="error" role="alert">
        {problem}
      </p>
    ) : null;
  return (
    <div className="card" data-testid="flickr-settings">
      <h2>Flickr</h2>
      <p className="muted">
        Upload the photos you pick to your own Flickr account (right-click them and choose Upload to
        Flickr…). Keepsake needs your own Flickr API key: on flickr.com, open the App Garden, choose
        Create an App, then Apply for a Non-Commercial Key, and paste the key and secret here. They
        and your sign-in are kept in this computer&apos;s credential store, never in a file.
      </p>
      <p role="status" data-testid="flickr-state">
        {!st.hasApp
          ? "No API key is saved yet."
          : st.signedIn
            ? `Signed in as ${st.username ?? "?"}. ${st.uploaded === 1 ? "1 photo" : `${st.uploaded} photos`} uploaded from this library.`
            : "The API key is saved. Sign in to Flickr next."}
      </p>
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          run(api.flickrSetApp(key, secret), (s) => {
            setSt(s);
            setKey("");
            setSecret("");
            setSignInUrl(null);
            setSaid("The key and secret are saved. Sign in to Flickr next.");
          });
        }}
      >
        <input
          type="password"
          autoComplete="off"
          spellCheck={false}
          placeholder={st.hasApp ? "New key, to replace it" : "Key"}
          aria-label="Flickr API key"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          data-testid="flickr-key"
        />
        <input
          type="password"
          autoComplete="off"
          spellCheck={false}
          placeholder="Secret"
          aria-label="Flickr API secret"
          value={secret}
          onChange={(e) => setSecret(e.target.value)}
          data-testid="flickr-secret"
        />
        <button
          type="submit"
          disabled={busy || !key.trim() || !secret.trim()}
          data-testid="flickr-key-save"
        >
          Save key
        </button>
        {st.hasApp && (
          <button
            type="button"
            disabled={busy}
            onClick={() =>
              run(api.flickrSetApp("", ""), (s) => {
                setSt(s);
                setSignInUrl(null);
                setSaid("The key, secret and sign-in are removed from this computer.");
              })
            }
            data-testid="flickr-key-remove"
          >
            Remove key
          </button>
        )}
      </form>
      {st.hasApp && (
        <div className="row" style={{ marginTop: 8 }}>
          {!st.signedIn ? (
            <button
              disabled={busy}
              onClick={() =>
                run(api.flickrSignInStart(), (url) => {
                  setSignInUrl(url);
                  setSt({ ...st, awaitingCode: true });
                })
              }
              data-testid="flickr-sign-in"
            >
              Sign in to Flickr…
            </button>
          ) : (
            <>
              <button
                disabled={busy}
                onClick={() => run(api.flickrTest(), setSaid)}
                data-testid="flickr-test"
              >
                Test the sign-in
              </button>
              <button
                disabled={busy}
                onClick={() =>
                  run(api.flickrSignOut(), (s) => {
                    setSt(s);
                    setSaid(
                      "Signed out on this computer. Keepsake can be removed from your account on flickr.com too.",
                    );
                  })
                }
                data-testid="flickr-sign-out"
              >
                Sign out
              </button>
            </>
          )}
        </div>
      )}
      {st.hasApp && !st.signedIn && st.awaitingCode && (
        <form
          className="flickr-code"
          onSubmit={(e) => {
            e.preventDefault();
            run(api.flickrSignInFinish(code), (s) => {
              setSt(s);
              setCode("");
              setSignInUrl(null);
              setSaid(`Signed in as ${s.username ?? "?"}.`);
            });
          }}
        >
          <p className="muted">
            Flickr&apos;s page opened in your browser. Choose OK, I&apos;ll authorize it, then type
            the code Flickr shows here.
            {signInUrl && (
              <>
                {" "}
                If the page did not open, copy this address into your browser:{" "}
                <code data-testid="flickr-sign-in-url">{signInUrl}</code>
              </>
            )}
          </p>
          <div className="row">
            <input
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder="123-456-789"
              aria-label="The code Flickr showed"
              autoComplete="off"
              spellCheck={false}
              data-testid="flickr-code"
            />
            <button type="submit" disabled={busy || !code.trim()} data-testid="flickr-code-save">
              Finish signing in
            </button>
          </div>
        </form>
      )}
      {said && (
        <p className="muted" data-testid="flickr-said">
          {said}
        </p>
      )}
      {problem && (
        <p className="error" role="alert" data-testid="flickr-problem">
          {problem}
        </p>
      )}
    </div>
  );
}
