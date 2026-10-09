import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { extensionManifestFor } from "../../scripts/extension-manifest.mjs";

const source = JSON.parse(await readFile(new URL("../manifest.json", import.meta.url), "utf8"));

test("the bundled Chromium extension declares only its Manifest V3 service worker", () => {
  assert.equal(source.manifest_version, 3);
  assert.equal(source.background.service_worker, "background.js");
  assert.equal("scripts" in source.background, false);
});

test("store packages keep the background declaration valid for each browser", () => {
  for (const browser of ["chrome", "edge"]) {
    const manifest = extensionManifestFor(source, browser);
    assert.deepEqual(manifest.background, { service_worker: "background.js" });
    assert.equal(manifest.key, undefined);
  }

  const firefox = extensionManifestFor(source, "firefox");
  assert.deepEqual(firefox.background, { scripts: ["background.js"] });
  assert.equal(firefox.permissions.includes("offscreen"), false);
});

test("local test packages retain the development ID required by the installed native host", () => {
  for (const browser of ["chrome", "edge"]) {
    const manifest = extensionManifestFor(source, browser, { preserveDevelopmentKey: true });
    assert.equal(manifest.key, source.key);
  }

  const storeManifest = extensionManifestFor(source, "chrome");
  assert.equal(storeManifest.key, undefined);
});
