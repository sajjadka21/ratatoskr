const NATIVE_HOST = "com.download_manager.native";

async function settings() {
  return chrome.storage.local.get({
    takeoverEnabled: false,
    minimumBytes: 0,
    excludedHosts: "",
    excludedExtensions: ""
  });
}

function hostExcluded(url, value) {
  try {
    const host = new URL(url).hostname.toLowerCase();
    return value.split(/[\n,]/).map((entry) => entry.trim().toLowerCase()).filter(Boolean)
      .some((entry) => host === entry || host.endsWith(`.${entry}`));
  } catch { return true; }
}

function extensionExcluded(filename, value) {
  const extension = filename?.split(".").pop()?.toLowerCase();
  return extension && value.split(/[\n,]/).map((entry) => entry.trim().toLowerCase().replace(/^\./, ""))
    .filter(Boolean).includes(extension);
}

async function handoff(message) {
  const response = await chrome.runtime.sendNativeMessage(NATIVE_HOST, message);
  return response?.accepted === true;
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({ id: "download-manager-link", title: "Download with Download Manager", contexts: ["link"] });
  chrome.contextMenus.create({ id: "download-manager-selection", title: "Send selected links to Download Manager", contexts: ["selection"] });
});

chrome.contextMenus.onClicked.addListener(async (info) => {
  const value = info.linkUrl || info.selectionText || "";
  if (!value) return;
  try { await handoff({ type: "inspect", text: value }); } catch { /* browser flow remains unaffected */ }
});

chrome.downloads.onCreated.addListener(async (download) => {
  const options = await settings();
  if (!options.takeoverEnabled || !/^https?:\/\//i.test(download.url || "")) return;
  if (download.fileSize && download.fileSize < Number(options.minimumBytes)) return;
  if (hostExcluded(download.url, options.excludedHosts) || extensionExcluded(download.filename, options.excludedExtensions)) return;
  try {
    const accepted = await handoff({
      type: "download",
      url: download.url,
      filenameHint: download.filename || null,
      referrer: download.referrer || null,
      userAgent: null
    });
    if (accepted) await chrome.downloads.cancel(download.id);
  } catch { /* native host absence must never break browser downloads */ }
});
