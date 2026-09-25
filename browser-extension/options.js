const fields = ["takeover", "minimumSize", "excludedHosts", "excludedExtensions"];
const MB = 1024 * 1024;

// Reading cookies needs its own permission, which the extension does not hold
// until the user asks for session handover, and gives back when they stop.
const SESSION_PERMISSIONS = { permissions: ["cookies"], origins: ["<all_urls>"] };

function saved() {
  document.getElementById("saved").textContent = chrome.i18n.getMessage("saved");
}

chrome.storage.local.get(
  { takeoverEnabled: false, sessionHandover: false, minimumBytes: 0, excludedHosts: "", excludedExtensions: "" },
  async (values) => {
    document.getElementById("takeover").checked = values.takeoverEnabled;
    document.getElementById("minimumSize").value = Math.round(Number(values.minimumBytes || 0) / MB);
    document.getElementById("excludedHosts").value = values.excludedHosts;
    document.getElementById("excludedExtensions").value = values.excludedExtensions;

    // The setting only counts while the browser still grants the permission.
    const granted = await chrome.permissions.contains(SESSION_PERMISSIONS);
    document.getElementById("sessionHandover").checked = values.sessionHandover && granted;
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
    await chrome.permissions.remove(SESSION_PERMISSIONS).catch(() => false);
  }

  saved();
});
