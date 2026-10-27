@echo off
rem Keepsake on Windows: set up everything, then run, build or test the app.
rem
rem   keepsake.bat           set up, then start the app (pnpm tauri dev)
rem   keepsake.bat build     set up, then build the installers
rem   keepsake.bat test      set up, then run the Rust and UI tests
rem   keepsake.bat release   tag v0.1.0 on mvp-hardening and push it (asks first)
rem
rem Safe to run again: each step skips what is already installed.
rem Run it from this folder, or copy it anywhere: outside the repository it
rem clones Keepsake into %USERPROFILE%\keepsake first.

setlocal EnableExtensions
set "MODE=%~1"
if "%MODE%"=="" set "MODE=dev"
if /i not "%MODE%"=="dev" if /i not "%MODE%"=="build" if /i not "%MODE%"=="test" if /i not "%MODE%"=="release" (
  echo Unknown option "%MODE%". Use: dev, build, test or release.
  exit /b 1
)

where winget >nul 2>nul
if errorlevel 1 (
  echo [keepsake] winget is missing. Install "App Installer" from the Microsoft Store, then run this again.
  exit /b 1
)

rem -- 1. Git ------------------------------------------------------------------
where git >nul 2>nul
if not errorlevel 1 goto :have_git
echo [keepsake] Installing Git...
winget install --id Git.Git -e --accept-package-agreements --accept-source-agreements
set "PATH=%ProgramFiles%\Git\cmd;%PATH%"
:have_git

rem -- 2. Find or clone the repository ----------------------------------------
set "ROOT="
if exist "%~dp0..\Cargo.toml" (
  for %%R in ("%~dp0..") do set "ROOT=%%~fR"
)
if defined ROOT goto :have_repo
set "ROOT=%USERPROFILE%\keepsake"
if exist "%ROOT%\Cargo.toml" goto :have_repo
echo [keepsake] Cloning Keepsake into %ROOT%...
git clone https://github.com/aelkays-hub/keepsake.git "%ROOT%"
if errorlevel 1 goto :fail
:have_repo
cd /d "%ROOT%"
echo [keepsake] Repository: %ROOT%

if /i "%MODE%"=="release" goto :release

rem -- 3. C++ Build Tools (Rust needs them to compile on Windows) -------------
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "VCCOMP=Microsoft.VisualStudio.Component.VC.Tools.x86.x64"
set "VCEXTRA="
if /i "%PROCESSOR_ARCHITECTURE%"=="ARM64" (
  set "VCCOMP=Microsoft.VisualStudio.Component.VC.Tools.ARM64"
  set "VCEXTRA=--add Microsoft.VisualStudio.Component.VC.Tools.ARM64"
)
set "HAVE_VC="
if exist "%VSWHERE%" (
  for /f "usebackq delims=" %%I in (`"%VSWHERE%" -products * -requires %VCCOMP% -property installationPath`) do set "HAVE_VC=1"
)
if defined HAVE_VC goto :have_vc
echo [keepsake] Installing Visual Studio Build Tools with C++ (this takes a while)...
winget install --id Microsoft.VisualStudio.BuildTools -e --accept-package-agreements --accept-source-agreements --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended %VCEXTRA%"
if errorlevel 1 goto :fail
:have_vc

rem -- 4. Rust, Node.js, pnpm, FFmpeg, ExifTool, LibRaw -----------------------
echo [keepsake] Running scripts\setup.ps1...
powershell -NoProfile -ExecutionPolicy Bypass -File "%ROOT%\scripts\setup.ps1"
if errorlevel 1 goto :fail

rem Pick up tools the setup just installed, without reopening the window.
for /f "usebackq delims=" %%P in (`powershell -NoProfile -Command "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')"`) do set "NEWPATH=%%P"
set "PATH=%USERPROFILE%\.cargo\bin;%APPDATA%\npm;%NEWPATH%"

rem -- 5. UI packages ----------------------------------------------------------
cd /d "%ROOT%\app\ui"
echo [keepsake] Installing UI packages...
call pnpm install
if errorlevel 1 goto :fail

if /i "%MODE%"=="build" goto :build
if /i "%MODE%"=="test" goto :test

rem -- 6a. Start the app -------------------------------------------------------
echo [keepsake] Starting Keepsake (the first build takes several minutes)...
call pnpm tauri dev
goto :end

:build
echo [keepsake] Building the installers...
call pnpm tauri build
if errorlevel 1 goto :fail
echo [keepsake] Installers are in %ROOT%\target\release\bundle
goto :end

:test
cd /d "%ROOT%"
echo [keepsake] Rust tests...
cargo test --workspace
if errorlevel 1 goto :fail
cargo run -p keepsake-cli -- --version
if errorlevel 1 goto :fail
cd /d "%ROOT%\app\ui"
echo [keepsake] UI tests...
call pnpm test
if errorlevel 1 goto :fail
echo [keepsake] All tests passed.
goto :end

rem -- Release: tag v0.1.0 on the pushed mvp-hardening and push the tag -------
:release
git fetch origin
if errorlevel 1 goto :fail
git ls-remote --exit-code --tags origin refs/tags/v0.1.0 >nul 2>nul
if not errorlevel 1 (
  echo [keepsake] v0.1.0 is already on GitHub. Nothing to do.
  goto :end
)
set "TAGAT="
set "BRANCHAT="
for /f %%C in ('git rev-parse -q --verify "refs/tags/v0.1.0^{commit}" 2^>nul') do set "TAGAT=%%C"
for /f %%C in ('git rev-parse origin/mvp-hardening') do set "BRANCHAT=%%C"
if not defined TAGAT goto :make_tag
if "%TAGAT%"=="%BRANCHAT%" goto :confirm_tag
echo [keepsake] A local tag v0.1.0 exists but points to an older commit
echo            ^(%TAGAT:~0,7%, not %BRANCHAT:~0,7%^). It was never pushed.
echo            Remove it with:  git tag -d v0.1.0   then run this again.
exit /b 1
:make_tag
where gh >nul 2>nul
if not errorlevel 1 (
  echo [keepsake] Latest CI run on mvp-hardening:
  gh run list --branch mvp-hardening --limit 1
)
echo.
echo This pushes tag v0.1.0 on origin/mvp-hardening to GitHub, which starts
echo the Release workflow (a draft release). Only do this when CI is green.
set "OK="
set /p "OK=Type yes to continue: "
if /i not "%OK%"=="yes" (
  echo [keepsake] Cancelled.
  goto :end
)
git tag v0.1.0 origin/mvp-hardening
if errorlevel 1 goto :fail
goto :push_tag
:confirm_tag
echo [keepsake] The local tag v0.1.0 already points to origin/mvp-hardening.
set "OK="
set /p "OK=Type yes to push it: "
if /i not "%OK%"=="yes" (
  echo [keepsake] Cancelled.
  goto :end
)
:push_tag
git push origin v0.1.0
if errorlevel 1 goto :fail
echo [keepsake] Tag pushed. Watch the Release workflow on GitHub (Actions tab).
goto :end

:fail
echo.
echo [keepsake] A step failed; see the messages above.
exit /b 1

:end
endlocal
