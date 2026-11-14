# Manual Test Steps — Read aircraft: misread registrations, the same-shot slider, Tag with AI to the same shot, Read aircraft: online lookup, the same plane, its burst, Read aircraft: the review window, Read aircraft reads more of what is painted, The sidebar tree keeps its place, Edit suggestions, Read aircraft for a folder or dates, Read aircraft, Face matching tuned, Faces step 5 (People in the Inspector, faces in the viewer), Faces step 4 (names in the sidecars), Faces step 3 (Name and find), Faces step 2 (searching photos for faces), Faces step 1 (the face models), Which AI tagged it, Tag with AI to All, Tags in the search bar, Tag with AI (Claude, ChatGPT or Gemini), Helpers end with the app, On-device AI (tags and same-day duplicates), Selecting many items to edit, Faster thumbnails on a network drive, Report rows open their photos, A full cache keeps the thumbnails, Scan again one folder (right-click), Previous and next in the viewer, No "database is locked" during a backlog, Older video types in added folders, Start-up while the library opens, Library report, Background fingerprints for added folders, Adding a large folder (saved as it goes), Viewer information panel, Calendar without empty days, Faster zoom, Thumbnail sizes and the zoom slider, Automatic card import, Folder tree, All metadata, Calendar tree, Open in menu, and Add a folder in place

Test on **Windows 11** and **macOS 14+**. Paths below are Windows examples; on macOS use a folder such as `~/Pictures/2025`.

Start the app from source (`cd app/ui; pnpm tauri dev`), or from `scripts\keepsake.bat` on Windows. **Close any running Keepsake first.**

---

## Read aircraft: misread registrations, the same-shot slider, Tag with AI to the same shot

Automated (run here and in CI):
```
cargo test -p keepsake-core --lib ai::aircraft
cargo test -p keepsake-core --test aircraft_tests
cd app/ui; pnpm exec vitest run src/unit/aircraftReview.test.ts src/unit/aircraft.test.ts
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/aircraft.spec.ts
```

Manual (both OSes):
1. Read aircraft on A7202143.jpg (Edited/WebSite/Air Brussels). **Expected:** Registration OO-SFO and Airline Brussels Airlines are offered. (Only the tail's copy is read; the small slanted one on the wingtip is not found.)
2. In the review window, find the fourth slider, **Same shot**, set to ±10 s. Move it to 30 s. **Expected:** the neighbours line reads "Taken within ±30 s by the same camera and lens"; more photos of a burst appear. Close and open the window again. **Expected:** it is still 30 s. Set it back to 10.
3. A burst shot with two lenses (or two cameras). **Expected:** photos taken with the other lens are not listed as neighbours.
4. Tag with AI (Claude, ChatGPT or Gemini) one photo of a burst whose photos have been analysed by On-device AI. **Expected:** the alike photos within the slider's seconds, from the same camera and lens, get the same tags; the inspector's "which AI tagged it" names that AI for them. Photos from another lens, or further apart in time, get nothing.
5. Read aircraft on the RAW A7202143.ARW (2019, January). **Expected:** OO-SFO and Brussels Airlines.
6. Read aircraft on A7202202.ARW (2019, January; C-FVLU). **Expected:** C-FVLU, Air Canada, Boeing 787-9 (adsbdb) and Boeing 787 Dreamliner all ticked.
7. Read aircraft on a folder or dates with hundreds of aircraft photos. **Expected:** the window fills in as they are read, and when reading ends it says "N of N photos read." within a few seconds, not "Reading…" for ever. Moving a slider re-sorts within a second or two; **Show more** brings thumbnails and neighbours for the next photos. **Accept all shown** gives a photo's keywords to its alike neighbours too, also for photos further down.
8. Read aircraft on A7202769.jpg (Edited/WebSite/Air Lingus). **Expected:** EI-EWR and Airbus A330-200; with the online lookup on, Aer Lingus from adsbdb.
9. Originals: unchanged.

---

## Read aircraft: online lookup, the same plane, its burst

Automated (run here and in CI):
```
cargo test -p keepsake-core --lib ai::aircraft
cargo test -p keepsake-core --test aircraft_tests
cd app/ui; pnpm exec vitest run src/unit/aircraftReview.test.ts
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/aircraft.spec.ts
```

Manual (both OSes). A library opened by this build moves to schema 17 (older builds cannot open it after); back the catalog up first.
1. Settings → Read aircraft: **Look registrations up online (adsbdb.com)** is off. Turn it on. **Expected:** the note says only the registration is sent.
2. Read aircraft on a photo whose registration is clear and current (for example an Air Canada C-F… or C-G…). **Expected:** a line "Registry (adsbdb): C-F…: <type>, <airline>, Canada", and Type and Airline marks labelled "(adsbdb)", ticked. If a painted type was read and differs (lower the Type slider to see it), it is unticked.
3. A registration no longer flying (VT-JEX). **Expected:** "adsbdb does not know VT-JEX"; only what was read is offered.
4. Read the same photo again. **Expected:** the registry line is there at once; adsbdb is not asked again (the log shows no new "aircraft lookup").
5. A photo from a burst (several shots seconds apart). **Expected:** "Taken within 10 s by the same camera": the near-identical ones ticked with "+2 s, 96% alike", a different plane unticked; photos not analysed by On-device AI say "not compared yet". Accept: the ticked ones get the same keywords (check one in the inspector).
6. After accepting a photo with a registration and an airline or type, if other photos in the library have that registration but lack them. **Expected:** a panel "N other photos of <registration> lack <…>" with **Add to N photos**; after it, those photos have the keywords. **Not now** closes it.
7. Turn the lookup off. **Expected:** no new lookups; answers kept before are still shown.
8. Originals: unchanged.

---

## Read aircraft: the review window

Automated (run here and in CI):
```
cargo test -p keepsake-core --lib ai::aircraft
cargo test -p keepsake-core --test aircraft_tests
cd app/ui; pnpm exec vitest run src/unit/aircraftReview.test.ts
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/aircraft.spec.ts
```

Manual (both OSes), with Read aircraft on. A library opened by this build moves to schema 16 (older builds cannot open it after).
1. Select a few aircraft photos → right-click → **Read aircraft**. **Expected:** a window in the middle of the screen, "Read aircraft: N photos", "Reading… x of N read" with Stop, three sliders (Registration 80%, Airline 85%, Type 70% the first time), and each photo appearing as it is read with what was found, ticked, each with its confidence (the type from the look says "(its look)"), and "Also read, below your confidence" for the rest.
2. Nothing is a keyword yet: select one of the photos (window closed or not) and look at its keywords. **Expected:** no new Aircraft keywords.
3. Move the Type slider down to 60. **Expected:** at once, painted types read between 60 and 70% are offered (in place of the look), with no reading again. Move Airline down: more airlines are offered, maybe some wrong ones; move it up: fewer.
4. Untick a wrong mark on one photo and press **Accept**. **Expected:** the photo leaves the list; its keywords are the ticked ones only, and the inspector's Tagged with AI shows them as from the Aircraft reader.
5. **Dismiss** another. **Expected:** it leaves the list, no keyword added. **Accept all shown** accepts the rest. Tick "Show accepted and dismissed too": every photo is there, marked Accepted or Dismissed.
6. Close and open Settings → Read aircraft. **Expected:** "Offered from: registration …%, airline …%, type …%" as last set. Read aircraft again: the sliders start there.
7. Right-click a folder (By folder) or a year, month or day → **Read aircraft**. **Expected:** the same window for its photos tagged Airplane, Helicopter or Airport. Photos read with an earlier version are read again; those read with this one are shown at once.
8. **Stop** while reading. **Expected:** "Stopped: x of N read."; what was read can still be reviewed.
9. With Read aircraft off: the window says "Turn on Read aircraft in Settings first." with **Open Settings**.
10. Originals: unchanged (only read).

---

## Read aircraft reads more of what is painted

Automated (run here and in CI):
```
cargo test -p keepsake-core --lib ai::aircraft
cargo test -p keepsake-core --test aircraft_tests
```

Manual (both OSes), with Read aircraft on:
1. Find a photo taken from below with the registration painted under the wing (for example DSC05834 of 24 December 2017). Right-click it → **Read aircraft**. **Expected:** its keywords include `Aircraft|Registration|VT-JEX` (or that aircraft's registration).
2. A photo with the type painted by the door, in any mix of capitals (DSC05972: `Boeing 777-300ER`). **Expected:** `Aircraft|Type|Boeing 777-300ER`, and `Aircraft|Airline|Jet Airways`.
3. A photo of an airline that no longer flies (Air Berlin, US Airways, Jet Airways…). **Expected:** the airline is given.
4. A few photos that gave nothing before and have no readable text. **Expected:** still nothing (no made-up marks).
5. Photos already read are not read again by a folder or dates: select them and choose Read aircraft to read them with this version.

---

## The sidebar tree keeps its place

Automated (run here and in CI):
```
cd app/ui; pnpm exec vitest run src/unit/treeMemory.test.ts
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/tree-place.spec.ts
```

Manual (both OSes):
1. Sidebar → **By date**. Open several years and months so the tree is longer than the window, and scroll it down to a month near the bottom.
2. Right-click that month → **Tag photos**. **Expected:** while the window counts up and when it finishes, the tree stays where it is: it does not jump to the top, and no year or month closes. The counts may change in place.
3. **By folder**: open a drive and some folders, scroll down, right-click a folder → **Scan again** (or Read aircraft). **Expected:** the same: the tree stays where it is.
4. Right-click any entry and press Esc, or click elsewhere. **Expected:** the tree does not move.
5. Scroll each tree to a different place, close Keepsake and start it again. **Expected:** the tree you left open comes back with the same years, months or folders open and scrolled to where it was (after a moment, as its levels load). Switch to the other tree: it is where you left that one.
6. Change a filter (a camera, a tag). **Expected:** the tree keeps its open levels and its place; only the counts change.

---

## Edit suggestions (ADR-042)

Automated (run here and in CI):
```
cargo test -p keepsake-core --lib ai::suggest
cargo test -p keepsake-core --test suggest_tests
cd app/ui; pnpm exec vitest run src/unit/editAdvice.test.ts
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/edit-advice.spec.ts
```

Before you start: Settings → Open in has Photoshop and Topaz Photo for your photo types (for example `arw, jpg`). For step 5, Settings → Tag with AI has a key for at least one AI. Use a test library or note that the suggestions are stored in the catalog (nothing is written next to the photos).

Manual (both OSes):
1. Right-click a photo in the grid. **Expected:** the menu has **Edit suggestions…** below Name and find. On a video it is greyed out.
2. Choose it. **Expected:** a window "Edit suggestions: <name>", briefly "Measuring the photo…", then a sentence ("Keepsake measured the photo and found N things to look at." or "Nothing stands out…") and a numbered list. Each item has a title, why, a **Photoshop:** line naming a Camera Raw slider or tool with a value, and, where Topaz Photo has a tool for it, a **Topaz Photo:** line.
3. Try a dark photo, a photo with a white sky, a high-ISO night photo and a photo with a tilted horizon. **Expected:** respectively "Brighten the photo" (Exposure +…), "Recover the highlights", "Reduce the noise" (Denoise), "Straighten it" (about N° clockwise or counter-clockwise, the right way round). A well-exposed photo gets few or none. Open **What Keepsake measured**: eight lines (Brightness … Level) that match what you see.
4. **Open in Photoshop**. **Expected:** the original opens in Photoshop; the window says "Opened with Photoshop". The same for **Open in Topaz Photo**. With a program not set up for that file type, its button is greyed out and its tooltip says to add it in Settings → Open in.
5. Under the list: the line says it sends a 1024-pixel preview and the measurements to the AI, by its company, and roughly what it costs, and that the original is not sent. With keys for more than one AI, pick one. Press **Ask Claude** (or ChatGPT / Gemini). **Expected:** "Asking Claude…", then within a minute "From Claude (<model>)" with a summary and suggestions that mention what is in the photo (for example something to remove, or a crop). No person is named.
6. Close the window and open it again on the same photo. **Expected:** everything, the AI's answer too, is shown at once (no "Asking…", nothing sent again). **Measure again** measures anew and the AI's answer is gone; **Ask … again** asks again.
7. Press Ask and then **Stop** at once. **Expected:** "Asking…" goes away and nothing is charged (if it was already sent, its answer may still appear next time).
8. With no AI key at all. **Expected:** a line saying to add a key in Settings → Tag with AI; no Ask button.
9. A wrong key (edit it in Settings to something invalid but of the right shape). **Expected:** "<AI> could not answer: <AI> did not accept the API key: check it in Settings."
10. A photo with no thumbnail yet (just added). **Expected:** "Making the preview first…", then the suggestions.
11. Open a photo in the viewer. **Expected:** an **Edit suggestions** button in the bar; it opens the same window. Esc closes it.
12. Originals: compare a photo's file size and modified time before and after steps 2–7: unchanged. No new file next to it.

---

## Read aircraft for a folder or dates (ADR-041)

Automated (run here and in CI):
```
cargo test -p keepsake-core --test aircraft_tests
cd app/ui; pnpm exec vitest run src/unit/aircraft.test.ts
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/aircraft.spec.ts e2e/mocked/folder-tree.spec.ts e2e/mocked/ai.spec.ts
```

Manual (both OSes), with Read aircraft on (Settings) and a folder whose photos were tagged (Tag photos):
1. Sidebar → **By folder**. Right-click a folder with aircraft photos → **Read aircraft**. **Expected:** a window in the corner, "Reading aircraft in <drive> › <folder>", saying it reads the photos tagged Airplane, Helicopter or Airport in this folder; then "N of M read…" counting up, where M is the aircraft photos in the folder and the folders in it (fewer than the folder holds).
2. Let it finish. **Expected:** "M of M aircraft photos read." Select one of them: its keywords include what is painted on the aircraft (`Aircraft|Registration|…`, `Aircraft|Airline|…`), and Tagged with AI has **Aircraft reader**. A photo in the folder with no aircraft tag has no new keywords.
3. Right-click the same folder → Read aircraft again. **Expected:** it finishes at once ("M of M aircraft photos read."): no photo is read twice. Add new aircraft photos to the folder, tag them, and ask again: only the new ones are read.
4. **Stop** part-way. **Expected:** "Stopped: N of M read. Keywords already added stay." Asking again carries on with the rest.
5. A folder never tagged (or with no aircraft). **Expected:** "No photos tagged Airplane, Helicopter or Airport in this folder. Tag the photos first (Tag photos)."
6. Sidebar → **By date**. Right-click a year, month or day → **Read aircraft**. **Expected:** the same for the photos of those dates ("Reading aircraft in 2024").
7. With Read aircraft off. **Expected:** "Could not start: Turn on Read aircraft in Settings first." and **Open Settings**.
8. Originals: unchanged, as for Read aircraft on picked photos.

---

## Read aircraft (ADR-041)

Automated (run here and in CI):
```
cargo test -p keepsake-core --lib ai::aircraft
cargo test -p keepsake-core --test aircraft_tests --test xmp_tests
cd app/ui; pnpm exec vitest run src/unit/aircraft.test.ts
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/aircraft.spec.ts
```
With the real models and a folder of aircraft photos of your own (never committed):
```
$env:KEEPSAKE_AIRCRAFT_SAMPLES = "C:\path\to\a few aircraft photos"
cargo test -p keepsake-core --test aircraft_tests -- --ignored --nocapture
```

Manual (both OSes):
1. Settings → **Read aircraft**. **Expected:** "Off." and "Turning it on downloads two text models once (95.9 MB)."
2. Tick **Read aircraft**. **Expected:** a download bar with Cancel; then "Ready." (after the first photo is read: "Ready, on the graphics card (DirectML)." on Windows, Core ML on a Mac). With On-device AI off it says the type is only read when painted on.
3. In the library, select a few photos of airliners taken side-on, close enough that you can read the registration when zoomed in. Right-click → **Read aircraft**. **Expected:** a window in the corner, "Reading aircraft: N photos", counting up; a few seconds a photo (longer for RAW files on a network drive). **Stop** leaves what was read; **Hide** makes it a small button.
4. When it says "N of N photos read.", close it and select one of the photos. **Expected:** the Inspector's keywords include what is painted on the aircraft, for example `Aircraft|Registration|C-FJZS`, `Aircraft|Airline|Air Canada` and, where the type is written by the door, `Aircraft|Type|Boeing 777-300ER`. Under **Tagged with AI** there is a row **Aircraft reader** with those keywords.
5. Type a registration (or an airline) in the search bar and pick the tag. **Expected:** the photos of that aircraft.
6. A photo with no type painted on it: **Expected:** often no `Type` keyword. When there is one (for example `Aircraft|Type|Boeing 737`), it is a guess from the look: check it. It is rarely given, and wide-bodies are often not told apart.
7. A photo that is not of an aircraft, or with no readable text: **Expected:** it is read and gets no keywords.
8. Read the same photos again. **Expected:** no keyword twice. A keyword you removed comes back if it is read again (a known limit).
9. Open a photo and press **Read aircraft** in the viewer. **Expected:** the same window for that one photo.
10. With Read aircraft off, right-click → Read aircraft. **Expected:** "Could not start: Turn on Read aircraft in Settings first." and **Open Settings**.
11. Disconnect the drive the photos are on and read one that has a thumbnail. **Expected:** it is read from the thumbnail: usually the airline only. One without a thumbnail waits.
12. Check an original's size and modified date before and after (or `keepsake verify`). **Expected:** unchanged. Its XMP sidecar (or the library's copy, for a folder added read-only) has the keywords and `keepsake:AITagSources` rows with `By` `aircraft`.
13. Untick Read aircraft, then **Delete the downloaded text models**. **Expected:** the keywords stay; the 95.9 MB is freed.

Command line:
```
keepsake ai aircraft status
keepsake ai aircraft read C:\path\to\photo.jpg
keepsake ai aircraft scan --tagged Airplane --limit 20
```
**Expected:** `status` names the two model files (or says how to install them); `read` prints the registrations, airlines and types found, what the picture looks like (three types with their probabilities) and every line of text with its confidence, and changes nothing; `scan` reads the items with that tag and adds the keywords, as the app does.

---

## CI on macos-15

Nothing to test in the app. On the pull request, check that a check named **macos-15 (Rust)** runs and passes next to **windows-latest**, and that no check named macos-14 is left waiting.

On a Mac: click a photo, then Ctrl+click another. Expected: both are selected and no menu opens; Ctrl+click it again and it is no longer selected. A two-finger click (or right-click) still opens the photo menu. On Windows: Ctrl+click toggles the selection and a right-click opens the menu.

---

## Face matching tuned (ADR-040)

Automated (run here and in CI):
```
cargo test -p keepsake-core --test face_store_tests
cd app/ui; pnpm exec playwright test --project=mocked e2e/mocked/name-find.spec.ts e2e/mocked/people.spec.ts
```

Manual (both OSes), on a library where people are named:
1. Open Name and find on any photo. **Expected:** the suggestions waiting for each person are scored again: far fewer remain, and those are clearly them (on the product owner's library, about 16,000 became about 900).
2. Name and find someone; in Review, move **Looser ↔ Stricter**. **Expected:** after a moment the suggestions change (fewer when stricter); the next time the window opens it remembers the setting.
3. A photo of someone who was wrongly confirmed (e.g. the bald man with glasses confirmed as a woman): **Expected:** his other photos are no longer suggested for her.
4. After Confirm, press **Check <name>'s faces** (or click a name in the Inspector's **People**). **Expected:** her confirmed faces, least alike first (with a %); the wrong ones are near the top. Click them and press **Remove N faces from <name>**: **Expected:** they go from the list, their People keyword and face region are taken off, and they are not suggested for her again.
5. Very small faces (far away in group photos) are no longer suggested.

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

## Helpers end with the app

Automated:
```
cargo test -p keepsake-core --test probe_tests shutdown
cargo test -p keepsake-platform job
```

Manual:
1. **Windows:** start Keepsake and browse a few photos, or import a card, so ExifTool starts. In Task Manager → Details, sort by name. **Expected:** one or two `exiftool.exe`. Close Keepsake with ✕. **Expected:** within about 5 s no `exiftool.exe`, `ffmpeg.exe` or Keepsake `msedgewebview2.exe` is left.
2. **Windows:** start Keepsake again and end `keepsake-app.exe` in Task Manager (End task), as a crash would. **Expected:** its `exiftool.exe` disappears at once too.
3. **Windows:** right-click a photo → Open in → an app (or Show in Explorer), then close Keepsake. **Expected:** the opened app and the Explorer window stay open.
4. **Windows:** run the installer over an installed Keepsake after using it and closing it. **Expected:** no "Error opening file for writing … exiftool.exe".
5. **macOS:** start Keepsake, browse some photos, quit it (⌘Q). In Activity Monitor search "exiftool". **Expected:** nothing left after about 5 s.

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
