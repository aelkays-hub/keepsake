# TEST_ME — Prompt 1: Test Corpus Infrastructure

Manual test steps for the `keepsake corpus` CLI and fixture machinery.

---

## Prerequisites

```powershell
# Windows
scripts\setup.ps1          # installs Rust, Node, pnpm, vendor/ffmpeg, vendor/exiftool
cargo build --workspace    # must compile cleanly
```

---

## 1. Fixture tests (always run, no corpus required)

```powershell
cargo test --workspace
```

**Expected**: all tests pass. Look for:
- `fixture_tests::role_*` — nine role-detection tests
- `fixture_tests::xavc_scan_*`, `avchd_scan_*`, `dcim_scan_*`, `mp_root_scan_*`
- `fixture_tests::xavc_grouper_*`, `avchd_grouper_*`, `dcim_grouper_*`, `mp_root_grouper_*`
- `fixture_tests::scanner_blake3_is_deterministic`
- `grouper::tests::xavc_base_stem_*`

No test may be ignored or skipped by default.

---

## 2. Real fixture generation (requires vendor/ffmpeg)

```powershell
scripts\make-fixtures.ps1
```

**Expected**:
- Script prints "Using FFmpeg: …" (not the stub warning).
- `tests\fixtures\xavc\PRIVATE\M4ROOT\CLIP\C0001.MP4` is a valid MP4 (~5–30 KB).
- `tests\fixtures\dcim\DCIM\100MSDCF\DSC00001.JPG` is a valid JPEG.
- `tests\fixtures\avchd\PRIVATE\AVCHD\BDMV\STREAM\00001.MTS` is a valid MPEG-TS file.

Run fixture tests again to confirm they still pass with real media:

```powershell
cargo test --workspace
```

---

## 3. CLI: `keepsake corpus scan`

### 3a. Missing KEEPSAKE_CORPUS and no path

```powershell
cargo run -p keepsake-cli -- corpus scan
```

**Expected**: error message "pass a path or set KEEPSAKE_CORPUS", exit 1.

### 3b. Scan the committed fixtures (simulated corpus)

Create a temporary corpus layout:

```powershell
$tmp = "$env:TEMP\keepsake-corpus-test"
New-Item -ItemType Directory "$tmp\cards\ILCE-7M4" -Force
Copy-Item -Recurse tests\fixtures\xavc\* "$tmp\cards\ILCE-7M4\"
Copy-Item -Recurse tests\fixtures\dcim\* "$tmp\cards\ILCE-7M4\"
```

```powershell
cargo run -p keepsake-cli -- corpus scan $tmp --output "$tmp\manifest.json"
```

**Expected**:
- Prints "Scanning …", "6+ file(s) across 1 camera card(s)", "Written …\manifest.json".
- `$tmp\manifest.json` is valid JSON with `cameras[0].name == "ILCE-7M4"`.
- Every entry has a non-empty `blake3` (64-char hex), a `kind`, and a `folder_role`.

### 3c. Scan using KEEPSAKE_CORPUS

```powershell
$env:KEEPSAKE_CORPUS = $tmp
cargo run -p keepsake-cli -- corpus scan
```

**Expected**: same output as 3b; writes `corpus\manifest.json` in the current directory.

---

## 4. CLI: `keepsake corpus init-expected`

```powershell
cargo run -p keepsake-cli -- corpus init-expected `
    --manifest "$tmp\manifest.json" `
    --output   "$tmp\expected.toml"
```

**Expected**:
- Prints "Written … (N asset(s))".
- File is valid TOML with `[[asset]]` entries.
- Each entry has `camera`, `files`, `capture_ts = ""`, `confirmed = false`.
- XAVC: C0001 and C0002 each appear as a group of 3 files (clip MP4 + XML + proxy MP4).
- DCIM: DSC00001 appears as a group of 3 files (JPG + ARW + HIF); DSC00002 as 1 file.

### 4b. Re-run without --force

```powershell
cargo run -p keepsake-cli -- corpus init-expected `
    --manifest "$tmp\manifest.json" `
    --output   "$tmp\expected.toml"
```

**Expected**: error "already exists — pass --force to overwrite", exit 1.

### 4c. Re-run with --force

```powershell
cargo run -p keepsake-cli -- corpus init-expected `
    --manifest "$tmp\manifest.json" `
    --output   "$tmp\expected.toml" `
    --force
```

**Expected**: succeeds; same output as 4a.

---

## 5. Corpus tests (requires your real Sony cards)

```powershell
# Set up your corpus layout:
#   $KEEPSAKE_CORPUS\cards\<camera-name>\PRIVATE\M4ROOT\... (XAVC)
#   $KEEPSAKE_CORPUS\cards\<camera-name>\DCIM\...           (stills)

$env:KEEPSAKE_CORPUS = "C:\path\to\your\corpus"
cargo run -p keepsake-cli -- corpus scan
cargo run -p keepsake-cli -- corpus init-expected --force

# Edit corpus\expected.toml:
#   - Fill in capture_ts, camera_model, duration_ms for a sample of assets
#   - Set confirmed = true for those you've verified against the camera

cargo test --workspace -- --include-ignored corpus
```

**Expected**: all three corpus tests pass; output shows "Verified N confirmed asset(s)".

---

## 6. --version and --help

```powershell
cargo run -p keepsake-cli -- --version
cargo run -p keepsake-cli -- --help
cargo run -p keepsake-cli -- corpus --help
cargo run -p keepsake-cli -- corpus scan --help
cargo run -p keepsake-cli -- corpus init-expected --help
```

**Expected**: each prints clean help/version text; no panics or stack traces.

---

## macOS

Steps are identical. Replace `scripts\make-fixtures.ps1` with `./scripts/make-fixtures.sh`.
PowerShell paths use `/` instead of `\`.
