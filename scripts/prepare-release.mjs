import { createHash, createPublicKey, verify } from "node:crypto";
import { cpSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

// Verify the Tauri/Minisign envelope with the public key shipped in the app.
// Format: https://github.com/jedisct1/rust-minisign-verify
export function verifyUpdater(data, encodedSignature, encodedPublicKey) {
  const keyLines = Buffer.from(encodedPublicKey, "base64").toString("utf8").trim().split(/\r?\n/);
  const lines = Buffer.from(encodedSignature, "base64").toString("utf8").trim().split(/\r?\n/);
  const key = Buffer.from(keyLines[1] ?? "", "base64");
  const envelope = Buffer.from(lines[1] ?? "", "base64");
  const globalSignature = Buffer.from(lines[3] ?? "", "base64");
  if (key.length !== 42 || envelope.length !== 74 || globalSignature.length !== 64 ||
      !lines[2]?.startsWith("trusted comment: ") || !key.subarray(2, 10).equals(envelope.subarray(2, 10))) {
    throw new Error("Invalid updater signature envelope or signing key ID");
  }
  const publicKey = createPublicKey({ key: Buffer.concat([
    Buffer.from("302a300506032b6570032100", "hex"), key.subarray(10),
  ]), format: "der", type: "spki" });
  const algorithm = envelope.subarray(0, 2).toString("ascii");
  if (!["Ed", "ED"].includes(algorithm)) throw new Error("Unsupported updater signature algorithm");
  const message = algorithm === "ED" ? createHash("blake2b512").update(data).digest() : data;
  const signature = envelope.subarray(10);
  const comment = Buffer.from(lines[2].slice("trusted comment: ".length));
  if (!verify(null, message, publicKey, signature) ||
      !verify(null, Buffer.concat([signature, comment]), publicKey, globalSignature)) {
    throw new Error("Updater signature does not verify with the app public key");
  }
}

function collect(folder, files = new Map()) {
  for (const entry of readdirSync(folder, { withFileTypes: true })) {
    const path = join(folder, entry.name);
    if (entry.isSymbolicLink()) throw new Error("Release artifacts must not contain symbolic links");
    if (entry.isDirectory()) collect(path, files);
    else if (entry.isFile()) {
      if (files.has(entry.name)) throw new Error(`Duplicate release filename: ${entry.name}`);
      if (statSync(path).size === 0) throw new Error(`Empty release artifact: ${entry.name}`);
      files.set(entry.name, path);
    }
  }
  return files;
}

export function prepareRelease({ input, output, version, repository, publicKey }) {
  if (!/^\d+\.\d+\.\d+$/.test(version) || !/^[\w.-]+\/[\w.-]+$/.test(repository)) {
    throw new Error("Expected a stable semantic version and owner/repository");
  }
  const files = collect(input);
  const installer = `Ratatoskr_${version}_x64-setup.exe`;
  const expected = [installer, `${installer}.sig`, `Ratatoskr_${version}_x64_en-US.msi`,
    "Ratatoskr-portable.zip", "Ratatoskr-android.apk", "Ratatoskr-android-source.zip", "latest.json",
    "Ratatoskr-android-arm64-v8a.apk", "Ratatoskr-android-armeabi-v7a.apk", "Ratatoskr-android-x86_64.apk",
    "Ratatoskr-extension-chrome.zip", "Ratatoskr-extension-edge.zip", "Ratatoskr-extension-firefox.zip"];
  for (const name of expected) {
    if (!files.has(name)) throw new Error(`Missing release artifact: ${name}`);
  }
  if (files.size !== expected.length) throw new Error("Unexpected files in release artifacts");
  const latest = JSON.parse(readFileSync(files.get("latest.json"), "utf8"));
  const platform = latest.platforms?.["windows-x86_64"];
  const url = `https://github.com/${repository}/releases/download/v${version}/${encodeURIComponent(installer)}`;
  const signature = readFileSync(files.get(`${installer}.sig`), "utf8").trim();
  if (latest.version !== version || platform?.url !== url || platform?.signature !== signature ||
      !Number.isFinite(Date.parse(latest.pub_date))) throw new Error("Updater metadata does not match this release");
  verifyUpdater(readFileSync(files.get(installer)), signature, publicKey);
  // Refuse to reuse a staging folder: stale files must never enter a release.
  mkdirSync(output);
  const manifest = [];
  for (const name of [...files.keys()].sort()) {
    cpSync(files.get(name), join(output, name), { errorOnExist: true, force: false });
    const digest = createHash("sha256").update(readFileSync(join(output, name))).digest("hex");
    manifest.push(`${digest}  ${basename(name)}`);
  }
  writeFileSync(join(output, "SHA256SUMS.txt"), `${manifest.join("\n")}\n`);
  return manifest;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [input, output, tag, repository] = process.argv.slice(2);
  const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
  if (tag !== `v${config.version}`) throw new Error("Tag does not match the application version");
  const manifest = prepareRelease({ input, output, version: config.version, repository,
    publicKey: config.plugins.updater.pubkey });
  console.log(`Verified updater signature and staged ${manifest.length} assets with SHA-256 checksums.`);
}
