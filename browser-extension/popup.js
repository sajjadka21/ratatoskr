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
