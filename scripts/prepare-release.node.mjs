import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";
import { test } from "node:test";
import { prepareRelease, verifyUpdater } from "./prepare-release.mjs";

function signed(data, algorithm = "ED") {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const id = Buffer.from("12345678");
  const key = Buffer.concat([Buffer.from("Ed"), id, publicKey.export({ type: "spki", format: "der" }).subarray(-32)]);
  const message = algorithm === "ED" ? createHash("blake2b512").update(data).digest() : data;
  const signature = sign(null, message, privateKey);
  const comment = "timestamp:1\tfile:installer\tprehashed";
  const global = sign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey);
  return {
    publicKey: Buffer.from(`untrusted comment: public key\n${key.toString("base64")}\n`).toString("base64"),
    signature: Buffer.from(`untrusted comment: signature\n${Buffer.concat([Buffer.from(algorithm), id, signature]).toString("base64")}\ntrusted comment: ${comment}\n${global.toString("base64")}\n`).toString("base64"),
  };
}

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "ratatoskr-release-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const input = join(root, "input");
  const windows = join(input, "release", "bundle", "nsis");
  mkdirSync(windows, { recursive: true });
  mkdirSync(join(input, "portable"));
  mkdirSync(join(input, "release", "bundle", "msi"));
  const data = Buffer.from("an installer");
  const keys = signed(data);
  const name = "Ratatoskr_1.0.1_x64-setup.exe";
  writeFileSync(join(windows, name), data);
  writeFileSync(join(windows, `${name}.sig`), keys.signature);
  writeFileSync(join(windows, "latest.json"), JSON.stringify({ version: "1.0.1", pub_date: "2026-10-01T00:00:00Z",
    platforms: { "windows-x86_64": { signature: keys.signature,
      url: `https://github.com/owner/repo/releases/download/v1.0.1/${name}` } } }));
  writeFileSync(join(input, "release", "bundle", "msi", "Ratatoskr_1.0.1_x64_en-US.msi"), "MSI");
  writeFileSync(join(input, "portable", "Ratatoskr-portable.zip"), "ZIP");
  writeFileSync(join(input, "Ratatoskr-android.apk"), "APK");
  return { input, output: join(root, "staged"), version: "1.0.1", repository: "owner/repo", publicKey: keys.publicKey };
}

test("nested Windows and Android artifacts flatten and checksum every file once", (t) => {
  const options = fixture(t);
  const manifest = prepareRelease(options);
  assert.equal(manifest.length, 6);
  assert.ok(manifest.every((line) => !line.endsWith("SHA256SUMS.txt")));
  for (const line of manifest) {
    const [digest, name] = line.split("  ");
    assert.equal(createHash("sha256").update(readFileSync(join(options.output, name))).digest("hex"), digest);
  }
});
test("colliding filenames cannot overwrite a release asset", (t) => {
  const options = fixture(t);
  writeFileSync(join(options.input, "portable", "Ratatoskr-android.apk"), "different APK");
  assert.throws(() => prepareRelease(options), /Duplicate/);
});
test("incomplete releases are rejected", (t) => {
  const options = fixture(t);
  rmSync(join(options.input, "Ratatoskr-android.apk"));
  assert.throws(() => prepareRelease(options), /Missing/);
});
test("tampered installers cannot be published with an old signature", (t) => {
  const options = fixture(t);
  writeFileSync(join(options.input, "release/bundle/nsis/Ratatoskr_1.0.1_x64-setup.exe"), "modified installer");
  assert.throws(() => prepareRelease(options), /does not verify/);
});
test("updater URLs must refer to the actual release", (t) => {
  const options = fixture(t);
  const path = join(options.input, "release/bundle/nsis/latest.json");
  const latest = JSON.parse(readFileSync(path));
  latest.platforms["windows-x86_64"].url = "https://example.com/installer.exe";
  writeFileSync(path, JSON.stringify(latest));
  assert.throws(() => prepareRelease(options), /metadata/);
});
test("Minisign legacy and prehashed signatures verify; a different key fails", () => {
  const data = Buffer.from("known installer");
  for (const algorithm of ["Ed", "ED"]) {
    const keys = signed(data, algorithm);
    verifyUpdater(data, keys.signature, keys.publicKey);
    assert.throws(() => verifyUpdater(data, keys.signature, signed(data).publicKey), /key ID|does not verify/);
  }
});
