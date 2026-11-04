# How to cut a Keepsake release

These are the manual steps to publish a new version. They take about 5 minutes of human time; CI does the rest.

---

## Prerequisites

- You have push access to `github.com/aelkays-hub/keepsake`.
- You are on a machine with `git` and `gh` (GitHub CLI) installed.
- The `packaging` branch (or whatever contains the release build) is merged or squash-merged into `main`.

---

## Steps

### 1. Decide the version number

Keepsake uses [SemVer](https://semver.org/): `MAJOR.MINOR.PATCH`.

- Bug-fixes only → bump PATCH (e.g. `0.1.0` → `0.1.1`)
- New features, no breaking changes → bump MINOR (e.g. `0.1.0` → `0.2.0`)
- Breaking changes → bump MAJOR (e.g. `0.1.0` → `1.0.0`)

### 2. Update the version in `Cargo.toml`

Open `Cargo.toml` (workspace root). Find:

```toml
[workspace.package]
version = "0.1.0"
```

Change the version to the new number (e.g. `0.2.0`).

The Tauri app reads `version` from the same `Cargo.toml` via `version.workspace = true`, so you only need to change it in one place.

### 3. Commit the version bump

```bash
git add Cargo.toml
git commit -m "chore: bump version to 0.2.0"
```

### 4. Push to `main`

```bash
git push origin main
```

Wait for the CI (`ci.yml`) to go green on `main` before tagging. If CI fails, fix it first.

### 5. Create and push the tag

Tags must match `v<MAJOR>.<MINOR>.<PATCH>` exactly — the release workflow triggers on this pattern.

```bash
git tag v0.2.0
git push origin v0.2.0
```

### 6. Watch the release workflow

Open GitHub Actions and select the **Release** workflow run that was just triggered. It runs three jobs:

| Job | What it does | Expected time |
|---|---|---|
| Windows x64 | Downloads vendor binaries, builds MSI + NSIS, signs (if cert present) | ~15 min |
| Windows ARM64 | Same, cross-compiled for ARM64 | ~20 min |
| Smoke test | Installs x64 MSI silently, runs CLI checks | ~5 min |
| GitHub Release | Creates the release and uploads all installers | ~2 min |

If any job fails, check the logs, fix the issue, delete the tag, and start from step 5.

```bash
# Delete the tag if you need to retry
git tag -d v0.2.0
git push origin :v0.2.0
```

### 7. Review the GitHub Release draft

Once the **GitHub Release** job completes, open the release at:

```
https://github.com/aelkays-hub/keepsake/releases/tag/v0.2.0
```

The body is auto-generated from merged PRs and includes the signing status. Review it, add any manual release notes, and click **Publish release**.

---

## Adding code-signing certificates (one-time setup)

### Windows signing

1. Obtain a code-signing certificate as a `.pfx` file.
2. Base64-encode it: `[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))`
3. In the GitHub repo → **Settings → Secrets → Actions**, add:
   - `WINDOWS_CERTIFICATE` — the base64-encoded PFX
   - `WINDOWS_CERTIFICATE_PASSWORD` — the PFX password
4. The release workflow automatically signs if these secrets are present.

### macOS signing and notarization

macOS support is deferred until the LGPL macOS FFmpeg binary is available (see ADR-005). Once it is:

1. Add secrets: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`.
2. Uncomment the `build-macos-arm64` job in `.github/workflows/release.yml`.
3. Add `build-macos-arm64` to the `needs` list of the `release` job.

---

## Installer locations after a successful release

| File | Platform | Arch |
|---|---|---|
| `Keepsake_<version>_x64_en-US.msi` | Windows | x64 |
| `Keepsake_<version>_x64-setup.exe` | Windows | x64 |
| `Keepsake_<version>_arm64_en-US.msi` | Windows | ARM64 |
| `Keepsake_<version>_arm64-setup.exe` | Windows | ARM64 |
