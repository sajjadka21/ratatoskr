const message = (key) => chrome.i18n.getMessage(key);

Promise.all([
  chrome.storage.local.get({ takeoverEnabled: false }),
  chrome.runtime.sendMessage({ type: "status" })
]).then(([{ takeoverEnabled }, host]) => {
  document.getElementById("status").textContent = host?.accepted
    ? host.appFound ? message("statusConnected") : message("statusNoApp")
    : message("statusNoHost");
  document.getElementById("takeover").textContent = message(takeoverEnabled ? "takeoverOn" : "takeoverOff");
});

document.getElementById("settings").addEventListener("click", () => chrome.runtime.openOptionsPage());
document.getElementById("checkConnection").addEventListener("click", async () => {
  if (/Firefox\//.test(navigator.userAgent)) {
    await chrome.permissions.request({ data_collection: ["technicalAndInteraction"] }).catch(() => false);
  }
  const host = await chrome.runtime.sendMessage({ type: "status" });
  document.getElementById("status").textContent = host?.accepted
    ? host.appFound ? message("statusConnected") : message("statusNoApp")
    : message("statusNoHost");
});
