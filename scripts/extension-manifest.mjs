export function extensionManifestFor(source, browser, { preserveDevelopmentKey = false } = {}) {
  if (!["chrome", "edge", "firefox"].includes(browser)) throw new Error("Unknown extension target");
  const manifest = structuredClone(source);
  if (!preserveDevelopmentKey) delete manifest.key;
  if (browser === "firefox") {
    manifest.permissions = manifest.permissions.filter(permission => permission !== "offscreen");
    manifest.background = { scripts: ["background.js"] };
    manifest.browser_specific_settings.gecko.strict_min_version = "140.0";
    manifest.browser_specific_settings.gecko.data_collection_permissions = {
      required: ["browsingActivity", "websiteContent"],
      optional: ["technicalAndInteraction"],
    };
  } else {
    manifest.background = { service_worker: "background.js" };
    delete manifest.browser_specific_settings;
  }
  return manifest;
}
