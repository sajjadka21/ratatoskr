// Packs the browser extension for the stores:
//   target/extension/ratatosk-extension-<version>.zip
// The development `key` is left out: stores refuse it and give their own ID
// (add that ID to STORE_EXTENSION_IDS in crates/dm-system/src/browser_hosts.rs).
// The same zip is uploaded to Edge Add-ons, Firefox Add-ons and, if used,
// the Chrome Web Store. Uses Windows' built-in `tar` to write the zip.
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join, resolve } from "node:path";

const source = "browser-extension";
const manifest = JSON.parse(readFileSync(join(source, "manifest.json"), "utf8"));
const staging = resolve("target", "extension", "staging");
const output = resolve("target", "extension", `ratatosk-extension-${manifest.version}.zip`);

rmSync(staging, { recursive: true, force: true });
mkdirSync(staging, { recursive: true });
cpSync(source, staging, { recursive: true, filter: (path) => !path.endsWith("README.md") });
delete manifest.key;
writeFileSync(join(staging, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);

rmSync(output, { force: true });
execFileSync("tar", ["-a", "-c", "-f", output, "-C", staging, "."], { stdio: "inherit" });
console.log(`Wrote ${output}`);
