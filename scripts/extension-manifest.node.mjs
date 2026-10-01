import assert from "node:assert/strict";
import test from "node:test";
import { extensionManifestFor } from "./extension-manifest.mjs";

const original = { key: "development-only", version: "1.2.0", permissions: ["nativeMessaging"], background: { service_worker: "background.js", scripts: ["background.js"] }, browser_specific_settings: { gecko: { id: "browser@ratatosk.app", strict_min_version: "121.0" } } };
test("Chromium store manifests contain only their supported background and no development key", () => {
  for (const browser of ["chrome", "edge"]) {
    const result = extensionManifestFor(original, browser);
    assert.deepEqual(result.background, { service_worker: "background.js" });
    assert.equal(result.key, undefined);
    assert.equal(result.browser_specific_settings, undefined);
  }
  assert.equal(original.key, "development-only");
});
test("Firefox store manifest declares native transfer data and optional connection diagnostics", () => {
  const result = extensionManifestFor(original, "firefox");
  assert.deepEqual(result.background, { scripts: ["background.js"] });
  assert.equal(result.key, undefined);
  assert.equal(result.browser_specific_settings.gecko.id, original.browser_specific_settings.gecko.id);
  assert.equal(result.browser_specific_settings.gecko.strict_min_version, "140.0");
  assert.deepEqual(result.browser_specific_settings.gecko.data_collection_permissions, { required: ["browsingActivity", "websiteContent"], optional: ["technicalAndInteraction"] });
});
test("Unknown store targets fail rather than shipping an ambiguous manifest", () => {
  assert.throws(() => extensionManifestFor(original, "unknown"));
});
