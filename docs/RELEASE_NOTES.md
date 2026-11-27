# Keepsake 0.12.1

Version 0.12.1 fixes upgrading Keepsake on Windows. It has everything in 0.12.0. No catalog change: 0.12.0 and 0.12.1 open the same library.

## Fixed in 0.12.1

**Upgrading on Windows**
- Installing a newer Keepsake over an older one now installs straight over it. It no longer asks to uninstall the old version first. Keepsake is closed for you if it is open (the installer asks first).
- This fixes "Error launching installer" when upgrading from 0.11.7 or earlier, and "Unable to uninstall" when Keepsake was open.
- Your library, thumbnails, AI models and settings are kept, as before.
- **Upgrading from 0.12.0:** the installer you run is 0.12.1's, so it uses the fixed behaviour.
- Reinstalling the same version, or installing an older one, still shows the "Already Installed" choice.

# Keepsake 0.12.0

Version 0.12.0 is Keepsake's first release under a licence. It has everything in 0.11.7. No catalog change: 0.11.7 and 0.12.0 open the same library.

## Copyright and licence

- Keepsake is **Copyright © 2026 Ahmad Elkays**. Tools → About and the installer now show it.
- The source code is available under the **Functional Source License 1.1 (FSL-1.1-ALv2)**. You may use, change and share it for anything except offering it, or something like it, as a commercial product. Each version also becomes Apache 2.0 two years after its release. See `LICENSE.md`.
- Earlier versions were published without a licence.
- The third-party software inside Keepsake keeps its own licences, listed on the About screen.
- Contributions: bug reports are welcome as GitHub issues. Code contributions are not accepted for now (`CONTRIBUTING.md`).

## Also in this release

Everything from 0.11.6 and 0.11.7:
- **Help** (Tools → Help): how-to topics with Find, the keyboard shortcuts, and Copy diagnostic report. Settings shows the version.
- **Sidebar trees By aircraft and By face**, and faster keyword searches.

# Keepsake 0.11.7

Version 0.11.7 adds a Help screen and shows the version in Settings. It has everything in 0.11.6. No catalog change: 0.11.6 and 0.11.7 open the same library.

## New in 0.11.7

**Help** (Tools → Help, right after Settings)
- **How to:** 15 short topics with numbered steps, such as importing a card, adding a folder, browsing by date, folder, aircraft or face, searching, rating, the viewer, fixing the time, backup, duplicates, on-device AI, Read aircraft and Flickr. Type in Find to narrow them.
- **Keyboard shortcuts:** every shortcut in one table. On a Mac, use ⌘ where it says Ctrl.
- **Get help:** the version, and **Copy diagnostic report** to paste into a bug report.

**Settings**
- The version (`Keepsake 0.11.7`) is shown under the Settings heading.

## Documentation

- The system requirements for Windows and macOS are in `docs/SPEC.md`, with memory, disk and processor use measured on a 129,000-item library.

# Keepsake 0.11.6

Version 0.11.6 adds two new sidebar trees, By aircraft and By face, and makes keyword searches much faster. It has everything in 0.11.5. No catalog change: 0.11.5 and 0.11.6 open the same library.

## New in 0.11.6

**Sidebar: By aircraft** (the sidebar's tree buttons)
- Airline › Type › Registration › Year › Month, with photo counts. Click any line to open exactly its photos.
- Names are A to Z. **No airline named**, **No type named** and **No registration named** come last.
- Years are newest first, and **No date** comes last.

**Sidebar: By face**
- Person › Year › Month, A to Z, with **Unnamed faces** last for photos that have a face with no name.

## Faster in 0.11.6

**Keyword searches**
- Opening a line that names no type or registration (for example Porter › No type named › No registration named) took up to a minute or two. It now opens in a fraction of a second, and so does any search by several keywords.

# Keepsake 0.11.5

Version 0.11.5 reads Porter's registrations, and lets the aircraft report open the photos that name no airline. It has everything in 0.11.4. No catalog change: 0.11.4 and 0.11.5 open the same library.

## Fixed in 0.11.5

**Registrations with the flag painted after them** (Read aircraft)
- Porter paints the Canadian flag right after the registration's last letter, and it was read as part of it (`C-GZQXI*I`, `C-GZOBM`), so no Porter photo was offered its registration. The flag is now left out: A7_08885.ARW offers C-GZQX beside Porter Airlines and fin 929.
- A Canadian registration is always `C-` and four letters, so one or two characters more after them are taken as the flag.
- Photos read before 0.11.5 keep what they read: right-click their folder or dates → **Read aircraft again**.

**Aircraft report: lines with no airline open their photos** (Tools → Report → Aircraft)
- In Airline › Type › Registration, **No airline named** and the types and registrations under it (and any "No type named" or "No registration named") are now links like the others. Each opens exactly its photos: those with its keywords and none of the kind it does not name.

# Keepsake 0.11.4

Version 0.11.4 reads the fin (fleet) number airlines paint on their aircraft, and uses it to find the registration when the registration itself cannot be read. It has everything in 0.11.3.

**Catalog change:** 0.11.4 moves the library to catalog version 23 the first time it opens it. After that, 0.11.3 and older cannot open the library. Back up `catalog.db` first if you may go back.

## New in 0.11.4

**Fin numbers are read**
- Air Canada paints a number on the tail and the nose gear doors (147, 840); US airlines paint a ship number on the tail and by the cockpit. A number alone on the aircraft, 2 to 4 digits, is now read as its fin. It is never added as a keyword: the review window shows it as "Fin 840" beside the airline.

**The fin table** (Settings → Read aircraft → **Fin numbers**)
- Which registration each airline's fin stood for, and when. Keepsake learns it when you accept a photo with the registration, the airline and the fin read.
- **Learn from tagged photos** fills it from photos already tagged with their registration and airline.
- **Import CSV…** adds a list you made, one aircraft per line: `airline,fin,registration` and, if a fin was reused, `from` and `to` dates (`2019`, `2019-05` or `2019-05-05`). Commas, semicolons or tabs; a header line is skipped. Lines that cannot be kept are listed with the reason.
- **Export to CSV…** writes the table in the same columns, to edit and import again. **Show** lists it; **Remove** deletes a row. Two registrations for one fin at the same time are marked "(two at once)".
- No fin list comes with Keepsake: there is no free source of one.

**A registration from the fin**
- When a photo reads the airline and its fin but no registration, the registration the table gives for the photo's date is offered, labelled "(from Air Canada fin 147, 97%)". It is ticked when there is one answer and the aircraft type the lookup gives is what the photo shows; otherwise it is offered unticked. Its burst gets it too.

**Read aircraft again for a folder or dates**
- Right-click a folder, or a year, month or day → **Read aircraft again**: its aircraft photos are read again, those read before too, so they get their fins. Photos you already accepted or dismissed stay so and do not come back for review; their keywords stay. A tagged photo's fin goes into the table as it is read.
- **Read aircraft** for a folder or dates still reads only photos never read.

**Photos read before 0.11.4**
- They have no fins: use **Read aircraft again** on their folder or dates, then check Settings → Read aircraft → Fin numbers.
- Some fins cannot be read: white or faint digits on a patterned tail (Air Canada's 787 tails) are not found as text. Import those with a CSV line, for example `Air Canada,840,C-FGFZ`.

# Keepsake 0.11.3

Version 0.11.3 reads registrations that used to come out a letter short, and the registration on a plane that fills a large photo. It has everything in 0.11.2.

No catalog change: 0.11.3 opens a 0.11.2 library as it is.

## New in 0.11.3

**A registration missing a letter**
- When a line of text is read less than surely, it is read again in a slightly longer box. Even big, clear letters could lose one in a tight box: C-GFCH on the side of an A320 was read C-GCH, which is no registration, so none was offered. Now it is read C-GFCH.

**A plane that fills a large photo**
- A plane filling most of a large photo was read from the whole photo shrunk to fit, which made its registration too small to find. Now its area is also read on its own, at full size: C-GJYI on the side of an A220 was missed and is now read. It takes a few seconds more on such photos.

**Photos read before 0.11.3**
- Use **Read again**.

# Keepsake 0.11.2

Version 0.11.2 makes Read aircraft better at a plane passing overhead: a burst of photos counts as one shot, and a registration read one letter wrong is checked against aircraft that really exist. It has everything in 0.11.1.

No catalog change: 0.11.2 opens a 0.11.1 library as it is.

## New in 0.11.2

**A burst is one shot**
- The same shot used to be the photos taken within a few seconds either side of one photo. Now it follows a burst from one photo to the next: photos by the same camera and lens, each no more than the same-shot setting after the one before (up to 30 seconds either side).
- Accepting one photo gives its keywords to every ticked photo of the burst.
- The review window says what the burst's photos read, for example "Registration read on 3 of 21 photos (JA02NJ) of this shot", and offers what they read together.

**A registration read one letter wrong**
- When a registration is not a known aircraft, Keepsake checks the registrations one look-alike letter away (W for N, 0 for O, and so on) with adsbdb and, when adsbdb knows none of them, with your Flightradar24 lookup.
- One that is known is offered ticked, for example "JA02WJ (read JA02NJ 95%; Flightradar24 knows JA02WJ)", with what was read beside it unticked.
- Flightradar24 is asked only when **Look up a new registration on its own** is on, or with **Look up aircraft on Flightradar24**. A registration it does not find costs no credits; one it finds costs 2.
- Photos read before 0.11.2: use **Read again** for these checks to run.

# Keepsake 0.11.1

Version 0.11.1 makes Read aircraft better at small registrations, such as one painted under the wing of a plane high overhead. It has everything in 0.11.0.

No catalog change: 0.11.1 opens a 0.11.0 or 0.10.0 library as it is.

## Fixed in 0.11.1

**Small registrations are read again**
- When a registration is only a few pixels high, the box around it could stop short of the last letter, and nothing was offered. Read aircraft now reads small lines a second time with the box made a little longer at each end.
- In marks such as Japan's `JA` followed by two digits, a letter O read where a digit belongs is taken as a zero (also D, Q, I and L), so `JAO2…` becomes `JA02…`.
- Very small letters can still be read wrong (a W read as N, for example). Check a small registration against the photo before you accept it.

# Keepsake 0.11.0

Keepsake is a local-first replacement for Sony PlayMemories Home. It imports camera cards safely, keeps a calendar-first library of photos and videos, and backs it up, all on your own computer. Version 0.11.0 makes Add route choose OpenSky or Flightradar24 for each photo by the date it was taken.

No catalog change: 0.11.0 opens a 0.10.0 library as it is.

## New in 0.11.0

**Add route chooses where to find each flight**
- **Settings → Read aircraft → Flights from** has a new choice, **Automatic**, and it is the default for new settings. A photo from within your Flightradar24 plan's history is asked of Flightradar24; an older one is asked of OpenSky.
- Add route assumes the plan goes back 30 days. When Flightradar24 says your plan starts later, Add route remembers how far back it really goes, and that photo is asked of OpenSky instead, so it doesn't fail.
- With only one service set up, Add route uses it wherever it can. A photo older than your Flightradar24 plan, with no OpenSky client, says to add one.
- The Add route window says, for each photo, which service its flight came from.
- **OpenSky only** and **Flightradar24 only** are still there. Settings saved by an earlier version keep the service they name: choose **Automatic** once to use it.

# Keepsake 0.10.0

Keepsake is a local-first replacement for Sony PlayMemories Home. It imports camera cards safely, keeps a calendar-first library of photos and videos, and backs it up, all on your own computer. Version 0.10.0 makes Add route ask OpenSky far less often, and lets you type a flight number to reuse the route your other photos of that flight already have.

No catalog change: 0.10.0 opens a 0.9.0 library as it is.

## New in 0.10.0

**Add route asks OpenSky once per aircraft and day**
- When you pick many photos of the same aircraft, Add route asks OpenSky once for each day they were taken, not once per photo, and finds each photo's flight in that one answer.
- Each question covers one UTC day, the cheapest kind (4 of OpenSky's daily credits): photos either side of midnight make two.
- At OpenSky's daily limit, Add route still waits and then carries on by itself.

**Type a flight number: routes from your other photos**
- In the Add route window, open **Type the flight number** under a photo and type it (`AC856`; `ac 856` and the callsign `ACA856` work too).
- The from and to airports, with their countries, of your other photos tagged with that flight are offered, the route on the most photos first, then the most recent.
- One click adds that route. **Add only Flight …** adds just the flight number and keeps the airports the photo has. Nothing is added until you click, and nothing is looked up online.

# Keepsake 0.9.0

Keepsake is a local-first replacement for Sony PlayMemories Home. It imports camera cards safely, keeps a calendar-first library of photos and videos, and backs it up, all on your own computer. Version 0.9.0 makes the online features work behind antivirus HTTPS scanning, and shows the licences and credits of everything Keepsake uses.

No catalog change: 0.9.0 opens a 0.8.0 library as it is.

## New in 0.9.0

**Online features work with antivirus HTTPS scanning**
- Keepsake now checks secure connections against the computer's own trusted certificates (the Windows certificate store, the macOS Keychain), as a browser does.
- An antivirus that scans HTTPS (Avast's Web Shield, for example) or a company network that inspects it no longer makes Add route, registration look-ups, Flightradar24, Flickr or Tag with AI fail with "invalid peer certificate: UnknownIssuer".

**Tools → About**
- The version, and the credit each data source and service asks for: Flickr, The OpenSky Network, Transport Canada's register (Open Government Licence – Canada), the FAA register, OurAirports, adsbdb and Flightradar24, each with a link to its page.
- **Licences:** the programs shipped beside Keepsake (FFmpeg, ExifTool, LibRaw, ONNX Runtime, DirectML), then every component built into Keepsake with its licence. Find narrows the list; Show licence opens its text. Works offline.

**Third-party notices in the installer**
- `licenses/THIRD-PARTY-NOTICES.txt` in the install folder lists every component built into Keepsake with its licence text, and the programs shipped beside it.

# Keepsake 0.8.0

Keepsake is a local-first replacement for Sony PlayMemories Home. It imports camera cards safely, keeps a calendar-first library of photos and videos, and backs it up, all on your own computer. Version 0.8.0 uploads your photos to your own Flickr account and shows what is on that account.

**Before installing, back up your catalog.** 0.8.0 adds two catalog tables, and Keepsake does not back up the catalog before upgrading it. Close Keepsake and copy `catalog.db` to `catalog.pre-0.8.0.db` beside it.

## New in 0.8.0

**Flickr account** (Settings → Flickr)
- Uses your own Flickr API key (a free non-commercial key from Flickr's App Garden).
- **Sign in to Flickr…** opens Flickr's page in your browser; you type back the code it shows. There are also **Test the sign-in**, **Sign out** and **Remove key**.
- The key, secret and sign-in are kept only in Windows Credential Manager or the macOS Keychain, never in a file or the log.

**Upload to Flickr…** (right-click one or several photos)
- Choose who can see them: Public, Friends and family, Friends, Family or Private (the default).
- Check each photo's tags before anything is sent:
  - keywords flattened to their last part (`Aircraft|Registration|C-FEJC` → `C-FEJC`);
  - the camera and lens by default;
  - optional machine tags (`aircraft:registration=C-FEJC`) and parent keywords.
- Remove or add tags for one photo or for all of them.
- Uploads run in the background, one photo at a time. Each photo shows its result, Flickr's errors are explained in plain words, and failed network tries are retried by themselves.
- A photo already on Flickr says so and is not ticked again. JPEG, PNG and TIFF originals go up as they are; RAW and HEIF go up as a full-size JPEG. Originals are never changed.

**Report → Flickr**
- Everything on your Flickr account, read from Flickr (photos uploaded any other way too), as a tree by upload date: year › month › day with counts.
- Each day shows Flickr's thumbnails, each opening its Flickr page; photos uploaded with Keepsake are marked **K**.
- Read when you first open it after starting Keepsake and on **Refresh from Flickr**; nothing about it is kept in the catalog.

# Keepsake 0.7.0

Keepsake is a local-first replacement for Sony PlayMemories Home. It imports camera cards safely, keeps a calendar-first library of photos and videos, and backs it up, all on your own computer. Version 0.7.0 reads the Canadian and US aircraft registers, fills in a photo's missing route keywords, and lists every flight you photographed.

## New in 0.7.0

**National aircraft registers** (Settings → National aircraft registers)
- Downloads Transport Canada's and the FAA's registers on demand (updated only when they change) and keeps them in the catalog. No owner name of a person and no address is kept.
- Used to identify and tag aircraft: build year, type, maker and transponder code for C- and N- registrations; a misread registration (C-FJ2S) is put right (C-FJZS).
- The inspector shows the register's rows for the photo's aircraft.

**Flightradar24 for new registrations**
- **Look up N new registrations** asks only registrations not looked up yet; optionally done by itself when a photo is tagged with a new registration.

**Add route**
- **The flight in or out the source did not see** (CN-RGY landing from Casablanca, when OpenSky saw only its take-off) is offered first, its other end filled in from the flight number's route (adsbdb).
- **At OpenSky's limit, Add route waits** until OpenSky allows more and goes on by itself; the window says when.

**Fill in route keywords** (right-click photos)
- From a flight number, or from and to airports, adds what is missing: airports, countries and the flight number. What a photo has is never changed.
- Finds the flight number from Add route's flights, from a flight with the same number before and after, or from the number next to the one it flew the other way (AT210 for RAM211B).
- When two flight numbers fly the route (QK893 and QK895), you choose with one click.
- Each photo shows what it had and only what it got.

**Flight report** (Report → Flights)
- Every flight photographed: date, registration, airline, flight, from and to, with its photo count; Find and CSV.

**Airline codes:** Jazz (Air Canada Express, QK), Flair (F8) and WestJet Encore (WR).

# Keepsake 0.6.0

Keepsake is a local-first replacement for Sony PlayMemories Home. It imports camera cards safely, keeps a calendar-first library of photos and videos, and backs it up, all on your own computer. Version 0.6.0 tells you how old each aircraft is and which flight you photographed, from OpenSky or your own Flightradar24 subscription. It includes 0.5.4, which was not released on its own.

## New in 0.6.0

**The aircraft's age**
- For a photo with a registration, the inspector and the viewer's information panel show how old the aircraft is today and how old it was in the photo, worked out when shown.
- **Build years from OpenSky's aircraft database** (Settings → Read aircraft → Build years, a 95 MB download, kept on this computer), or typed once per registration in the inspector (**Aircraft built**). Each photo of it gets `Aircraft|Built|<year>`.

**Add route** (right-click photos → Add route…)
- Finds the flight you photographed: **from and to** (airport codes and countries) and the **flight number**, offered as keywords you tick and accept. Nothing is added until you accept.
- **From OpenSky** (free; by transponder code; history from about 2023) or **from Flightradar24** with your own API subscription (by registration; how far back depends on your plan; its answers are kept 30 days, as its terms require; the keywords you accept stay).
- **A flight the source did not see** (over an ocean) is worked out from the landing before and the take-off after, and offered beside the others for you to choose.
- **Two flights as near the photo:** you choose; the photo's position, when known, decides.
- **Ask again** on a photo, or for all. Accepting a route replaces the route keywords it had.
- **Transponder codes:** from adsbdb, from OpenSky's aircraft database, or typed in the window for an aircraft neither knows. A registration adsbdb did not know is asked again after 30 days.
- **Camera clock on Eastern time, changed for daylight saving:** for a camera that marks every photo UTC−05:00 though its clock is put forward in summer, Add route reads its photos as UTC−04:00 from March to November. The photos are not changed.
- **A photo's time zone** can be set in the window (UTC−12:00 to +14:00): the camera's clock time stays, its UTC time moves; kept in the catalog and the sidecar, never in the photo.

**Look up aircraft on Flightradar24** (Settings → Read aircraft)
- One background job asks your Flightradar24 API about every registration on your photos: its type, airline (from its livery), and transponder code, from its latest flight in the last four weeks. About 2 credits per aircraft found. Kept 30 days; shown in the inspector; its type and airline are offered in Read aircraft; its code lets OpenSky find the aircraft's older routes.

**Faster and steadier**
- **A photo you ask about is read next,** ahead of a backlog of queued photos.
- **The catalog's change log is cut back** after it is written into the catalog (64 MB at most): a long run had left it at 1.4 GB, and every start read through it.

API keys and tokens (OpenSky, Flightradar24) are kept in Windows Credential Manager or the macOS Keychain, never in a file.

## New in 0.5.4 (included; not released on its own)

**Read aircraft**
- **Only the text on the aircraft is read:** an aircraft finder (YOLOX-s) finds the aircraft first, so signs, buildings and a signature are no longer read.
- **A photo you pick always shows what was read.** A photo that got its keywords earlier as another photo's neighbour was read but not listed; now it waits for review like any other. Its keywords stay.
- **"No registration, airline or type was found on this photo"** is said when a read finds nothing.
- **Read again** on each photo in the window, and **Read all again** for all of them (it asks first above 20 photos). A new read replaces the one kept and waits for review.
- The window lists only photos with something to accept.

**Aircraft report**
- It opens on a drill-down, **Airline › Type › Registration**: each airline with its photos; open it for its types, and a type for its registrations, each with its count. Expand all, Collapse all and Find. A line opens exactly its photos. The CSV has the same lines.

**Fixes**
- **The app was slow** when raw files without a built-in preview had faces, tags or aircraft asked for them: their thumbnails were tried again every 15 seconds without end. They are now tried once.

## New in 0.5.3 (included)

**Aircraft report** (Tools → Report → Aircraft)
- How many aircraft photos you have (and how many distinct shots, with copies in other folders counted once), how many airlines, types and registrations are named on them, and how many reads are waiting for review.
- Aircraft photos by year.
- One table each for **airlines**, **types** and **registrations**, most photos first, with Find: photos, registrations, main airlines and types, and the first and last day seen. Click a name to see its photos.
- **Export to CSV…** for Excel.

**Read aircraft reads more**
- **Distant aircraft:** when an airline's name is read but no registration, the fuselage around the name is read again at full size (a registration 15 pixels high is now found).
- **Letters misread as digits** in a registration are put right (`00-SFO` → OO-SFO), and a capital I misread as a small l (`El-EWR` → EI-EWR).
- **An Airbus model written alone** (`A330-200`) is its type.
- A painted type said less exactly ("Boeing 787 Dreamliner") agrees with the registry's ("Boeing 787-9") and stays ticked.
- **A same-shot slider** (default 10 seconds) sets how close in time photos must be to get the same keywords; **Tag with AI** gives its tags to identical photos within it (same camera and lens).

**Fixes**
- Right-click a month with fewer than 31 days → Read aircraft, Tag photos or Find duplicates no longer gives a "not a date" error.
- The Read aircraft window no longer stays on "Reading…" with hundreds of photos: only the photos on screen get thumbnails and neighbours.

Photos read before 0.5.3 keep their saved reads; to apply the new reading to them, right-click → Read aircraft on those photos (reading a whole folder skips photos already read).

## New in 0.5.2 (included)

**Read aircraft**
- **Look registrations up online** (Settings → Read aircraft; off until you turn it on): each registration read is looked up once at the free adsbdb.com. Only the registration's text is sent, never a photo. The type and airline it knows are offered in the review window, marked "(adsbdb)"; a painted type it disagrees with is offered unticked. Aircraft no longer flying may be unknown there.
- **The same plane's other photos:** after you accept a photo's registration with its airline or type, the window offers them to your other photos of that registration that lack them (**Add to N photos**).
- **The rest of a burst:** photos taken by the same camera within 10 seconds that look alike (On-device AI's comparison) are ticked under the photo, and get the same keywords when you accept.
- **Names partly hidden** by an engine or a wing are read (Air Transat as `trainsa`), and a registry's company name counts as the airline ("Emirates Airline" → Emirates).

**Fixes**
- The filters at the top could leave the grid empty: the work that marks the photos on screen held the library up. It now runs on its own.
- Photos that cannot be rendered are no longer tried again every time they are on screen.
- **Show in Explorer** works for photos on a network drive, and **Open in** hands them over by drive letter (`Z:\…`), so Topaz Photo opens them. If Topaz Photo is already open, Keepsake says so: close it and choose Open in again.

**Upgrading from 0.5.1:** the first time 0.5.2 or later opens your library it updates the catalog (schema 17). Older versions cannot open it after that, so install the same version on every computer that opens the library.

## New in 0.5.1 (included)

**Read aircraft: you decide what is kept**
- Every Read aircraft (the photos you pick, the viewer, a folder, a year, month or day) opens a review window in the middle of the screen. Photos appear as they are read.
- Three sliders set how sure a **registration**, an **airline** and a **type** must be to be offered. Moving one re-sorts every photo at once (nothing is read again), and they are remembered.
- Each photo shows what was found, ticked, with how surely it was read, and what was read below your levels. Untick mistakes, then **Accept**, **Dismiss**, or **Accept all shown**. Nothing becomes a keyword until you accept it.

**Read aircraft reads more**
- Registrations painted under the wing (upside down in a photo from below) are read.
- Painted types read a little less clearly (`Boeing 777-300ER`) are kept.
- Airlines that no longer fly are known, for older photos: Jet Airways, Air Berlin, Kingfisher, Germanwings, Monarch, Thomas Cook, Virgin America, US Airways, Continental, Northwest.

**Also**
- The sidebar tree keeps its place after a right-click action and over a restart.

Photos read by Read aircraft before 0.5.1 keep their keywords; to review them in the window, read them again (a folder or dates does this by itself for photos read before).

**This release is for Windows 11 (x64 and ARM64).** macOS will follow in a later release.

## Install

Download the installer for your PC:

| PC | File |
|---|---|
| Most PCs (Intel or AMD) | `Keepsake_0.5.3_x64_en-US.msi` or `Keepsake_0.5.3_x64-setup.exe` |
| ARM PCs (Snapdragon) | `Keepsake_0.5.3_arm64_en-US.msi` or `Keepsake_0.5.3_arm64-setup.exe` |

The installers are **not code-signed yet**, so Windows SmartScreen may warn you. Click **More info → Run anyway**.

Everything Keepsake needs is included: FFmpeg, ExifTool and LibRaw. Keepsake connects to the internet only when you ask it to: to download an AI model you turn on in Settings, or to send a photo's small preview to Claude, ChatGPT or Gemini (Tag with AI, Ask in Edit suggestions).

**Upgrading from 0.1.0:** the catalog is updated the first time 0.5.2 opens your library; older versions cannot open it after that.

## New in 0.5.0 (included)

**Your existing folders**
- **Add a folder in place**: photos you already have are catalogued where they are, read-only. Large folders fill in as they are read and resume if stopped; full checks run in the background. **Scan again** (right-click a folder) finds new, changed and removed files.
- **By folder**: the sidebar shows your drives and folders as they are on disk, beside the calendar's years, months and days, each with its count. The tree keeps its place and comes back where you left it.
- **Every card imports by itself** into year › month › day, if you turn it on.

**Browsing**
- Thumbnail sizes (Small, Medium, Large), much faster thumbnails from a network drive, and thumbnails that stay when the cache is full.
- Select many items (tick-boxes, Select mode, drag a rectangle) to edit them together.
- The viewer: a zoom slider, faster zoom, previous and next, and an information panel with every metadata tag.
- **Show in Explorer / Finder**, and **Open in…** with several programs per file type (for example Photoshop and Topaz Photo).
- **Report** (Tools): your photos, videos and space by year, month, camera, lens, file type and drive; rows open their photos; export to CSV.

**AI, on this computer**
- **Tag photos**: tags such as Beach, Dog or Sunset from what a photo shows, for a folder, a date or everything, searchable like keywords.
- **Duplicates**: near-identical RAW photos of one day, grouped, so you can mark the keeper. Nothing is deleted.
- **Name and find**: name a face, and Keepsake suggests the other photos that person is in, across the years. You confirm each.
- **Read aircraft**: the registration, airline and type painted on aircraft, as keywords, for picked photos, a folder or dates.

**AI you ask for, from the cloud** (your own API key; you pay the provider)
- **Tag with AI**: Claude, ChatGPT or Gemini tag the photos you pick. Only a 1024-pixel preview is sent, never the original; the inspector shows which AI gave each keyword.
- **Edit suggestions** (right-click a photo): Keepsake measures the photo (brightness, highlights, shadows, contrast, colour, noise, sharpness, a tilted horizon) and lists what to improve, with the steps in Photoshop and in Topaz Photo. **Ask** an AI for advice that takes in what the photo shows. **Open in Photoshop** or **Open in Topaz Photo** from the same window.

## Also in Keepsake (since 0.1.0)


**Import cards safely**
- Insert a Sony card and Keepsake recognises it, even in another reader, and shows what is new since the last import.
- Import to two drives at once:
  - every copy is checked (BLAKE3) before it gets its final name;
  - files are renamed by template;
  - presets save your usual destinations;
  - live thumbnails come straight from the card.
- **SAFE TO FORMAT** appears only when every item is verified on two separate disks.
- An interrupted import (a pulled card, a crash, Stop) resumes where it left off and never leaves half-written files.

**Browse a large library**
- A calendar (years, months, days), a grid and a details table. They stay smooth at 300,000 items.
- Search by words, date, camera, lens, type and rating.
- Ratings, colour labels, comments and keywords are saved in standard XMP sidecars. Keepsake reads existing Lightroom and digiKam sidecars and keeps their other fields.
- Items on unplugged drives keep their thumbnails, marked offline.

**View**
- A full-screen viewer:
  - photos zoom to 100%, with RAW files decoded at full size;
  - RAW + JPEG pairs switch with R;
  - videos have frame stepping and the camera's timecode.
- Open any item in another application (Ctrl+O), set per file type.

**Fix times**
- Correct a camera clock for a whole shoot: pick a photo, enter the right time, check the preview, apply. Only the sidecars change.

**Back up and verify**
- Incremental, verified backups to another drive, with a copy of the library.
- Verify a drive at any time.
- A history of every backup and check, with the date each drive was last verified.

**Settings, logs and help**
- Settings cover the cache location and size, workers, Open in applications and import presets.
- Logs and crash reports stay on your computer. **Settings → Copy diagnostic report** gives you a text to paste into a bug report; your home folder is shown as `~`.

**Command line**
- `keepsake` imports, verifies, searches, shifts times, backs up and rebuilds the catalog. Every command has `--json`. See `docs/CLI.md`.

## Your files are safe

- Keepsake **never modifies, moves or deletes your original photos and videos**. Edits go into XMP sidecars and the library.
- Cards and read-only drives are never written to. Tests lock them at the operating-system level and check every file is byte-for-byte unchanged.
- Every place Keepsake writes to disk is listed, with its test, in `docs/SAFETY.md`.

## Known limits

- **Windows only** in this release. The Mac version is built and tested automatically but not packaged yet.
- **Unsigned installers** (see Install).
- On ARM PCs, LibRaw (full-size RAW) runs as an x64 program under Windows' emulation. It works, a little slower.
- Cameras that only connect over MTP (no card over USB) are not supported yet; use a card reader.
- Full-size HEIF viewing is untested: we have no HEIF sample files yet.
- The AI features run on the graphics card where they can (DirectML), else the processor; their models are downloaded once when you turn them on.
- Noise and focus in Edit suggestions are judged from a small preview, so they are rough.
- Not yet included (planned): trimming and joining clips, export, the map view, a People screen, slideshows.

## Licences

Keepsake runs these programs as separate processes; they are not built into Keepsake. Their licences are in the install folder:
- **FFmpeg**: LGPL v3 (`licenses\FFmpeg-LICENSE.txt`).
- **LibRaw 0.22.2**: LGPL 2.1 or CDDL 1.0 (`libraw\`).
- **ExifTool** by Phil Harvey: the same terms as Perl.
