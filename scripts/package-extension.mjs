// Produce separate, browser-compatible store packages. No store approval is implied.
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join, resolve, sep } from "node:path";
import { extensionManifestFor } from "./extension-manifest.mjs";

const source = resolve("browser-extension");
const original = JSON.parse(readFileSync(join(source, "manifest.json"), "utf8"));
const outputFolder = resolve("target", "extension");
mkdirSync(outputFolder, { recursive: true });
for (const browser of ["chrome", "edge", "firefox"]) {
  const staging = mkdtempSync(join(outputFolder, ".staging-" + browser + "-"));
  const output = join(outputFolder, "ratatoskr-extension-" + browser + "-" + original.version + ".zip");
  if (!staging.startsWith(outputFolder + sep)) throw new Error("Staging escaped the output folder");
  try {
    cpSync(source, staging, { recursive: true, filter: path => !path.endsWith("README.md") });
    writeFileSync(join(staging, "manifest.json"), JSON.stringify(extensionManifestFor(original, browser), null, 2) + "\n");
    // Python's zipfile works on both Windows and Linux; do not create a tar named .zip.
    execFileSync(process.env.PYTHON || (process.platform === "win32" ? "python" : "python3"), ["-c",
      "from pathlib import Path; import sys,zipfile; root=Path(sys.argv[1]); output=Path(sys.argv[2]); z=zipfile.ZipFile(output,'w',zipfile.ZIP_DEFLATED); [(z.write(p,p.relative_to(root).as_posix())) for p in sorted(root.rglob('*')) if p.is_file()]; z.close()",
      staging, output], { stdio: "inherit" });
    console.log("Wrote " + output);
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}
