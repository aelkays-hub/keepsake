# TEST_ME — Vendor binary setup (Prompt 0b)

## What was changed

`scripts/setup.ps1` and `scripts/setup.sh` were rewritten to:
- Download FFmpeg and ExifTool from a stable GitHub Release mirror instead of
  upstream URLs that have rotted or serve GPL builds.
- Verify SHA-256 for every download.
- Fail with a clear message (file name + URL) on download or checksum failure.
- Confirm after install that FFmpeg is an LGPL build (no `--enable-gpl`, no
  `--enable-version3`).

---

## Windows manual test (Windows 11 x64 or ARM64)

### Prerequisites
- PowerShell 5.1 or later (built into Windows 11)
- Internet access

### Steps

1. **Delete `vendor\` if it exists** so setup runs from scratch:
   ```powershell
   Remove-Item -Recurse -Force .\vendor
   ```

2. **Run the setup script**:
   ```powershell
   .\scripts\setup.ps1
   ```

3. **Expected output** — each line should appear and the script must not exit
   with an error:
   ```
   [setup] Architecture: AMD64          ← or ARM64 on an ARM device
   [setup] Downloading ffmpeg-n9.0-latest-win64-lgpl-9.0.zip ...
   [setup] Verifying SHA-256 for ffmpeg-n9.0-latest-win64-lgpl-9.0.zip ...
   [setup]   OK SHA-256 verified
   [setup] Extracting FFmpeg...
   [setup]   Installed ffmpeg.exe and ffprobe.exe
   [setup] Verifying FFmpeg is an LGPL build...
   [setup]   OK LGPL verified (no --enable-gpl, no --enable-version3)
   [setup]   ffmpeg version: 9.0
   [setup] Downloading exiftool-13.59_64.zip ...
   [setup] Verifying SHA-256 for exiftool-13.59_64.zip ...
   [setup]   OK SHA-256 verified
   [setup] Extracting ExifTool...
   [setup]   Installed exiftool.exe
   [setup]   OK ExifTool 13.59
   [setup] Dev environment ready. Next steps:
   ```

4. **Verify binaries exist**:
   ```powershell
   Test-Path vendor\ffmpeg.exe    # must be True
   Test-Path vendor\ffprobe.exe   # must be True
   Test-Path vendor\exiftool.exe  # must be True
   ```

5. **Verify FFmpeg reports LGPL** manually:
   ```powershell
   .\vendor\ffmpeg.exe -version
   ```
   - The `configuration:` line must NOT contain `--enable-gpl`.
   - The `configuration:` line must NOT contain `--enable-version3`.
   - It SHOULD contain `--disable-gpl`.

6. **Verify ExifTool works**:
   ```powershell
   .\vendor\exiftool.exe -ver
   ```
   Expected: `13.59`

7. **Idempotency** — run the script a second time.  Expected: all "already
   present, skipping download" messages; no re-download occurs.

8. **Checksum failure simulation** — corrupt a file and re-run:
   ```powershell
   "corrupt" | Out-File vendor\ffmpeg.exe
   .\scripts\setup.ps1
   ```
   Expected: the script downloads ffmpeg.exe anew (the old file is re-
   downloaded because the binary is missing from vendor/).  To test the actual
   checksum failure path, temporarily edit setup.ps1 and change one hex digit
   of `$FfmpegZipSHA`.  Expected error message must name both the file and the
   mirror URL.

### ARM64-specific
On a Windows ARM64 device, step 3 should show `Architecture: ARM64` and the
download filename should be `ffmpeg-n9.0-latest-winarm64-lgpl-9.0.zip`.

---

## macOS (deferred)

macOS FFmpeg support is not yet implemented.  Running `./scripts/setup.sh` on
macOS will:
- Install Rust, Node, pnpm normally.
- Print a warning that FFmpeg is not configured for macOS.
- Install ExifTool from the vendor-binaries mirror.

macOS FFmpeg will be addressed in a future prompt that adds a CI workflow
compiling an LGPL FFmpeg for Apple Silicon.

---

## CI

No CI changes were made in this prompt.  The existing `windows-latest` CI job
does not run `setup.ps1`; it downloads no vendor binaries.  CI verifies that
the Rust and TypeScript code compiles and passes lint/tests.
