const fields = ["takeover", "minimumBytes", "excludedHosts", "excludedExtensions"];
chrome.storage.local.get({ takeoverEnabled: false, minimumBytes: 0, excludedHosts: "", excludedExtensions: "" }, (values) => {
  document.getElementById("takeover").checked = values.takeoverEnabled;
  document.getElementById("minimumBytes").value = values.minimumBytes;
  document.getElementById("excludedHosts").value = values.excludedHosts;
  document.getElementById("excludedExtensions").value = values.excludedExtensions;
});
fields.forEach((id) => document.getElementById(id).addEventListener("change", () => {
  chrome.storage.local.set({
    takeoverEnabled: document.getElementById("takeover").checked,
    minimumBytes: Math.max(0, Number(document.getElementById("minimumBytes").value || 0)),
    excludedHosts: document.getElementById("excludedHosts").value,
    excludedExtensions: document.getElementById("excludedExtensions").value
  }, () => { document.getElementById("saved").textContent = "Saved locally"; });
}));
