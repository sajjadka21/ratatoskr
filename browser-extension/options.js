const fields = ["takeover", "minimumSize", "excludedHosts", "excludedExtensions"];
const MB = 1024 * 1024;

// Reading cookies needs its own permission, which the extension does not hold
// until the user asks for session handover, and gives back when they stop.
const SESSION_PERMISSIONS = { permissions: ["cookies"], origins: ["<all_urls>"] };
// Alt+click needs a small script on every page, so it asks for access to all
// sites only when turned on.
const ALT_PERMISSIONS = { permissions: ["scripting"], origins: ["<all_urls>"] };
const ALT_SCRIPT = { id: "ratatosk-alt", matches: ["<all_urls>"], js: ["alt.js"], runAt: "document_start", allFrames: true };

// Access to all sites is shared by both features: it goes back only when
// neither needs it.
async function releaseSites(keepFor) {
  const { sessionHandover, altSkip } = await chrome.storage.local.get({ sessionHandover: false, altSkip: false });
  const stillNeeded = (keepFor !== "session" && sessionHandover) || (keepFor !== "alt" && altSkip);
  if (!stillNeeded) await chrome.permissions.remove({ origins: ["<all_urls>"] }).catch(() => false);
}

function saved() {
  document.getElementById("saved").textContent = chrome.i18n.getMessage("saved");
}

chrome.storage.local.get(
  { takeoverEnabled: false, sessionHandover: false, altSkip: false, minimumBytes: 0, excludedHosts: "", excludedExtensions: "" },
  async (values) => {
    document.getElementById("takeover").checked = values.takeoverEnabled;
    document.getElementById("minimumSize").value = Math.round(Number(values.minimumBytes || 0) / MB);
    document.getElementById("excludedHosts").value = values.excludedHosts;
    document.getElementById("excludedExtensions").value = values.excludedExtensions;

    // The setting only counts while the browser still grants the permission.
    const granted = await chrome.permissions.contains(SESSION_PERMISSIONS);
    document.getElementById("sessionHandover").checked = values.sessionHandover && granted;
    document.getElementById("altSkip").checked =
      values.altSkip && (await chrome.permissions.contains(ALT_PERMISSIONS));
  }
);

fields.forEach((id) => document.getElementById(id).addEventListener("change", () => {
  chrome.storage.local.set({
    takeoverEnabled: document.getElementById("takeover").checked,
    minimumBytes: Math.max(0, Number(document.getElementById("minimumSize").value || 0)) * MB,
    excludedHosts: document.getElementById("excludedHosts").value,
    excludedExtensions: document.getElementById("excludedExtensions").value
  }, saved);
}));

document.getElementById("sessionHandover").addEventListener("change", async (event) => {
  const box = event.target;

  if (box.checked) {
    // Must run in the click handler: browsers only show the permission
    // prompt in response to a user gesture.
    const granted = await chrome.permissions.request(SESSION_PERMISSIONS).catch(() => false);
    box.checked = granted;
    await chrome.storage.local.set({ sessionHandover: granted });
  } else {
    await chrome.storage.local.set({ sessionHandover: false });
    await chrome.permissions.remove({ permissions: ["cookies"] }).catch(() => false);
    await releaseSites("session");
  }

  saved();
});

document.getElementById("altSkip").addEventListener("change", async (event) => {
  const box = event.target;

  if (box.checked) {
    const granted = await chrome.permissions.request(ALT_PERMISSIONS).catch(() => false);
    if (granted) {
      await chrome.scripting.unregisterContentScripts({ ids: [ALT_SCRIPT.id] }).catch(() => {});
      await chrome.scripting.registerContentScripts([ALT_SCRIPT]).catch(() => {});
    }
    box.checked = granted;
    await chrome.storage.local.set({ altSkip: granted });
  } else {
    await chrome.storage.local.set({ altSkip: false });
    if (chrome.scripting) {
      await chrome.scripting.unregisterContentScripts({ ids: [ALT_SCRIPT.id] }).catch(() => {});
    }
    await chrome.permissions.remove({ permissions: ["scripting"] }).catch(() => false);
    await releaseSites("alt");
  }

  saved();
});
