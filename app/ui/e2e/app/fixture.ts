// Drives the real Tauri app on Windows. WebView2 is Chromium: started with
// a remote-debugging port, Playwright attaches to it over CDP and sees each
// window (main, import) as a page.
//
// Needs `cargo build -p keepsake-app -p keepsake-cli` first (debug builds
// load the UI from the Vite dev server, which this fixture starts). Test
// libraries, media and settings live in a temp folder; your own library and
// settings are never touched (KEEPSAKE_CONFIG_DIR).

import { chromium, type Browser, type Page } from "@playwright/test";
import { execFileSync, spawn, spawnSync, type ChildProcess } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
export const repo = path.resolve(here, "../../../..");
export const exe = (name: string, profile = "debug") =>
  path.join(repo, "target", profile, process.platform === "win32" ? `${name}.exe` : name);
export const vendor = path.join(repo, "vendor");
const ffmpeg = path.join(vendor, process.platform === "win32" ? "ffmpeg.exe" : "ffmpeg");

export class World {
  root = fs.mkdtempSync(path.join(os.tmpdir(), "keepsake-ui-"));
  config = path.join(this.root, "config");
  env: NodeJS.ProcessEnv = {
    ...process.env,
    KEEPSAKE_CONFIG_DIR: this.config,
    KEEPSAKE_LOG_DIR: path.join(this.root, "logs"),
    KEEPSAKE_VENDOR: vendor,
    KEEPSAKE_UI_TEST: "1",
  };

  p(...parts: string[]) {
    return path.join(this.root, ...parts);
  }

  /** Run the CLI (debug build unless `release`). Exit code 2 ("finished
   * with problems", e.g. one destination is not safe to format) is fine. */
  cli(args: string[], release = false): string {
    const r = spawnSync(exe("keepsake", release ? "release" : "debug"), args, {
      env: this.env,
      encoding: "utf8",
      maxBuffer: 64 << 20,
    });
    if (r.status !== 0 && r.status !== 2) {
      throw new Error(`keepsake ${args.join(" ")} exited ${r.status}: ${r.stderr}`);
    }
    return r.stdout;
  }

  /** Point the rendition cache into the temp folder. */
  cacheHere() {
    const file = path.join(this.config, "config.toml");
    const text = fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "";
    const cache = this.p("cache").replace(/\\/g, "\\\\");
    const updated = text.includes("[render]")
      ? text.replace("[render]", `[render]\ncache_dir = "${cache}"`)
      : `${text}\n[render]\ncache_dir = "${cache}"\n`;
    fs.mkdirSync(this.config, { recursive: true });
    fs.writeFileSync(file, updated);
  }

  /** A folder of real media: `photos` JPEGs and one clip, all different. */
  media(dir: string, photos = 6): string {
    const d = this.p(dir);
    fs.mkdirSync(path.join(d, "DCIM", "100MSDCF"), { recursive: true });
    for (let i = 0; i < photos; i++) {
      const out = path.join(d, "DCIM", "100MSDCF", `DSC0${1000 + i}.JPG`);
      execFileSync(ffmpeg, ["-y", "-v", "error", "-f", "lavfi", "-i", `testsrc2=size=${1200 + i * 16}x800`, "-frames:v", "1", out]);
    }
    const clip = path.join(d, "PRIVATE", "M4ROOT", "CLIP", "C0001.MP4");
    fs.mkdirSync(path.dirname(clip), { recursive: true });
    execFileSync(ffmpeg, [
      "-y", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30", "-f", "lavfi", "-i",
      "sine=sample_rate=48000", "-t", "2", "-c:v", "libopenh264", "-c:a", "aac", "-shortest", clip,
    ]);
    return d;
  }

  cleanup() {
    try {
      fs.rmSync(this.root, { recursive: true, force: true });
    } catch {
      // A WebView2 process may still hold files; the OS temp cleaner gets them.
    }
  }
}

let vite: ChildProcess | null = null;

/** The Vite dev server the debug app loads its UI from (port 1420). */
export async function startVite(): Promise<void> {
  if (vite) return;
  vite = spawn("pnpm", ["exec", "vite", "--port", "1420", "--strictPort"], {
    cwd: path.join(repo, "app", "ui"),
    shell: true,
    stdio: "ignore",
  });
  for (let i = 0; i < 120; i++) {
    try {
      const r = await fetch("http://localhost:1420");
      if (r.ok) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error("vite did not start on port 1420");
}

export function stopVite() {
  if (vite?.pid) killTree(vite.pid);
  vite = null;
}

function killTree(pid: number) {
  try {
    if (process.platform === "win32") execFileSync("taskkill", ["/PID", String(pid), "/T", "/F"], { stdio: "ignore" });
    else process.kill(pid, "SIGKILL");
  } catch {
    // already gone
  }
}

export class App {
  proc!: ChildProcess;
  browser: Browser | null = null;
  port = 0;
  log = "";
  exited: string | null = null;

  constructor(
    private world: World,
    private extraEnv: NodeJS.ProcessEnv = {},
  ) {
    this.launch();
  }

  /** Start the app on a fresh debugging port and WebView2 profile. */
  private launch() {
    this.port = 9300 + Math.floor(Math.random() * 500);
    this.exited = null;
    this.log = this.world.p(`app-${this.port}.log`);
    const out = fs.openSync(this.log, "a");
    // KEEPSAKE_APP_PROFILE=release runs the release app (UI built in; no
    // Vite), for performance tests.
    this.proc = spawn(exe("keepsake-app", process.env.KEEPSAKE_APP_PROFILE ?? "debug"), [], {
      env: {
        ...this.world.env,
        RUST_LOG: process.env.RUST_LOG ?? "info",
        KEEPSAKE_UI_DEBUG_PORT: String(this.port),
        WEBVIEW2_USER_DATA_FOLDER: this.world.p(`webview2-${this.port}`),
        ...this.extraEnv,
      },
      stdio: ["ignore", out, out],
    });
    this.proc.on("exit", (code, signal) => {
      this.exited = `exit code ${code} signal ${signal}`;
    });
  }

  /** What we know when attaching fails: did the app exit, its log, and
   * whether the debugging port answers. */
  private async diagnose(): Promise<string> {
    let port = "no answer";
    try {
      const r = await fetch(`http://127.0.0.1:${this.port}/json/version`);
      port = `${r.status} ${await r.text()}`;
    } catch (e) {
      port = `no answer (${String(e)})`;
    }
    let procs = "";
    try {
      // Command lines show whether --remote-debugging-port and our data
      // folder reached WebView2.
      procs = execFileSync(
        "powershell",
        [
          "-NoProfile",
          "-Command",
          "Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe'\" | ForEach-Object { $_.ProcessId.ToString() + ' ' + $_.CommandLine.Substring(0, [Math]::Min(400, $_.CommandLine.Length)) }",
        ],
        { encoding: "utf8" },
      );
    } catch {
      procs = "(tasklist failed)";
    }
    const log = fs.existsSync(this.log) ? fs.readFileSync(this.log, "utf8").slice(-4000) : "(no log)";
    return [
      `app: ${this.exited ?? "still running"}`,
      `debug port ${this.port}: ${port}`,
      `webview processes:\n${procs.trim()}`,
      `app log:\n${log}`,
    ].join("\n");
  }

  /** Attach over CDP. WebView2 occasionally never opens its debugging
   * port; then the app is restarted once on a new port. */
  async connect(): Promise<void> {
    let report = "";
    for (let attempt = 1; attempt <= 2 && !this.browser; attempt++) {
      const deadline = Date.now() + 45_000;
      while (!this.browser && Date.now() < deadline && !this.exited) {
        try {
          this.browser = await chromium.connectOverCDP(`http://127.0.0.1:${this.port}`, {
            timeout: 10_000,
          });
        } catch {
          await new Promise((r) => setTimeout(r, 500));
        }
      }
      if (!this.browser) {
        report += `attempt ${attempt}:\n${await this.diagnose()}\n`;
        if (attempt === 1) {
          if (this.proc.pid) killTree(this.proc.pid);
          this.launch();
        }
      }
    }
    if (!this.browser) throw new Error(`could not attach to the app's WebView2\n${report}`);
  }

  /** The window whose URL matches (main: not the import window). */
  async page(kind: "main" | "import", timeoutMs = 60_000): Promise<Page> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      for (const c of this.browser?.contexts() ?? []) {
        for (const p of c.pages()) {
          const isImport = p.url().includes("#/import");
          if ((kind === "import") === isImport && p.url().startsWith("http")) return p;
        }
      }
      await new Promise((r) => setTimeout(r, 300));
    }
    throw new Error(`no ${kind} window`);
  }

  async close() {
    await this.browser?.close().catch(() => undefined);
    if (this.proc.pid) killTree(this.proc.pid);
  }
}
