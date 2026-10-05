import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const json = (path) => JSON.parse(readFileSync(path, "utf8"));
const version = json("src-tauri/tauri.conf.json").version;
assert.match(version, /^\d+\.\d+\.\d+$/);
assert.equal(json("package.json").version, version);
const lock = json("package-lock.json");
assert.equal(lock.version, version);
assert.equal(lock.packages[""].version, version);
for (const path of ["src-tauri/Cargo.toml", "crates/dm-cli/Cargo.toml"]) {
  assert.equal(readFileSync(path, "utf8").match(/^version = "([^"]+)"/m)?.[1], version, path);
}
const android = readFileSync("android/app/build.gradle.kts", "utf8");
const windowsOnly = readFileSync(`docs/releases/${version}.md`, "utf8")
  .includes("<!-- release-platforms: windows -->");
if (!windowsOnly) assert.equal(android.match(/versionName = "([^"]+)"/)?.[1], version);
if (process.argv[2]) assert.equal(process.argv[2], `v${version}`, "Release tag mismatch");
console.log(`Release version is consistent: ${version}${windowsOnly ? " (Windows only)" : ""}`);
