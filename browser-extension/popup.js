const status = document.getElementById("status");

Promise.all([
  chrome.storage.local.get({ takeoverEnabled: false }),
  chrome.runtime.sendMessage({ type: "status" })
]).then(([{ takeoverEnabled }, host]) => {
  const connection = host?.accepted
    ? host.appFound ? "Connected to Ratatosk" : "Host found, application not found"
    : "Native host not installed";
  status.textContent = `${connection} · Takeover ${takeoverEnabled ? "on" : "off"}`;
});

document.getElementById("settings").addEventListener("click", () => chrome.runtime.openOptionsPage());
