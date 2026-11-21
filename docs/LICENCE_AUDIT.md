# Licence audit: could Keepsake be sold as a subscription?

Audited 2026-11-21 at `main` (`0e98d36`, v0.8.0). This is an engineering check, not legal advice. Have a lawyer confirm before you sell anything.

**How it was checked.**
- **Rust crates:** read from `Cargo.lock` with `cargo metadata`. Only the runtime and build dependencies of the workspace were counted, not dev-only ones.
- **npm packages:** `pnpm licenses list --prod`.
- **Bundled files:** the bundle resources in `tauri.windows.conf.json`, and the data files committed under `crates/keepsake-core/src/ai/`.
- **Online services:** every host the code calls (`git grep` for `https://`), with the licences recorded in `docs/DECISIONS.md`.
- **Service terms:** read online on 2026-11-21. Two sites refused automated reading (OpenSky, Flightradar24); those rows say "not read".

## The answer in short

The code and nearly all the bundled software can be sold. **The blockers are online services and data whose terms forbid or limit commercial use.** Five things must be settled before a paid version ships:

1. **Flickr:** a paid app counts as a commercial application, so it needs a **commercial API key** from Flickr (SmugMug), which may cost fees.
2. **OpenSky Network** (Add route, and build years from its aircraft database): the terms recorded in ADR-044 say **non-commercial only**. You need OpenSky's written agreement or a commercial licence, or the feature is switched off in the paid version.
3. **adsbdb route data** (the `/v0/callsign/` lookup): its README says the route data may not be copied or put into other databases without the author's explicit permission. Keepsake caches the answers, so ask for permission or drop the callsign lookup.
4. **Video patents** (H.264 and HEVC): this is not a copyright licence, so the LGPL doesn't settle it. A paid product that encodes H.264 with the OpenH264 built into FFmpeg, or decodes HEVC, can owe patent royalties. Ask the lawyer, and consider using the operating system's own encoders and decoders.
5. **Third-party notices:** the installer ships FFmpeg's, LibRaw's, ONNX Runtime's and DirectML's licence files, but **not** ExifTool's, nor the notices for 544 Rust crates and 9 npm packages. MIT, Apache-2.0 and BSD all require their licence text to be shipped. This is easy to fix (see the end).

Everything else is fine, or fine with an attribution line.

## 1. Code linked into Keepsake

| What | Count | Licences | Commercial use |
|---|---|---|---|
| Rust crates (runtime and build) | 544 | MIT, Apache-2.0, BSD, ISC, Zlib, Unlicense, CC0, Unicode-3.0 | ✅ Ship their licence texts |
| …of which MPL-2.0 | 5 (`cssparser`, `cssparser-macros`, `selectors`, `dtoa-short`, `option-ext`) | MPL-2.0 (copyleft per file) | ✅ Unmodified, so you only ship the notice and say where their source is |
| …of which `webpki-roots` | 1 | CDLA-Permissive-2.0 | ✅ |
| …of which `r-efi` | 2 | MIT OR Apache-2.0 OR LGPL-2.1+ | ✅ Use it under MIT |
| npm packages in the UI | 9 (React, TanStack Virtual, Tauri API and dialog plugin) | MIT, MIT/Apache-2.0 | ✅ |

No GPL code is linked (CLAUDE.md, non-negotiable c), so the subscription model doesn't conflict with Keepsake's own code. Keepsake's own `Cargo.toml` says `license = "UNLICENSED"`, which is right for closed-source software you sell.

## 2. Programs and libraries shipped beside the app

| What | Licence | Commercial use | To do |
|---|---|---|---|
| FFmpeg / ffprobe (BtbN LGPL build, separate process) | LGPL-3.0 (Windows) | ✅ | Ship the licence (done). Offer the exact source of the build you ship, for example a link to BtbN's source for that tag, or a copy on the `vendor-binaries` release. **Patents: see 6.** |
| ExifTool 13.59 (separate process, with its bundled Perl) | Artistic or GPL (Perl's terms) | ✅ Running it as a separate program is fine | **Its licence isn't in the installer.** Add it. |
| LibRaw `dcraw_emu` and `libraw.dll` | LGPL-2.1 or CDDL-1.0 | ✅ | Licence files are shipped (done) |
| ONNX Runtime | MIT | ✅ | Licence and ThirdPartyNotices are shipped (done) |
| DirectML | Microsoft DirectML licence (redistributable with an app) | ✅ | Licence is shipped (done) |
| WebView2 (the system's) | Microsoft | ✅ | Nothing (part of Windows) |

## 3. AI models (downloaded by the user from Hugging Face or GitHub)

| Model | Used for | Licence | Commercial use |
|---|---|---|---|
| SigLIP 2 base p16-224 (image encoder) | Tags, duplicates, aircraft type | Apache-2.0 | ✅ |
| `labels-siglip2-b16-224.bin`, `aircraft-types-siglip2-b16-224.bin` (committed) | Text vectors made from SigLIP 2 | Made from Apache-2.0 weights | ✅ |
| YuNet 2023mar | Finding faces | MIT | ✅ |
| SFace 2021dec | Recognising faces | Apache-2.0 (OpenCV Zoo) | ✅ by its licence. ⚠️ Face models are often trained on research-only face datasets. Ask the lawyer whether the Apache-2.0 on the weights is enough. |
| PP-OCRv5 detector and English recogniser, and their dictionary | Reading registrations | Apache-2.0 | ✅ |
| YOLOX-s | Finding the aircraft | Apache-2.0 | ✅ |

The models InsightFace, OpenAI CLIP and Apple MobileCLIP were deliberately not used, because their licences are non-commercial or unconfirmed (ADR-035, ADR-040). That choice now pays off.

## 4. Data built into the app

| File | Source | Licence | Commercial use |
|---|---|---|---|
| `airports.tsv` | OurAirports | The Unlicense (public domain) | ✅ |
| `airlines.tsv`, `airline-codes.tsv`, `icao-types.tsv`, `aircraft-types.tsv`, `labels.tsv` | Hand-made lists of facts (names, ICAO designators) | Keepsake's own | ✅ Facts aren't copyrightable. Low risk. |

## 5. Online services Keepsake calls

| Service | Feature | Who holds the key | Commercial use | To do |
|---|---|---|---|---|
| **Flickr API** | Upload, Report → Flickr | The user's own App Garden key | ❌ **Needs a commercial key.** Flickr's terms define a paid product as a commercial application, whoever holds the key. | Apply at flickr.com/services/api/keys/apply/ (fees may apply), or keep Flickr out of the paid version. Other conditions: the line "This product uses the Flickr API but is not endorsed or certified by SmugMug, Inc.", no Flickr logo, a privacy policy, a 3,600 calls an hour limit per key, caching up to 24 h, and deletion requests honoured within 24 h. |
| **OpenSky REST API** | Add route | The user's free account | ❌ Non-commercial (ADR-044; the terms page wasn't read today) | Write to OpenSky for a commercial licence, or switch Add route off in the paid version. |
| **OpenSky aircraft database** | Build years | Downloaded by the user | ❌ Non-commercial, credit OpenSky (ADR-044) | The same as above; build years can come from the TC and FAA registers instead. |
| **adsbdb aircraft lookup** (`/v0/aircraft/`) | Registration → type, owner | No key | ⚠️ Unclear: the code is MIT, the data comes from PlaneBase, and no data terms are published | Ask the adsbdb author, or check PlaneBase's terms. |
| **adsbdb callsign lookup** (`/v0/callsign/`) | Route from the callsign | No key | ❌ The route data "may not be copied, published, or incorporated into other databases" without permission | Ask David J Taylor for permission, or drop the callsign lookup. |
| **Flightradar24 API** | Flights, aircraft details | The user's own subscription | ⚠️ Not read (the terms page refused automated reading) | Read the API terms yourself, in particular whether a subscriber's key may be used in software sold by someone else. The 30-day storage rule is already followed. |
| **Transport Canada register** | Build year, type | Download | ✅ Open Government Licence – Canada allows commercial use | Add the attribution "Contains information licensed under the Open Government Licence – Canada." Owner names of individuals aren't covered by the licence, and Keepsake already doesn't keep them. |
| **FAA register** | Build year, type | Download | ✅ US government work, public domain | None |
| **Claude, ChatGPT, Gemini** | Tag with AI, edit suggestions | The user's own key | ✅ The user is the provider's customer under the provider's own terms | Say in your terms that the user's own provider account and costs apply. |
| **Hugging Face, GitHub, NuGet** | Model and tool downloads | No key | ✅ Each file's own licence applies (above) | None |

There's no map yet. When the OpenStreetMap map view is built, note that the public `tile.openstreetmap.org` servers forbid heavy or commercial use. A paid app needs a commercial tile provider (MapTiler, Stadia, Thunderforest and others) and the "© OpenStreetMap contributors" credit (ODbL).

## 6. Patents (separate from licences)

- **H.264:** OpenH264 inside the BtbN FFmpeg build is compiled from source. Cisco's royalty cover applies only to Cisco's own binary, so it doesn't apply here. Proxy encoding with the hardware encoders (NVENC, QSV, AMF) or Media Foundation uses the drivers or Windows, which carry their own licences.
- **HEVC** (Sony XAVC HS, HEIF): FFmpeg's own HEVC decoder carries no patent licence. On Windows, the HEVC Video Extensions from Microsoft's Store do, and so does VideoToolbox on macOS.
- **AAC:** the same question applies, if FFmpeg encodes AAC audio for MP4 conversion.

**To do:** ask the lawyer. The usual safe design is to decode and encode through the operating system (Media Foundation, VideoToolbox) in the paid version, and keep FFmpeg for remuxing, trimming and probing, which need no codec.

## What to do, in order

1. **Ask before building:** Flickr (commercial key), OpenSky (commercial licence), and adsbdb (aircraft data and route data). Their answers decide which features can be in the paid version.
2. **Lawyer:** video patents (6), the SFace training data (3), the Flightradar24 terms, and the EULA and privacy policy.
3. **Easy fixes in the app, whatever is decided:**
   - Generate a `THIRD-PARTY-NOTICES.txt` for the Rust crates and npm packages at build time, and ship it in `licenses/`. This needs a tool, for example `cargo-about` (MIT OR Apache-2.0), which is a new dependency, so it needs your approval.
   - Ship ExifTool's licence.
   - Add an **About → Licences** page that shows the notices and the attribution lines (Flickr, OpenSky, OGL-Canada, OpenStreetMap later).
   - Add a "source of FFmpeg" link for the LGPL.
4. **Design the paid tiers** so that features with unresolved terms (Flickr, OpenSky, the adsbdb routes) can be switched off by licence tier, or so that the user supplies their own key under terms that allow it.
