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

// The host answers `accepted: true` only after the task is saved in the
// application's database, so the browser's copy is cancelled only then.
async function handoff(message) {
  try {
    const response = await chrome.runtime.sendNativeMessage(NATIVE_HOST, message);
    return response ?? { accepted: false, error: "no response" };
  } catch (error) {
    return { accepted: false, error: String(error?.message ?? error) };
  }
}

function sendLink(url, referrer) {
  return handoff({
    type: "download",
    url,
    filenameHint: null,
    referrer: referrer || null,
    userAgent: navigator.userAgent
  });
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({ id: "download-manager-link", title: "Download with Download Manager", contexts: ["link"] });
  chrome.contextMenus.create({ id: "download-manager-selection", title: "Send selected links to Download Manager", contexts: ["selection"] });
});

chrome.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId === "download-manager-link" && info.linkUrl) {
    await sendLink(info.linkUrl, info.pageUrl);
  } else if (info.selectionText) {
    await handoff({ type: "inspect", text: info.selectionText });
  }
});

chrome.downloads.onCreated.addListener(async (download) => {
  const options = await settings();
  const url = download.finalUrl || download.url || "";
  if (!options.takeoverEnabled || !/^https?:\/\//i.test(url)) return;
  if (download.fileSize > 0 && download.fileSize < Number(options.minimumBytes)) return;
  if (hostExcluded(url, options.excludedHosts) || extensionExcluded(download.filename, options.excludedExtensions)) return;

  // Hold the browser's transfer while the application saves the task, and
  // let it continue untouched if anything goes wrong.
  await chrome.downloads.pause(download.id).catch(() => {});
  const response = await sendLink(url, download.referrer);
  if (response.accepted) {
    await chrome.downloads.cancel(download.id).catch(() => {});
    await chrome.downloads.erase({ id: download.id }).catch(() => {});
  } else {
    await chrome.downloads.resume(download.id).catch(() => {});
  }
});

chrome.runtime.onMessage.addListener((message, _sender, reply) => {
  if (message?.type === "status") {
    handoff({ type: "ping" }).then(reply);
    return true;
  }
  return false;
});
