# Manual Test Steps — Making every missing thumbnail without the app, Open in for photos on a network drive, Show in Explorer / Finder, Faces step 5 (People in the Inspector, faces in the viewer), Faces step 4 (names in the sidecars), Faces step 3 (Name and find), Faces step 2 (searching photos for faces), Faces step 1 (the face models), Which AI tagged it, Tag with AI to All, Tags in the search bar, Tag with AI (Claude, ChatGPT or Gemini), Tag the photos with no date, Helpers end with the app, On-device AI (tags and same-day duplicates), Selecting many items to edit, Faster thumbnails on a network drive, Report rows open their photos, A full cache keeps the thumbnails, Scan again one folder (right-click), Previous and next in the viewer, No "database is locked" during a backlog, Older video types in added folders, Start-up while the library opens, Library report, Background fingerprints for added folders, Adding a large folder (saved as it goes), Viewer information panel, Calendar without empty days, Faster zoom, Thumbnail sizes and the zoom slider, Automatic card import, Folder tree, All metadata, Calendar tree, Open in menu, and Add a folder in place

Test on **Windows 11** and **macOS 14+**. Paths below are Windows examples; on macOS use a folder such as `~/Pictures/2025`.

Start the app from source (`cd app/ui; pnpm tauri dev`), or from `scripts\keepsake.bat` on Windows. **Close any running Keepsake first.**

---

## Faces, step 5: People in the Inspector, faces in the viewer (ADR-040)

Automated (run here and in CI):
```
cargo test -p keepsake-core --test face_store_tests
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/people.spec.ts
```

Manual (both OSes), with Recognise faces on and someone named (step 3):
1. Select a photo of that person. **Expected:** the Inspector has **People** with their name, "N faces with no name." if others are in it, and **Name and find…**. A photo not searched yet says "Not searched for faces yet." and selecting it does not start a search. Videos have no People.
2. Double-click it, then press **P** (or the **Faces** button). **Expected:** a yellow box on each named face with the name under it (a suggestion shows "Name?"), a dashed white box on each face with no name. Zoom in and pan: the boxes stay on the faces and the names stay readable. **F** is still full screen.
3. Click (or right-click) a named box. **Expected:** a menu: **Name…**, **Not <name>**, **Not a face**. Choose **Not <name>**. **Expected:** the name goes from the box and from the Inspector, the People › <name> keyword goes from the photo, and Find more never suggests that face for them again.
4. Click a dashed box → **Not a face** (e.g. a face on a poster). **Expected:** the box goes and does not come back after Settings → Search all photos. (Limit: "not a face" is not written to the sidecar, so a catalog rebuild forgets it.)
5. Click a box → **Name…**. **Expected:** the Name and find window with that face picked.
6. Press **P** again. **Expected:** the boxes go.

---

## Faces, step 4: names in the sidecars and search (ADR-040)

Automated (run here and in CI):
```
cargo test -p keepsake-core --test xmp_tests --test face_store_tests --test rebuild_tests
```

Manual (both OSes):
1. Name a face (Name and find…) and confirm a few suggestions. **Expected:** each of those photos gets the keyword **People › Sara** (Inspector → Keywords), and the Tag list and the search bar's tags find Sara.
2. Open the photo's `.xmp` sidecar in a text editor (next to the photo, or in the library's mirror folder for a read-only drive). **Expected:** `mwg-rs:Regions` with a `mwg-rs:Name` Sara, `mwg-rs:Type` Face and an `mwg-rs:Area` (x, y the face's centre, w, h, normalized). The photo itself is not changed (its modified date stays).
3. Open the same photo in Lightroom Classic (or digiKam) after reading the metadata from the file. **Expected:** Sara's face region with her name.
4. In Lightroom, name another face on a photo (e.g. Bob) and save the metadata to the file. In Keepsake, Scan again that folder (or rebuild the catalog). **Expected:** Bob is one of the people, with that face confirmed; the Lightroom region is still there.
5. Reject a suggestion for Sara, then rebuild the catalog (`keepsake rebuild-catalog`). **Expected:** Sara and her confirmed faces come back; after Search all photos, the rejected face is not suggested for her again.
6. Remove Sara's only face from a photo (reject it in Review, or Delete all face data). **Expected:** the People › Sara keyword goes from that photo; a People › … keyword you typed yourself for someone who is not one of the people stays.
7. Settings → Delete all face data. **Expected:** the face regions Keepsake wrote are gone from the sidecars (other apps' regions stay), and the People keywords of its people are gone.

---

## Faces, step 3: Name and find (ADR-040)

Needs Settings → On-device AI and **Recognise faces** on. Names are kept in the catalog only for now (the sidecars come in the next step).

Automated (run here and in CI):
```
cargo test -p keepsake-core --test face_store_tests
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/name-find.spec.ts
```

Manual (both OSes), with your own photos of one person over several years:
1. Right-click a photo of that person → **Name and find…**. **Expected:** a window "Name and find: <file>", briefly "Looking for faces in this photo…" if it was not searched yet, then the faces in it, cut out (one face: already picked). A video's menu has the item greyed out; a photo with no faces says so.
2. Pick the face, type the name (e.g. Sara) and press **Name and find** (or Enter). **Expected:** "Finding Sara" and "Searching photos for faces: S of N done (W to go)…" while photos not searched yet are searched. **Show matches so far** shows what is found up to now; you can also close the window and come back later.
3. When every photo is searched: **Expected:** "F faces that may be Sara, best first (photos from Y1 to Y2)", grouped by year, newest year first. The years are about 3 either side of the photo you named.
4. Click any face that is not Sara. **Expected:** it fades with a red ✕ and the button says "Confirm F−1 faces". Press it. **Expected:** "F−1 faces confirmed as Sara; 1 face marked as not Sara."
5. Press **Find more**. **Expected:** new suggestions a few years further back (or on) than before; repeat. When there are no more: "No more faces like Sara's were found (photos from … to … looked at)." The faces you rejected never come back for Sara.
6. Right-click another photo of her → Name and find… and start typing "Sa". **Expected:** Sara is offered; naming the face Sara (any case) adds it to the same person.
7. In the viewer (double-click a photo), press **Name and find**. **Expected:** the same window for that photo.
8. Settings → Recognise faces. **Expected:** the people count includes Sara. Delete all face data forgets her too.
9. Originals are not changed (modified dates stay), and nothing goes over the network.

---

## Faces, step 2: searching photos for faces (ADR-040)

Names and Name and find come in the next step; this one finds and stores the faces. The first start upgrades the catalog (schema 13).

Automated (run here and in CI):
```
cargo test -p keepsake-core --test face_store_tests
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/faces.spec.ts
cd app/ui; pnpm test
```

Manual (both OSes):
1. Settings → **Recognise faces** with On-device AI **off**. **Expected:** "Needs On-device AI (just above)" and the tick-box cannot be ticked. Turn On-device AI on.
2. Tick **Recognise faces**. **Expected:** if the models are not installed, "Downloading the face models: … of 39 MB" with Cancel, then the box stays ticked. The status says "0 of N photos searched; no faces yet. On the graphics card (DirectML)" (macOS: Core ML).
3. Press **Search all N photos**. **Expected:** "Searching N photos…", a **Stop** button, and the counts going up every few seconds ("F faces in P of S photos searched (N photos in all); W waiting"). Photos without a thumbnail get one first. The app stays usable; on battery it pauses if Settings says so.
4. Press **Stop**. **Expected:** "Stopped."; the counts stay. Press **Search all** again. **Expected:** it continues with the photos not searched yet (it does not search the done ones again).
5. With the app closed, in a terminal: `keepsake ai faces status`. **Expected:** the model paths and the same counts. `keepsake ai faces scan` searches the rest from the terminal and ends with "Searched … photo(s); … face(s) in … photo(s) so far."
6. Press **Delete all face data…**. **Expected:** "Forget F faces and 0 people? … This cannot be undone." with **Delete all face data** (red) and **Cancel**. Cancel: nothing changes. Delete: "All face data was deleted." and "0 of N photos searched".
7. Untick **Recognise faces**, then **Delete the downloaded face models**. **Expected:** the two model files are gone from the cache folder (`%LOCALAPPDATA%\keepsake\models\` or `~/Library/Caches/keepsake/models/`); ticking again downloads them again.
8. Throughout: originals are not changed (their modified dates stay), and no network traffic other than the one-time model download.

---

## Faces, step 1: the face models (ADR-040)

Nothing shows in the app yet: this step adds the face models and the code that finds and describes faces. Name and find comes in a later step.

Automated (run here; the model tests skip where the models are not installed, as in CI):
```
cargo test -p keepsake-core --lib ai::faces
cargo test -p keepsake-core --test face_tests -- --include-ignored corpus
```

Manual (both OSes):
1. Run `scripts\setup.ps1 -WithAi` (Windows) or `scripts/setup.sh --with-ai` (macOS). **Expected:** besides the SigLIP model, it downloads `face_detection_yunet_2023mar.onnx` (about 230 KB) into `vendor/models/yunet-2023mar/` and `face_recognition_sface_2021dec.onnx` (about 39 MB) into `vendor/models/sface-2021dec/`, and says the checksums matched. Run it again. **Expected:** "already present, skipping download" for each.
2. Change one byte of a face model file (or replace it with any small file) and run the setup again. **Expected:** the setup notices the checksum is wrong and downloads it again.
3. Run `cargo test -p keepsake-core --test face_tests -- --include-ignored --nocapture` with `KEEPSAKE_CORPUS` set. **Expected:** 4 tests pass and a line like "corpus: 49 photos read, 25 with faces, 56 faces; lowest self-match 0.939". Without the models, the tests print "skipping" and pass.
4. **macOS:** the same test reports the faces found with Core ML (the GPU test compares Core ML with the CPU and passes).

---

## Which AI tagged it, Tag with AI to All, and tags in the search bar (ADR-039)

The first start after this update upgrades the catalog (schema 12); it takes a moment and nothing visible changes.

Automated (run here and in CI):
```
cargo test -p keepsake-core --test ai_tests each_ai_is_recorded
cargo test -p keepsake-core --test catalog_tests hierarchical_tags_and_search
cargo test -p keepsake-core --test xmp_tests
cargo test -p keepsake-core --lib settings
cd app/ui; pnpm vitest run src/unit/tagSearch.test.ts
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/cloud.spec.ts e2e/mocked/ai.spec.ts
```

Manual (both OSes). You need API keys for **at least two** AIs in Settings → Tag with AI (cheapest models are fine).

### Tag with AI → All
1. Right-click one photo → **Tag with AI…**. Open the **AI** list. **Expected:** the AIs with a key, then **All (Claude, ChatGPT and Gemini)** (only the names with a key).
2. Pick **All**. **Expected:** the heading says "Tagging with Claude, ChatGPT and Gemini: …"; the text names each model, says "Each one tags them", and gives one cost "in all", charged by each company. Every AI's privacy note shows (OpenAI's 30 days, Gemini's billing note).
3. Press **Send**. **Expected:** the bar moves; at the end "Photos tagged: Claude 1 of 1, ChatGPT 1 of 1, Gemini 1 of 1." If one key is bad, that AI's count stays 0 and the problem starts with its name.
4. Right-click another photo → **Tag with AI…**. **Expected:** **All** is already picked. Remove all keys but one in Settings and try again. **Expected:** no AI list; that one AI is used.

### Which AI gave each tag (Inspector)
5. Select the photo from step 3. **Expected:** the Inspector's **Keywords** has each tag once. Under **AI tags** (labelled *On-device*) the on-device tags are unchanged. Below, **Tagged with AI** has one row per AI that tagged it (Claude, ChatGPT, Gemini), each with the keywords **that** AI gave. A keyword two AIs gave is in both rows.
6. Remove one of those keywords with its ✕ in **Keywords**. **Expected:** it disappears from every AI row too. Add it back by typing it. **Expected:** it is a keyword again but **not** in any AI row (you added it).
7. A photo you only tagged yourself, or tagged by AI before this update. **Expected:** no **Tagged with AI** section.
8. Check the sidecar survives: close the app, open the photo's `.xmp` next to it in a text editor. **Expected:** a `keepsake:AITagSources` list with `keepsake:Name` and `keepsake:By` (`claude`, `openai`, `gemini`). The original photo file's modified date did not change.

### Tags in the search bar
9. Click the search box and type the start of a tag you use, e.g. `bea`. **Expected:** a list under the box with matching tags and their counts (`# Beach (40)`), those starting with the letters first.
10. Click one. **Expected:** it becomes a chip "Beach ✕" in the search bar, the typed letters go, and the grid shows only items with that tag (same count as in the list).
11. Type another tag, press **↓** then **Enter**. **Expected:** a second chip; only items with **both** tags show (fewer or the same). A tag already chosen is not suggested again. A parent tag (e.g. `Travel`) also matches items with a tag below it (`Travel › Japan`).
12. Type words that are not a tag (part of a file name). **Expected:** they search names, comments and keywords, together with the chips.
13. With the box empty press **Backspace**. **Expected:** the last chip goes. Click ✕ on a chip. **Expected:** that one goes. **✕ Clear** removes all chips and filters.
14. **Esc** with the list open closes the list; Esc again clears the words.

---

## Tag with AI (Claude, ChatGPT or Gemini, for the photos you pick)

You need an API key from at least one: console.anthropic.com (Claude), platform.openai.com (ChatGPT), aistudio.google.com (Gemini; use a project with billing turned on). Each company charges for each photo; Claude Haiku 5.5, GPT-6 Luna and Gemini 3.5 Flash-Lite are the cheapest for trying it.

Automated (run in CI; local test runs are turned off on this computer):
```
cargo test -p keepsake-core --lib ai::cloud
cargo test -p keepsake-core --lib settings
cargo test -p keepsake-core --test ai_tests cloud refused each_provider
cargo test -p keepsake-platform secrets
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/cloud.spec.ts
```

Manual (both OSes):
1. Open **Settings**. **Expected:** below On-device AI, a "Tag with AI" card with three sections: Claude (Anthropic), ChatGPT (OpenAI) and Gemini (Google). Each says "No API key is saved yet. Get one at …" and has a model list with the cost a photo: Claude Opus 5.5, GPT-6.1 Sol and Gemini 3.6 Flash are selected.
   - **Expected:** the ChatGPT section says OpenAI keeps photos up to 30 days to check for abuse; the Gemini section says to use a key from a project with billing.
2. In the ChatGPT section type `hello` and press **Save key**. **Expected:** a red message says it does not look like a ChatGPT API key. Paste a Claude key there. **Expected:** the same message. Paste your OpenAI key and save. **Expected:** the field empties and that section says "An API key is saved in this computer's credential store." The other sections still say no key.
   - **Windows:** Credential Manager → Windows Credentials. **Expected:** an entry for `openai-api-key` (and `claude-api-key` if you had one, which still works).
   - **macOS:** Keychain Access, search Keepsake. **Expected:** items with accounts `openai-api-key` and so on.
   - `config.toml` has `openai_model`, `gemini_model` and `cloud_provider`, but no key.
3. Save a Gemini key too (either kind: `AIza…`, or the newer `AQ.…` with dots). **Expected:** it is accepted. Change Gemini's model to **Gemini 3.5 Flash-Lite**, close and reopen Settings. **Expected:** still selected; the other providers' models did not change.
4. In the grid, right-click a photo of something recognisable. **Expected:** a menu with **Open** and **Tag with AI…**. Choose it. **Expected:** a window "Tagging with …" with an **AI** list showing only the providers with a key, e.g. "ChatGPT (GPT-6.1 Sol)" and "Gemini (Gemini 3.5 Flash-Lite)".
5. Pick **Gemini**. **Expected:** the heading says "Tagging with Gemini: …", the line says "Send 1 photo to Gemini 3.5 Flash-Lite to be tagged? It costs under 1¢, charged by Google.", and the billing note shows. Pick **ChatGPT**: the line says "charged by OpenAI" and the 30-days note shows. Nothing is sent yet.
6. Press **Send**. **Expected:** within about 10 seconds "1 of 1 photos tagged." The inspector shows new keywords (list tags, maybe some under AI); old ones are still there. No person is named.
7. Tag another photo. **Expected:** the AI list now starts at the one you used last (ChatGPT).
8. Tag the same photo again with the other AI. **Expected:** it finishes; keywords both gave appear once.
9. Select 3 photos, **Tag with AI…**, **Send**, then **Hide**: a small "ChatGPT · n%" (or Gemini) button stays in the corner. **Stop** on a larger selection: photos done keep their tags.
10. Open a photo in the viewer. **Expected:** the top bar has **Tag with AI**; it works as in step 4.
11. Remove all keys but one. **Tag with AI…**. **Expected:** no AI list, the one with a key is used. Remove the last key. **Expected:** "Add an API key for Claude, ChatGPT or Gemini in Settings first" with **Open Settings**.
12. Save a wrong key that still looks right (change its last letters) for Gemini and tag a photo with it. **Expected:** "0 of 1 photos tagged. 1 photo could not be: Gemini did not accept the API key: check it in Settings". Put the right key back.
13. Unplug the network and tag a photo. **Expected:** "could not reach ChatGPT: …" (or the AI chosen), retried after 30 s and 60 s; plug back in within a minute and it finishes.
14. Originals: a tagged photo's file is unchanged (size and modified time); the keywords are in its `.xmp` sidecar.
15. Claude still works as before (ADR-036): a Claude key saved before this change is still there and tags photos.

## Making every missing thumbnail without the app

Automated:
```
cargo test -p keepsake-core --test backfill_tests
cargo test -p keepsake-cli --test cli_tests renders_thumbnails
```

Manual (both OSes; on macOS use `./target/release/keepsake` and a library such as `~/Pictures/Keepsake`, and stop a run with `kill -9 <pid>` instead of Task Manager). The app may stay open. Build first: `cargo build -p keepsake-cli --release`.
1. Run `keepsake renders thumbnails`. **Expected:** "Making missing thumbnails in batches of 100 …", then one line per batch: "batch 1 saved: … made, … already had them, … failed, … offline (position #…)". Files that cannot be rendered appear as `FAILED <file>: <error>` on one line and the run goes on.
2. After a few batches, end the process the hard way (Task Manager → End task on `keepsake.exe`, or `kill -9`). Note the last "position #N".
3. Run the command again. **Expected:** "Continuing after asset #N" with the number from step 2 (at most one unsaved batch is redone), and the batches carry on from there.
4. Press Ctrl-C once during a batch. **Expected:** it stops after the files in hand, prints "Stopped. Run `keepsake renders thumbnails` to continue from here." and exits with 130; the next run continues from that point.
5. Run `keepsake renders thumbnails --failures-log failed.txt > run.log` (redirected). **Expected:** `run.log` has the batch lines, and `failed.txt` one line per failed file: path, a tab, the error.
6. Disconnect the network share (or unplug the drive) and run it again. **Expected:** "Stopped: the next 100 asset(s) after #N are all offline …", exit status 2, and the position does not move. Reconnect and run again: it continues from #N.
7. In the app, scroll to photos from step 1. **Expected:** their thumbnails show without waiting. No original file changed (modified dates in Explorer / Finder are the same).
8. Once it reaches the end ("Reached the end of the library"), run it again. **Expected:** "0 asset(s) looked at" unless new photos were added. `--restart` walks the whole library again and retries the failed files.

---

## Tag the photos with no date

Needs Settings → On-device AI on.

1. In the sidebar's calendar tree, right-click **No date**. **Expected:** a menu with **Show** and **Tag photos** (no Find duplicates). Right-clicking **Calendar** shows no menu.
2. Choose **Tag photos**. **Expected:** the library shows the items with no date, and a window "Tagging No date" says "Photos and videos with no date." with a progress bar, Stop and Hide; it ends with "N of N photos tagged."
3. Select one of them. **Expected:** the Inspector's AI tags list what it shows. Searching or the Tag list finds them by those tags.
4. Start it again and press **Stop**. **Expected:** "Stopped: … done. What was done stays."

---

## Helpers end with the app

Automated:
```
cargo test -p keepsake-core --test probe_tests shutdown
cargo test -p keepsake-platform job
cargo test -p keepsake-app shutdown
```

Manual:
1. **Windows:** start Keepsake and browse a few photos, or import a card, so ExifTool starts. In Task Manager → Details, sort by name. **Expected:** one or two `exiftool.exe`. Close Keepsake with ✕. **Expected:** within about 5 s no `exiftool.exe`, `ffmpeg.exe` or Keepsake `msedgewebview2.exe` is left.
2. **Windows:** start Keepsake again and end `keepsake-app.exe` in Task Manager (End task), as a crash would. **Expected:** its `exiftool.exe` disappears at once too.
3. **Windows:** right-click a photo → Open in → an app (or Show in Explorer), then close Keepsake. **Expected:** the opened app and the Explorer window stay open.
4. **Windows:** run the installer over an installed Keepsake after using it and closing it. **Expected:** no "Error opening file for writing … exiftool.exe".
5. **macOS:** start Keepsake, browse some photos, quit it (⌘Q). In Activity Monitor search "exiftool". **Expected:** nothing left after about 5 s.
6. **Both:** start an import of a card (or a backup, or the AI model download in Settings) and close Keepsake while it runs. **Expected:** the app closes within about 5 s; nothing of `exiftool`, `curl` or `ffmpeg` is left. Start it again: the import or backup can be resumed as after any interruption.
7. **macOS, known gap:** after a crash or Force Quit, an `exiftool` can stay (it does not end when its input closes). End it with `pkill -f exiftool`.

---

## On-device AI: tags and same-day duplicates

Setup (once): `scripts\setup.ps1 -VendorOnly` (Windows) or `scripts/setup.sh` (macOS, Apple Silicon) installs ONNX Runtime into `vendor/onnxruntime/`. Add `-WithAi` / `--with-ai` to get the model too (372 MB) for the automated model tests; the app downloads its own copy when you turn AI on.

Automated:
```
cargo test -p keepsake-core --lib ai::
cargo test -p keepsake-core --test ai_tests
cd app/ui; pnpm test; pnpm exec playwright test --project=mocked e2e/mocked/ai.spec.ts
```
Without the model, the three `ai_tests` that run it print "skipping" and pass.

Manual (both OSes), with a library that has some bursts or repeated shots:
1. Open **Settings**. **Expected:** an "On-device AI" card says "Off." and that turning it on downloads the model once (371.8 MB).
2. Tick **Tag photos and find duplicates**. **Expected:** a progress bar counts up to 371.8 MB. Afterwards the card says "Starting…", then "Ready, on the graphics card (DirectML). 0 items analysed so far." Nothing is analysed yet: it waits until you ask (step 4). On a Mac it says CoreML; without a usable GPU it says processor (CPU). The model is in `%LOCALAPPDATA%\keepsake\models\siglip2-b16-224\` (Windows) or `~/Library/Caches/keepsake/models/siglip2-b16-224/` (macOS). Nothing is written next to your photos.
3. Start the download again on a fresh profile and press **Cancel**. **Expected:** the bar goes away, AI stays off, and no `vision_model.part` is left in that folder.
4. In the sidebar's calendar tree (By date), right-click a day with a burst of RAW photos (or select it and press the Menu key or Shift+F10). **Expected:** the menu has **Show**, **Find duplicates** and **Tag photos**. Choose **Find duplicates**. **Expected:**
   - Duplicates opens as "Duplicates · Sat 4 July 2026" (that day).
   - It shows "Comparing: n of m done…", counting up every few seconds, then "m of m RAW photos compared." Only RAW photos are counted. No AI tags are given. For a day of a few hundred photos with thumbnails, that takes well under a minute.
   - Its groups appear. Only that day was analysed: `keepsake ai status` shows that day's count, not the whole library.
   - The same works on a month or a year (longer).
   - With AI off, the menu says to turn it on in Settings first.
   - Thumbnails missing for that day are made first. AI work pauses on battery and during a card import.
5. Right-click the same day and choose **Tag photos**. **Expected:** the menu closes at once, the library shows that day, and the progress window opens in the bottom-right corner: "Tagging Sat 4 July 2026", "Getting ready…" (a few seconds on a USB drive), then a progress bar with "n of m done…", then "m of m photos tagged." and **Close**. It has **Stop** and **Hide**, as for a folder (step 16). A photo of a person, baby or child gets **People** (and Baby, Child or Portrait as fits). Photos tagged by the previous build are tagged again, quickly. Photos already compared need no new analysis, so it is quick. Select one photo. **Expected:** the inspector shows **AI tags** under Keywords (dotted chips, e.g. "Beach", "People"); hover shows how sure it is.
6. Type one of those tags in the search box. **Expected:** the photo is found.
7. Press ✕ on an AI tag. **Expected:** it goes. Run `keepsake ai retag`, or wait for the photo to be analysed again. **Expected:** it does not come back. Press ✓ on another. **Expected:** it moves to Keywords.
8. On the Duplicates screen, press **All analysed dates** (or open Duplicates from the sidebar). **Expected:** every analysed day with groups, newest first, each with "n of m" left to decide. Only RAW photos are grouped: a JPEG-only photo, including an edited JPEG with the same name as a RAW (e.g. `Edits\DSC001.JPG` next to `DSC001.ARW`), never appears in a group. A day shows its groups: near-identical shots, the suggested one ("Suggested", best rated, else earliest) outlined. The others say "96% alike" or "Same picture".
9. Click a photo in a group. **Expected:** it says "Kept" and the others "Not kept"; the day's count drops by one.
10. Click in the groups area and use the keys: arrows move, **K** keeps the focused photo, **N** marks the group "Not duplicates" (it goes away), and **U** undoes a group's decision.
11. Go to **All items** and tick **Hide not kept** in the toolbar. **Expected:** the item count drops by the number of "not kept" photos; untick to see them again.
12. Check the originals: in Explorer / Finder their modified dates and sizes are unchanged and none was moved or deleted. Only the library's XMP sidecars changed (they now hold `keepsake:DuplicateChoice` / `keepsake:AITagsRejected`).
13. Untick **Tag photos and find duplicates**, then press **Delete the downloaded model**. **Expected:** the model file is gone; AI tags and your keeper choices stay. Duplicates now says the AI is off, with a button to Settings.
14. In the toolbar, open the **Tag** list (after Lens). **Expected:** "Any tag", then every keyword and AI tag with its count, most used first (e.g. "People (412)"). Choose one. **Expected:** only items with that tag show, and the count in the status bar drops. Press **✕ Clear**. **Expected:** the list is back on "Any tag" and every item shows. On both OSes.
15. Tag a day whose photos have few obvious subjects (a building, the moon, flowers). **Expected:** nearly every photo gets one or two tags; hover shows they are less sure. ✕ removes a wrong one for good.
16. In the sidebar choose **By folder**, open a drive, and right-click a year folder. **Expected:** the menu has Show, Scan again, **Find duplicates** and **Tag photos**. Choose **Tag photos**. **Expected:** the menu closes, and a window in the bottom-right corner reads "Tagging <drive> › <year>", then "Getting ready…". Then a progress bar and "n of m done…" count up every 2 s. The library can still be used.
    - Press **Hide**. **Expected:** the window goes and a small "Tagging · 40%" button stays in the corner; click it to bring the window back.
    - Press **Stop**. **Expected:** "Stopped: n of m done. What was done stays." Run Tag photos again on the same folder. **Expected:** it carries on from there.
    - Let one finish. **Expected:** "m of m photos tagged." and a **Close** button. With **Find duplicates** it shows "m of m RAW photos compared." and **Show duplicates**, which opens the Duplicates screen.
    - Speed: tagging a folder of a few hundred photos that already have thumbnails takes minutes, not an hour, even on the USB drive. On both OSes.
17. Check one picture's tags from a terminal: `keepsake ai explain "C:\Photos\photo.jpg"` (on a Mac, `keepsake ai explain ~/Pictures/photo.jpg`). **Expected:** "Tags given: …", then the 15 best tags with their scores. A photo of a person lists People (or Portrait, Baby, Child) near the top.
18. The app icon: after installing, look at the Start menu entry, the desktop shortcut, the taskbar while Keepsake runs, and the installer file itself. **Expected:** each shows a dark camera on an orange rounded square, not a blank square. On macOS, the Dock and Finder show the same icon.
19. Windows: run `scripts\keepsake.bat`. **Expected:** after "Repository: …" it prints "Getting the latest changes..." and git's result ("Already up to date." or the files it updated), then starts the app. Change a file in the repository and run it again. **Expected:** "You have changed files here: not updating, using them as they are." Unplug the network. **Expected:** "Could not update (offline, or the branch moved); using the copy you have.", and the app still starts.
20. Windows only: run the installed app (not from source) and turn AI on. **Expected:** it works without `vendor/`, because ONNX Runtime and DirectML are in the app's `onnxruntime\` folder. Their licences are in `licenses\`.

macOS notes: CoreML is tried first, then the CPU. The installer does not bundle ONNX Runtime yet (no macOS app bundle of the sidecars), so on a Mac test from source.

## Show in Explorer / Finder (item right-click menu)

Automated:
```
cargo test -p keepsake-core --test reveal_tests
cargo test -p keepsake-platform reveal
cargo test -p keepsake-app reveal
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/show-in-folder.spec.ts
```

Manual, with a library that has photos and videos on a connected drive, and (for step 7) some on a drive you can unplug. **Windows** says "Explorer", **macOS** says "Finder".
1. In the grid, right-click a photo. **Expected:** a small menu opens at the pointer with **Show in Explorer** (macOS: **Show in Finder**) focused, a line, then **Open in …** for each application set in Settings and **Open with the system default**.
2. Click **Show in Explorer / Finder**. **Expected:** the menu closes; a window opens on the folder holding the photo's original, with that file selected (highlighted). On macOS Finder comes to the front with the file selected. Try a file whose folder name has spaces: it still works.
3. Do the same for a video, and for a RAW + JPEG item. **Expected:** the original file is selected (for the pair, the file the catalog holds as the original).
4. Select three tiles (Ctrl/⌘-click), then right-click the third. **Expected:** all three stay selected, the menu is for the third (hover over the menu: its name is that file), and Show selects that file.
5. Click a tile, press **Shift+F10**. **Expected:** the menu opens on the tile; ↓ and ↑ move through the items; **Esc** closes it and the tile keeps focus. On a Windows keyboard with a Menu key, press it: same menu. Press Enter on Show: the folder opens.
6. Press Ctrl+3 (details table) and right-click a row, then open the viewer (Enter) and right-click the picture, or press Shift+F10. **Expected:** the same menu in both. In the viewer, Esc closes the menu but not the viewer, and after Show the bottom line says "Shown in Explorer" ("Shown in Finder").
6b. **macOS:** Ctrl+click two tiles. **Expected:** each Ctrl+click adds or removes the tile from the selection and no menu opens; the arrow keys still move in the grid. A two-finger click (secondary click) opens the menu.
7. Unplug the drive holding some items, then right-click one of them (it has an "offline" badge). **Expected:** **Show in Explorer / Finder** is greyed out, the menu says **Drive not connected (drive name)**, and there are no Open in items. Esc closes it. Plug the drive back in and try again: Show works.
8. Check the originals did not change: their modified dates and sizes are the same as before. Nothing is moved, renamed or deleted.

## Open in for photos on a network drive

Automated:
```
cargo test -p keepsake-core --lib external
```

Manual, **Windows**, with a library folder on a network share (e.g. `\\MYCLOUD-92LNT0\aelkays\Photos-OldAcer`) and Photoshop / Topaz Photo set in Settings for ARW and JPG:
1. Select a JPG from the network share, open **Open in** in the inspector and choose **Photoshop**. **Expected:** Photoshop opens that photo; the inspector says "Opened with Photoshop".
2. Do the same with an ARW and **Topaz Photo**. **Expected:** Topaz Photo opens that RAW.
3. Press Ctrl+O on a network photo. **Expected:** it opens in the first app set for its type.
4. Choose **System default**. **Expected:** the photo opens in Windows' default app for that type.
5. Repeat step 1 with a photo on a local drive (e.g. `E:\Pictures`). **Expected:** still opens as before.

Manual, **macOS**, with a library on a mounted SMB share (`/Volumes/…`): choose an app from **Open in**. **Expected:** it opens the photo (macOS paths are unchanged by this fix; this checks nothing broke).

---

## Selecting many items to edit

Automated:
```
cargo test -p keepsake-core --test selection_summary_tests
cd app/ui; pnpm test; pnpm exec playwright test --project=mocked e2e/mocked/multi-select.spec.ts
```

Manual (both OSes; on macOS read Ctrl as ⌘), in the grid with at least a dozen photos:
1. Point at a thumbnail. **Expected:** a round tick-box appears at its top right.
2. Click one photo, then click the tick-boxes of two others. **Expected:** all three are selected (blue border, filled tick), the status bar says "3 selected", and the inspector says "3 items selected — every edit below applies to all of them".
3. Click the tick-box of one of them again. **Expected:** it leaves the selection ("2 selected").
4. Click **Select** in the toolbar (it turns to "✓ Selecting" and every tile shows its tick-box). Click three tiles with no keys held. **Expected:** each click adds a tile; clicking a selected tile removes it. **All** selects everything shown, **None** clears it.
5. Press Esc in the grid. **Expected:** the selection clears and Select mode turns off.
6. Press the mouse on a thumbnail and drag across several rows. **Expected:** a blue rectangle follows the mouse and every tile it touches is selected; on release the rectangle goes. Drag near the bottom edge: the grid scrolls and keeps selecting.
7. Hold Ctrl and drag a second rectangle elsewhere. **Expected:** it adds to the first selection. Click in an empty gap between tiles: the selection clears.
8. Select three photos and click 4 stars in the inspector. **Expected:** all three show 4 stars on their tiles.
9. Type a keyword (e.g. `Places|Lake`) and press Enter. Type a comment and click elsewhere. **Expected:** each of the three photos, selected alone, shows the keyword and the comment.
10. Add a fourth photo (its tick-box). **Expected:** the inspector shows "Mixed ratings" (if its rating differs), the keyword chip reads "Places|Lake (3 of 4)" with a dashed border, and the comment box says "Different comments…" if its comment differs. Click **+** on the chip. **Expected:** the chip loses "(3 of 4)": all four have it.
11. Check that no original file changed: their modified dates in Explorer / Finder are the same as before; only the XMP sidecars in the library changed.

---

## Faster thumbnails on a network drive

Automated:
```
cargo test -p keepsake-core --test render_tests built_in
cargo test -p keepsake-core --test render_tests fingerprints_wait
$env:KEEPSAKE_CORPUS = "C:\path\to\corpus"
cargo test -p keepsake-core --test corpus_tests -- --include-ignored corpus_jpeg
```

Manual (both OSes), with a library on a network drive or a slow USB disk and many items without thumbnails:
1. Before starting the new build, note the rate: run `keepsake renders status` twice, a minute or two apart, and compare the "thumbnails/previews: … done" numbers.
2. Close Keepsake, start the new build, wait two minutes, and measure the same way. **Expected:** more thumbnails a minute than before.
3. Run `keepsake renders --json status` and look at `fingerprint` under `by_kind`. **Expected:** its `done` number stays the same while thumbnails are being made, and goes up once they are all done.
4. Scroll the grid to camera JPEGs (straight from a Sony camera). **Expected:** their thumbnails look right and are the right way up, including photos taken in portrait.
5. Open one of them in the viewer. **Expected:** the full picture appears sharp (it is made from the whole file), and zooming to 100% shows full detail.

---

## Report rows open their photos

Automated:
```
cargo test -p keepsake-core --test report_tests every_report_row
cd app/ui; pnpm test; pnpm test:e2e report
```

Manual (both OSes):
1. Open **Report** in the sidebar. **Expected:** the name in each row (a year, a camera…) is a link in the accent colour; hovering it says how many items it shows.
2. On **Year**, click a year. **Expected:** the grid shows that year's photos and videos; the heading reads **Report › Year 2024**, and the item count in the status bar equals the row's **Items**.
3. Click **Report** in the heading. **Expected:** the report comes back on the **Year** table.
4. Repeat with a month, a day, a camera, a lens, a file type (for example ARW) and a drive. **Expected:** each time the count equals the row's **Items**. A drive that is unplugged still shows its items (with cached thumbnails).
5. If the report has an **Unknown** row (date, camera or lens), click it. **Expected:** only items without a date (or without a camera, or lens) show.
6. Two cameras whose names overlap (for example ILCE-7 and ILCE-7M4): click one. **Expected:** only that camera's items show.
7. Set a filter in the toolbar (type Videos, some words), open the report and click a row. **Expected:** the filters are cleared, and the count matches the row.

---

## A full cache keeps the thumbnails

Automated:
```
cargo test -p keepsake-core --test render_tests eviction
cargo test -p keepsake-core --test render_tests a_full_cache
cargo test -p keepsake-core --test catalog_tests opening_a_library_catalog
cd app/ui; pnpm test:e2e viewer
```

Manual (both OSes), with a large library whose cache is full (Settings shows the cache use against its limit):
1. Start Keepsake. **Expected:** the window and the grid come up within a few seconds, and the sidebar's **By folder** tree lists the folders.
2. Scroll the grid to the newest photos and wait a minute. **Expected:** their thumbnails show and stay; they do not disappear and come back.
3. Open a photo that has not been opened before (double-click). **Expected:** the picture appears within a few seconds (its preview is made right away). Open a video that has not been played. **Expected:** it plays after a short wait.
4. On Settings, the cache use stays at or under its limit. **Expected:** it settles near 90% of the limit or lower rather than sitting exactly at it.
5. Raise the cache limit in Settings (for example to 100 GB), wait a while. **Expected:** previews are made in the background again (the cache use grows past the old limit).
6. Open the log folder (Settings → Open log folder) and look at the newest log. **Expected:** "slow catalog write" lines are rare, not one every minute or two.

---

## Scan again one folder (right-click)

Automated:
```
cargo test -p keepsake-core --test add_folder_tests subfolder
cd app/ui; pnpm test:e2e folder-tree
```

Manual (both OSes):
1. In the sidebar choose **By folder**, open an added folder (on Windows for example `Videos` on `Z:`), and right-click one of its sub-folders. **Expected:** a menu with **Show** and **Scan again**.
2. Add a new photo to that sub-folder on disk, delete one from it, and also add a photo to a *different* sub-folder. Then choose **Scan again** on the first sub-folder. **Expected:** the Folders screen opens with "Scanning a folder…" and then "Scanned <name>: N items". The new photo is in the library and the deleted one is gone. The photo added to the other sub-folder is *not* in the library yet. Ratings on the scanned sub-folder's photos are unchanged.
3. Right-click the added folder itself (the top of its tree) and choose **Scan again**. **Expected:** the whole folder is read, and the photo in the other sub-folder comes in.
4. Right-click a folder on a drive that is not connected. **Expected:** Scan again is greyed out, and the menu says the drive is not connected.
5. With the keyboard, move to a folder with the arrow keys and press the Menu key (Windows) or Shift+F10 (both OSes). **Expected:** the menu opens on its first item; ↑ ↓ move, Enter chooses, and Escape closes it and returns to the folder.
6. Start a backup, and while it runs choose Scan again on a folder. **Expected:** the menu says another job is already running, and nothing starts.
7. Nothing inside the added folder is changed by any of this: no files are written there.

---

## Previous and next in the viewer

Automated:
```
cd app/ui; pnpm test:e2e viewer
```

Manual (both OSes):
1. Double-click a photo in the grid. **Expected:** large ‹ and › buttons sit on the left and right edges of the picture, and brighten when the mouse is over the picture.
2. Click ›, then ‹. **Expected:** the next and then the previous item shows in the same viewer; the position at the top ("4 of 50") changes, and the viewer does not close.
3. Go to the first item. **Expected:** ‹ is hidden. On the last item, › is hidden.
4. After clicking a button, press → and ←. **Expected:** they step too.
5. Select three items (Ctrl-click on Windows, ⌘-click on macOS), press Enter, and use ‹ ›. **Expected:** only those three are shown.
6. Open a video. **Expected:** the buttons are there too, and do not cover the play bar.
7. Zoom a photo in (Z). **Expected:** the arrow keys pan the picture, and the edge buttons still step to the next item.

---

## No "database is locked" during a backlog

Automated:
```
cargo test -p keepsake-core --lib catalog::error
cargo test -p keepsake-core --test fingerprint_tests large_backlog
```

Manual (both OSes; best with the library on a USB hard disk or other slow drive):
1. Start Keepsake with a large library that still has thumbnails or fingerprints to make (the status bar shows work queued), then click **Scan again** on a folder in **Folders**.
2. Scroll the grid and open items while the scan and background work run, for a few minutes.
3. **Expected:** no "database is locked" message, and thumbnails keep appearing. A slow drive may make things wait briefly, but nothing fails.
4. Open the log folder (**Settings → Open log folder**). **Expected:** no `database is locked` lines. Any `slow catalog write` lines name the code that held the catalog; please send them along.

---

## Older video types in added folders

Automated:
```
cargo test -p keepsake-core --test add_folder_tests older_home_video_types
```

Manual (both OSes):
1. Open **Folders** in the sidebar and click **Scan again** next to `Videos` (on Windows `Z:ideos`, a NAS share); on macOS add a folder holding `.mpg`, `.vob` or `.wmv` files.
2. **Expected:** the count grows from 33 to about 300; `.mpg`, `.mpeg` and DVD `.VOB` clips appear as videos with thumbnails and play in the viewer; files in subfolders (for example `Cancun_2009`) are there too.
3. **Expected:** DVD `.IFO`/`.BUP` files, `Thumbs.db` and the hidden `.metadata` folder are not items. A few broken files (for example `SalehKetab_trial_0.mp4`, which has no `moov` atom) show without a thumbnail, as before.

---

## Start-up while the library opens

Automated:
```
cargo test -p keepsake-app state::tests
cd app/ui; pnpm test:e2e opening
```

Manual (both OSes):
1. Start Keepsake normally. **Expected:** the library appears within a few seconds, as before.
2. To see a slow open, use a library on a slow drive (a USB hard drive or network folder) the first time after a catalog upgrade, or a large library on any drive. **Expected:** the window draws at once with **Opening your library…**; after about 3 s it adds that the first start after an update can take a minute; then the library appears. The window can be moved and resized meanwhile and is never shown as *Not responding* (Windows) or with a spinning cursor (macOS).
3. Close Keepsake while it says Opening, then start it again. **Expected:** it opens normally (an interrupted upgrade is redone).

---

## Library report (Tools → Report)

Automated:
```
cargo test -p keepsake-core --test report_tests
cargo test -p keepsake-cli --test cli_tests report
cd app/ui; pnpm test; pnpm exec playwright test e2e/mocked/report.spec.ts --project=mocked
```
**Expected:** all pass.

The first start after this change upgrades the catalog to schema v8 (it adds each item's file count and size). This happens once and takes about 15 seconds for 300,000 items; the window may wait that long on that first start.

1. In the sidebar under **Tools**, click **Report**.
   **Expected:** within a second or two the screen shows Photos, Videos, Files and Size for the whole library, and a table by **Year**, newest first. Items with no date are one **Unknown** row at the bottom. Hovering a size shows the exact bytes.
2. Compare with what you know: the number of photos plus videos equals "N items" in the status bar of **All items** with no filters.
   **Expected:** they match. A RAW+JPEG pair is one photo, but its row's Files counts 2.
3. Click **Month**, **Day**, **Camera**, **Lens**, **File type** and **Drive** in turn.
   **Expected:** each shows its own table. Months read "June 2024", days "1 June 2024". Cameras and lenses are biggest first, with **Unknown** last (items without a camera or lens, e.g. added with `--no-probe`). File type shows `ARW`, `JPG`, `MP4` and so on, items only (Photos and Videos show —); a RAW+JPEG pair is counted under both. The footer row is the library total.
4. With a drive unplugged (e.g. the network drive off, or an external disk ejected), click **Refresh**.
   **Expected:** nothing changes in the totals: unplugged items still count. In **Drive**, that drive is listed with "(unplugged)".
5. If you have a backup (Tools → Backup), look at **Drive**.
   **Expected:** the backup drive is listed with its own files and size. The totals at the top are not doubled: a backed-up file counts once. The footer "All drives" adds every copy.
6. Click **Export to CSV…**, keep the suggested name (`Keepsake report <today>.csv`) and save it in Documents.
   **Expected:** "Saved to …" with the path. Opening the file in Excel (Windows) or Numbers (macOS) shows the columns Breakdown, Group, Photos, Videos, Items, Files, Bytes, Size, a Total row, then the Year, Month, Day, Camera, Lens, File type and Drive rows. Accented names read correctly.
7. Click **Export to CSV…** and cancel the dialog.
   **Expected:** nothing is written and no error is shown.
8. From a terminal: `keepsake report`, `keepsake report --by camera`, `keepsake report --by drive --csv report.csv`, `keepsake --json report`.
   **Expected:** a table with photos, videos, items, files and size per group and a Total line; `--csv` writes the file and says so; `--json` prints the whole report.
9. Check nothing was written to your photo folders.
   **Expected:** the report only reads the catalog; no file in a photo folder has a new modification time.

On **macOS**, the save dialog is the system sheet; on **Windows**, the standard Save As window. Everything else is the same.

---

## Adding a large folder: saved as it goes, carries on after a stop

Automated:
```
cargo test -p keepsake-core --test add_folder_batch_tests
cd app/ui; pnpm exec playwright test e2e/mocked/folders.spec.ts --project=mocked
```
**Expected:** all pass.

Use a big folder (thousands of photos). A network drive is the best test, e.g. `2010` on the MyCloud drive; on macOS a folder on a NAS share or an external disk.

1. Open **Folders**, type or pick the folder, and click **Add**.
   **Expected:**
   - within a few seconds the folder appears under "In the library" and in the sidebar's folder tree (By folder), even though the progress bar has only just started;
   - the line "N items in the library so far" grows by about 20 at a time;
   - the sidebar count for the folder grows as well; the grid shows the new photos every few seconds when nothing is selected.
2. Select a photo in the grid while the scan runs.
   **Expected:** the selection stays put (the grid does not jump); when the scan finishes, the grid shows everything.
3. Click **Stop** part way through.
   **Expected:** the screen says what was read so far is in the library; the folder and its items stay in the tree.
4. Click **Add** again for the same folder (or **Scan again** in its row).
   **Expected:** the count starts from where it stopped, the progress moves quickly over the files already saved, and it finishes with the full number of items. No item is listed twice.
5. Quit Keepsake while a scan runs, start it again, and add the folder again.
   **Expected:** the same as step 4: it carries on.
6. In the folder (outside Keepsake), delete one photo and add a new one, then **Scan again**.
   **Expected:** the deleted photo is gone from the library, the new one is there, and the scan is quick.
7. From a terminal: `keepsake add-folder <folder>`, press Ctrl+C part way, then run it again.
   **Expected:** the first run says what was read is in the library; the second finishes and its report counts the items not read again.
8. Add a very large folder with many Lightroom `.xmp` files (e.g. `2019` on the MyCloud drive).
   **Expected:** the progress shows "Listing files", then "Reading sidecars", and the first items appear within about a minute; the screen never sits still for minutes. Photos rated in Lightroom show those ratings. The whole folder is in within minutes to tens of minutes, not hours (only the start and end of each file is read).

---

## Full fingerprints are made in the background

Automated:
```
cargo test -p keepsake-core --test fingerprint_tests
cd app/ui; pnpm exec playwright test e2e/mocked/folders.spec.ts --project=mocked
```
**Expected:** all pass.

1. Open an existing library made before this change.
   **Expected:** it opens as before; every folder already in it shows no "Fingerprinting" line (its files were read in full when added).
2. Add a big folder (e.g. `2019` on the MyCloud drive; on macOS a NAS share or external disk).
   **Expected:** when the add finishes, its row under "In the library" says "Fingerprinting N files in the background", and N goes down every few seconds. Thumbnails keep appearing meanwhile. When N reaches 0 the line goes.
3. While it is fingerprinting, quit Keepsake and start it again.
   **Expected:** the line comes back with the count where it was, and keeps going down.
4. Unplug the drive (or disconnect the network share) while it is fingerprinting.
   **Expected:** the row says "N files to fingerprint when connected"; reconnect and it carries on within about ten minutes.
5. From a terminal: `keepsake verify` while some files are not fingerprinted yet.
   **Expected:** for that folder it says how many were checked by size and first and last MiB only; no problems are reported.
6. Back the library up to another drive (Backup screen) before fingerprinting finishes.
   **Expected:** the backup copies the folder's files without errors; those files then no longer count as waiting for a fingerprint.

---

## The viewer's information panel

Automated:
```
cd app/ui; pnpm exec playwright test e2e/mocked/viewer.spec.ts --project=mocked
cd app/ui; pnpm exec playwright test e2e/app/tools.spec.ts --project=app
```
**Expected:** all pass (the second is Windows only).

1. Open a photo in the viewer (double-click it) and click **ⓘ Info** at the top right.
   **Expected:**
   - a panel opens on the right; the picture shrinks to fit the space left, and is still whole and centred;
   - at the top: date and time (with seconds), camera, lens, exposure (e.g. "1/250 s · f/2.8"), ISO, focal length, dimensions, file size, and the location if the photo has GPS;
   - below: All metadata, grouped as in the inspector (Camera and image, Exposure, maker notes, File…).
2. Type "iso" in the panel's filter.
   **Expected:** only ISO tags stay; the viewer does not move to another photo or close while you type.
3. Click outside the filter (on the picture's dark edge), then press → and ←.
   **Expected:** the next and previous items are shown, and the panel updates for each one.
4. With the panel open, press Z, use the zoom slider, drag the picture, then click Fit.
   **Expected:** zoom works as before; at 100% the picture zooms around the middle of the area beside the panel; Fit shows the whole picture beside the panel, not under it.
5. Press **I**.
   **Expected:** the panel closes and the picture grows back to the full width. Press I again: it opens.
6. Open a RAW + JPEG pair and press R.
   **Expected:** the panel says "The RAW file" and shows the RAW's tags (e.g. a different file name ending in .ARW); R again goes back to the JPEG.
7. Open a video clip.
   **Expected:** the panel shows duration, frame rate and dimensions, and no exposure line; playback keys (Space, `,` `.`, J L) still work.
8. Close the app with the panel open, start it again and open the viewer.
   **Expected:** the panel is open. Close it, restart: it stays closed.
9. Unplug the drive with some photos (or pick an item marked as offline) and view it.
   **Expected:** the panel shows the catalog's date, camera and lens, and says the drive is not connected; no tag list.
10. On **macOS**, repeat steps 1, 4 and 5 with a Retina display.
    **Expected:** the same; Fit's percentage is smaller with the panel open when the picture is limited by its width.

---

## The calendar shows only months and days with pictures

Automated:
```
cd app/ui; pnpm exec playwright test e2e/mocked/library.spec.ts --project=mocked
```
**Expected:** all pass.

1. Open Calendar view (Ctrl+1) and click a year in which you took pictures in only some months.
   **Expected:**
   - only the months with pictures are shown, in order (e.g. March, June, December), each with its count and a cover;
   - no greyed-out empty months.
2. Click a month.
   **Expected:**
   - only the days with pictures are shown, in order, each with its count and a cover;
   - no Mo–Su headings and no blank or greyed-out days;
   - each day reads its weekday and number, e.g. "Sat 14" (or "14 Sat", depending on your language settings), and the weekday is correct.
3. Use the arrow keys to move between cells, Enter to open, Backspace to go up.
   **Expected:** focus moves only between the cells shown; Enter on a day opens that day's grid with the count shown on the cell.
4. Set a filter (e.g. 4 stars or more), then go back to the calendar.
   **Expected:** months and days with no matching items disappear.
5. Compare a day in the sidebar's calendar tree with the same day in the calendar.
   **Expected:** the same label (e.g. "Sat 14").

---

## Faster zoom in the viewer

Automated:
```
cd app/ui; pnpm exec playwright test e2e/mocked/viewer.spec.ts --project=mocked
cd app/ui; pnpm exec playwright test e2e/app/zoom-perf.spec.ts --project=app
```
**Expected:** all pass (the second is Windows-only and needs the debug app built; it prints a timing table).

Use a large camera JPEG (20 MP or more) on a plugged-in drive.

1. Open the photo in the viewer and wait one second without touching anything.
   **Expected:** no "Loading full size…" note appears; the picture may get slightly sharper as the full size replaces the preview.
2. Press **Z**.
   **Expected:** 100% at once, with no wait and no note.
3. Turn the mouse wheel quickly up and down over a detail.
   **Expected:** the zoom keeps up smoothly; the detail stays under the pointer; no stutter or white flash.
4. Drag the zoom slider end to end quickly.
   **Expected:** the picture follows the slider smoothly.
5. Zoom in and drag the picture to a corner, then further.
   **Expected:** it stops at the picture's edge; no empty gap opens beside it. Zoomed out below the window size, it sits centred.
6. Open a photo shot in portrait (camera turned).
   **Expected:** upright at Fit and at 100%.
7. Open a photo on an unplugged drive.
   **Expected:** the preview shows with the offline note; zoom enlarges the preview; nothing hangs.
8. Check the original files' modified time in Explorer / Finder before and after.
   **Expected:** unchanged.

---

## Thumbnail sizes in the grid

Automated:
```
cd app/ui; pnpm exec playwright test e2e/mocked/thumb-size.spec.ts --project=mocked
```
**Expected:** all pass.

1. Open Grid view (Ctrl+2). At the right of the toolbar, next to Calendar | Grid | Details, is "Thumbnails" with three buttons: **Small**, **Medium** and **Large**, each with a square icon.
   **Expected:** Medium is highlighted.
2. Click the large one.
   **Expected:**
   - fewer, bigger tiles (about 280 px);
   - the pictures are sharp, not blurry (they use the 1024 px thumbnails; tiles whose 1024 px thumbnail is not made yet show the small one until it is).
3. Click the small one.
   **Expected:** many small tiles (about 120 px).
4. Click a tile, press **+** twice, then **−** twice.
   **Expected:**
   - the size goes up to Large and back down to Small;
   - the selected tile stays on screen;
   - nothing is rated.
5. Restart Keepsake.
   **Expected:** the grid keeps the last size.
6. Switch to Details (Ctrl+3) or Calendar (Ctrl+1).
   **Expected:** the size switch is not shown.

---

## The zoom slider in the viewer

Automated:
```
cd app/ui; pnpm exec playwright test e2e/mocked/viewer.spec.ts --project=mocked
```
**Expected:** all pass.

1. Open a photo in the viewer (Enter or double-click).
   **Expected:** under the picture, a bar with **Fit**, **−**, a slider, **+**, **100%**, and "Fit".
2. Drag the slider to the right.
   **Expected:**
   - the picture grows from the middle of the window;
   - if you drag within half a second of opening, "Loading full size…" may show briefly (a RAW: "Decoding the RAW…"); after that, no note;
   - the level reads a percentage, up to 400% at the end.
3. Drag the picture, or use the arrow keys.
   **Expected:** it pans; you stay on the same photo.
4. Click **100%**.
   **Expected:** one picture pixel per screen pixel (fine detail is crisp).
5. Click **Fit**.
   **Expected:**
   - the whole picture fits the window;
   - the level reads e.g. "Fit (23%)".
6. Press **+** a few times, then **−**.
   **Expected:** it zooms in and out in steps.
7. Point at a detail and turn the mouse wheel up.
   **Expected:** it zooms in on that detail, which stays under the pointer. Wheel down goes back to Fit.
8. Press **Z**, then Z again.
   **Expected:** 100%, then Fit. A click on the picture does the same.
9. Tab to the slider and press → and ←.
   **Expected:** the zoom changes; it does not move to another photo.
10. Open a video.
    **Expected:** no zoom bar; the playback controls are as before.

---

## Every card imports by itself

Automated:
```
cargo test -p keepsake-core --test auto_import_tests
cargo test -p keepsake-core --lib -- template settings
cd app/ui; pnpm test; pnpm exec playwright test e2e/mocked/auto-import.spec.ts --project=mocked
```
**Expected:** all pass.

1. Open **Settings → When a card is inserted**.
   **Expected:**
   - **Copy new photos and videos automatically** is selected;
   - **Copy to** is `E:\Pictures`;
   - **Folders** is Year › Month › Day;
   - the example reads "A photo taken today goes to E:\Pictures\2026\October\2026-10-30\A7400123.ARW" (today's date).
2. Insert a camera card with a few new photos.
   **Expected:**
   - the Import window opens and says "Importing new items automatically to E:\Pictures, in folders by year, month and day";
   - it starts by itself, with no clicks;
   - only new items are copied; items already in the library are left on the card;
   - it ends with "Copied N items to E:\Pictures" and "NOT safe to format" (one copy only);
   - "Closing in 5 s…" counts down and the window closes by itself;
   - **Keep open** stops the countdown;
   - if anything failed, or you pressed Stop, it stays open.
3. In Explorer, open `E:\Pictures\<year>\<Month>\<year-month-day>\`.
   **Expected:**
   - the photos are there; any missing year, month or day folders were made;
   - no `.xmp` files appear next to them.
4. In Keepsake, open **By folder → Pictures → <year> → <Month>**.
   **Expected:**
   - the new day's folder, with its count;
   - the calendar shows that day.
5. Remove the card and insert it again.
   **Expected:** the window opens with nothing new to copy ("everything is already in your library"), or does not open at all.
6. Disconnect E: (or set Copy to a missing drive, e.g. `Z:\Pictures`), then insert a card.
   **Expected:** "Not imported automatically: destination … does not exist". Nothing is copied.
7. Choose **Open the import window, and let me choose** and save.
   **Expected:** the next card opens the window without starting.
8. Check the card in Explorer afterwards.
   **Expected:** nothing on it was changed or deleted.

---

## The sidebar tree by folder

Automated:
```
cargo test -p keepsake-core --test folder_tree_tests
cd app/ui; pnpm test; pnpm exec playwright test e2e/mocked/folder-tree.spec.ts --project=mocked
```
**Expected:** all pass.

1. Under **Library** in the sidebar, click **By folder**.
   **Expected:**
   - the calendar tree is replaced by your folders (e.g. **Pictures**) with their counts and connected dot;
   - the separate Volumes list is gone.
2. Click **Pictures**.
   **Expected:**
   - the grid shows everything in it;
   - its folders appear below (2025, 2026, Edited JPEG…), each with a count.
3. Click **2025**, then **May**.
   **Expected:**
   - the heading reads "Pictures › 2025 › May";
   - the status bar's count matches the tree;
   - the grid holds only that folder and its sub-folders.
4. Click only the ▸ arrow of another folder.
   **Expected:** it opens or closes; the grid does not change.
5. Set Type to Video.
   **Expected:** the counts change; folders without videos disappear.
6. Restart Keepsake.
   **Expected:** the sidebar is still **By folder**.
7. Click **By date**.
   **Expected:** the calendar tree and the Volumes list are back.
8. Keyboard: Tab to the sidebar and ↓ to Pictures.
   - → opens it; → again goes to its first folder;
   - ← goes back up; Enter shows it.

---

## All metadata in the inspector

Automated:
```
cargo test -p keepsake-core --lib meta_tests
cargo test -p keepsake-core --test probe_tests every_tag
cd app/ui; pnpm test; pnpm exec playwright test e2e/mocked/metadata.spec.ts --project=mocked
```
**Expected:** all pass.

1. Select an ARW photo. At the foot of the right-hand panel, click **All metadata**.
   **Expected:**
   - within about a second: "All metadata (300 or so)";
   - groups in this order:
     - Camera and image (IFD0)
     - Exposure (EXIF)
     - Sony maker notes
     - Computed by ExifTool
     - the file's inner structure (IFD1, SubIFD, SR2…)
     - File
     - System
   - values match the camera, e.g. Exposure Time 1/250, ISO 1600, Lens ID Sigma 24-70mm F2.8 DG DN | A.
2. Type `iso` in the filter.
   **Expected:** only tags whose name or value contains "iso". Clear it.
3. Click **Copy** and paste into Notepad (macOS: TextEdit).
   **Expected:** the file name, then `[Group]` blocks with `Name: value` lines.
4. Select another photo.
   **Expected:** the section stays open and shows that photo's tags. It is still open after restarting Keepsake.
5. Select a RAW + JPEG pair.
   **Expected:**
   - JPEG and RAW buttons;
   - RAW shows the ARW's tags, with many more maker notes.
6. Select an item on an unplugged drive.
   **Expected:** "Connect the drive with this item to read its metadata."
7. Check the file afterwards.
   **Expected:** its modified time and size are unchanged.

---

## Calendar tree in the sidebar

Automated: `cd app/ui; pnpm exec playwright test e2e/mocked/calendar-tree.spec.ts --project=mocked`.
**Expected:** passes.

1. Look at the sidebar under Library.
   **Expected:** **Calendar ▾** with every year that has items, each with its count, and "No date" if some items have no date.
2. Click a year.
   **Expected:**
   - the main area shows its months;
   - the year opens in the tree, listing its months with counts;
   - the year is highlighted.
3. Click a month.
   **Expected:** the main area shows its days; the tree lists the days (e.g. "Sun 15") with counts.
4. Click a day.
   **Expected:** the grid of that day; the heading reads Calendar › year › month › day; the count matches the tree.
5. Click only the ▸ arrow of another year.
   **Expected:** it opens or closes in the tree; the main area does not change.
6. Move through the calendar view itself (Ctrl+1, then click a year, then a month).
   **Expected:** the tree opens to the same place.
7. Keyboard: Tab to the sidebar and ↓ to a year.
   - → opens it; → again goes to its first month;
   - ← goes back to the year; ← again closes it;
   - Enter opens it in the main area.
8. Set a filter (e.g. Type: Video).
   **Expected:** the tree's counts change to match.

---

## Open in: Photoshop and Topaz Photo

Automated:
```
cargo test -p keepsake-core --lib settings
cargo test -p keepsake-app --lib
cd app/ui; pnpm exec playwright test e2e/mocked/open-in.spec.ts e2e/mocked/a11y.spec.ts --project=mocked
```
**Expected:** all pass.

1. **Settings → Open in.** Add a row:
   - types `arw, jpg, dng`;
   - Browse… to `C:\Program Files\Adobe\Adobe Photoshop 2026\Photoshop.exe` (macOS: `Adobe Photoshop 2026.app`).

   Then add a second row: the same types, with `C:\Program Files\Topaz Labs LLC\Topaz Photo\Topaz Photo.exe` (macOS: `Topaz Photo.app`). Click **Save**.
   **Expected:**
   - one row per type and application;
   - the first row of each type shows "(Ctrl+O)";
   - `config.toml` has `arw = ['…Photoshop.exe', '…Topaz Photo.exe']`.
2. Select a JPEG or ARW and click **Open in… ▾** in the right-hand panel.
   **Expected:**
   - the menu lists Photoshop (marked Ctrl+O), Topaz Photo and System default;
   - the first entry has focus.
3. Press ↓ then Enter.
   **Expected:**
   - Topaz Photo opens the file;
   - the panel says "Opened with Topaz Photo";
   - focus is back on the button.
4. Press **Ctrl+O** (macOS: Cmd+O).
   **Expected:** Photoshop opens the file.
5. Open the viewer (Enter), click **Open in… ▾**, then press Escape.
   **Expected:** the menu closes and the viewer stays open.
6. Pick **Photoshop** from the viewer's menu.
   **Expected:** it opens.
7. On a RAW + JPEG pair, press R (to show the RAW), then use Open in.
   **Expected:** the ARW is opened.
8. Select a video and open the menu.
   **Expected:** only System default (unless an app is set for mp4).

---

## Add a folder in place

## A. Automated tests

```
cargo test -p keepsake-core --test add_folder_tests
cargo test -p keepsake-cli --test cli_tests add_folder
cd app/ui; pnpm exec playwright test e2e/mocked/folders.spec.ts e2e/mocked/a11y.spec.ts --project=mocked
```
**Expected:** all pass.

---

## B. Add a folder of photos you already have (the app)

1. Before you start, note the folder's file count and size. Windows: right-click → Properties. macOS: Get Info.
2. In Keepsake, with an empty library, the grid says "Nothing here…" and shows **Add a folder…**. Click it.
   **Expected:** the **Folders** screen opens. Folders is also in the sidebar under Tools.
3. Click **Browse…**, pick the folder (e.g. `E:\Pictures\2025`) and click **Add folder**.
   **Expected:**
   - a progress bar with files, bytes and the file being read; then "Reading dates and cameras", then "Saving";
   - you can switch to other screens meanwhile;
   - at the end: "Added 2025: N items", "Nothing in the folder was changed", how many took ratings from `.xmp` files, and how many other files were left out.
4. Click **Show them**.
   **Expected:** the grid shows the folder's items, with dates and cameras. "2025" is in the sidebar under Volumes. Thumbnails appear over the next minutes.
5. Photos you rated in Lightroom show those stars.
6. Rate a photo and give it a label in Keepsake.
7. Check the folder again (as in step 1).
   **Expected:** the same file count and size. No new `.xmp` files appear next to your photos. Keepsake's sidecars are in the library folder, under `sidecars\`.

## C. Stop, scan again, overlaps

1. Add a large folder and click **Stop** partway.
   **Expected:** "Stopped. Nothing was added"; nothing new in the sidebar.
2. Copy a new photo into an added folder, then click **Scan again** next to it on the Folders screen.
   **Expected:** the count goes up by one. Earlier ratings are kept.
3. Try to add a folder **inside** one you added (e.g. `E:\Pictures\2025\January`).
   **Expected:** an error saying it is inside a folder already in the library. Nothing changes.
4. Try to add a folder that **holds** one you added (e.g. `E:\Pictures`).
   **Expected:** an error saying it holds a folder already in the library.

## D. A library made inside the photo folder

1. Create a library **in** a photo folder (Settings → Open another library → create, choosing e.g. `E:\Pictures\2025`). Then add that same folder.
   **Expected:**
   - only photos and videos are added: no `catalog.db`, `.json` or `sidecars` items;
   - after rating something, **Scan again** adds no extra items, and the rating stays.

## E. The command line

```
keepsake add-folder "E:\Pictures\2025"
```
**Expected:**
- a progress bar, then "Added 2025 (E:\Pictures\2025): N items, N files, N with Lightroom or Keepsake sidecars";
- Ctrl-C stops: "Stopped. Nothing was added";
- adding `E:\Pictures\2025\January` afterwards fails (exit code 1) with "already in the library".

## F. Accessibility

1. Keyboard only: Tab to the sidebar, arrow down to **Folders**, press Enter. Tab to the folder field, type a path and press Enter.
   **Expected:** the folder is added.
2. With a screen reader (Narrator / VoiceOver):
   - the folder field is announced as "Folder to add";
   - progress is announced as a status.
