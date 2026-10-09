const message = (key, substitutions) => chrome.i18n.getMessage(key, substitutions);
const statusElement = document.getElementById("status");
const connectionPanel = document.getElementById("connectionPanel");

async function refreshConnection() {
  statusElement.textContent = "…";
  let host;
  try { host = await chrome.runtime.sendMessage({ type: "status" }); } catch { host = null; }
  const connected = Boolean(host?.accepted && host.appFound);
  statusElement.textContent = host?.accepted
    ? host.appFound ? message("statusConnected") : message("statusNoApp")
    : message("statusNoHost");
  connectionPanel.dataset.state = connected ? "connected" : "missing";
}

async function refreshPopup() {
  const stored = await chrome.storage.local.get({ takeoverEnabled: false });
  document.getElementById("takeover").textContent = message(stored.takeoverEnabled ? "takeoverOn" : "takeoverOff");
  await refreshConnection();
}

document.getElementById("settings").addEventListener("click", () => chrome.runtime.openOptionsPage());
document.getElementById("settings").title = message("popupSettings");
document.getElementById("settings").setAttribute("aria-label", message("popupSettings"));
document.getElementById("checkConnection").addEventListener("click", async () => {
  const button = document.getElementById("checkConnection");
  button.disabled = true;
  try {
    if (/Firefox\//.test(navigator.userAgent)) {
      await chrome.permissions.request({ data_collection: ["technicalAndInteraction"] }).catch(() => false);
    }
    await refreshConnection();
  } finally {
    button.disabled = false;
  }
});

void refreshPopup();
