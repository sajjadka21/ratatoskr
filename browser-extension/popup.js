chrome.storage.local.get({ takeoverEnabled: false }, ({ takeoverEnabled }) => {
  document.getElementById("status").textContent = takeoverEnabled ? "Takeover enabled" : "Takeover disabled";
});
document.getElementById("settings").addEventListener("click", () => chrome.runtime.openOptionsPage());
