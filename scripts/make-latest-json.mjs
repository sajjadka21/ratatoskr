// Writes latest.json next to the signed NSIS installer: the file installed
// copies of Ratatosk read to learn about a new version.
//
//   node scripts/make-latest-json.mjs https://github.com/OWNER/REPO/releases/download/vX.Y.Z
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const base = process.argv[2]?.replace(/\/+$/, "");
if (!base || !/^https:\/\//.test(base)) {
  console.error("Give the https address the installer will be downloaded from.");
  process.exit(1);
}

const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const folder = join("target", "release", "bundle", "nsis");
const installer = readdirSync(folder).find(
  (name) => name.includes(`_${config.version}_`) && name.endsWith("-setup.exe"),
);
if (!installer) {
  console.error(`No installer for version ${config.version} in ${folder}.`);
  process.exit(1);
}
const signature = readFileSync(join(folder, `${installer}.sig`), "utf8").trim();

const latest = {
  version: config.version,
  notes: process.env.RELEASE_NOTES ?? "",
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature,
      url: `${base}/${encodeURIComponent(installer)}`,
    },
  },
};
writeFileSync(join(folder, "latest.json"), `${JSON.stringify(latest, null, 2)}\n`);
console.log(`Wrote ${join(folder, "latest.json")} for ${installer}.`);
