import { describe, expect, it } from "vitest";

import cli from "../../node_modules/@tauri-apps/cli/package.json";
import template from "../../src-tauri/nsis/installer.nsi?raw";
import windowsConf from "../../src-tauri/tauri.windows.conf.json";

// The Windows installer uses a copy of Tauri's NSIS template with one fix
// (ADR-055). These checks stop the copy from silently going stale or losing it.
describe("Windows installer template", () => {
  it("is the one the Windows build uses", () => {
    expect(windowsConf.bundle.windows.nsis.template).toBe("nsis/installer.nsi");
  });

  it("was copied from the Tauri CLI version installed", () => {
    // When this fails, @tauri-apps/cli changed: copy the new upstream
    // installer.nsi, re-apply the KEEPSAKE changes and update the header.
    expect(template).toContain(`(tauri-cli v${cli.version},`);
  });

  it("upgrades over the old version without running its uninstaller", () => {
    expect(template).toMatch(/\$\{ElseIf\} \$R0 = 1\n(?:\s*;[^\n]*\n)*\s*Abort\n/);
  });

  it("finds the old install folder even when the publisher changed", () => {
    const calls = template.match(/Call ReadPreviousInstallDir/g) ?? [];
    expect(calls).toHaveLength(2);
    // The publisher key is read in one place only: the helper, with a fallback.
    const direct = template.match(/ReadRegStr \$4 SHCTX "\$\{MANUPRODUCTKEY\}" ""/g) ?? [];
    expect(direct).toHaveLength(1);
    expect(template).toMatch(
      /Function ReadPreviousInstallDir[\s\S]*"\$\{UNINSTKEY\}" "InstallLocation"[\s\S]*FunctionEnd/,
    );
    // Never run the old uninstaller with an unchecked folder.
    expect(template).toContain('StrCpy $R1 "$R1 _?=$4"');
  });
});
