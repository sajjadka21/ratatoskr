const NATIVE_HOST = "com.download_manager.native";

const SESSION_PERMISSIONS = { permissions: ["cookies"], origins: ["<all_urls>"] };

// After an Alt+click on a link, the next download for a few seconds stays in
// the browser.
const ALT_WINDOW_MS = 4000;
let altUntil = 0;

async function settings() {
  return chrome.storage.local.get({
    takeoverEnabled: false,
    sessionHandover: false,
    minimumBytes: 0,
    excludedHosts: "",
    excludedExtensions: ""
  });
}

// The cookie header the browser itself would send to `url`: the browser
// applies domain, path and Secure matching, so nothing from another site is
// included. Returns null unless the user turned session handover on and the
// permission is still granted. Never stored and never logged here.
async function sessionFor(url) {
  const { sessionHandover } = await settings();
  if (!sessionHandover || !(await chrome.permissions.contains(SESSION_PERMISSIONS))) {
    return null;
  }

  try {
    const cookies = await chrome.cookies.getAll({ url });
    const header = cookies.map((cookie) => `${cookie.name}=${cookie.value}`).join("; ");
    return header || null;
  } catch {
    return null;
  }
}

function hostExcluded(url, value) {
  try {
    const host = new URL(url).hostname.toLowerCase();
    return value.split(/[\n,]/).map((entry) => entry.trim().toLowerCase()).filter(Boolean)
      .some((entry) => host === entry || host.endsWith(`.${entry}`));
  } catch { return true; }
}

// The browser often has no file name yet when a download starts, so the
// name is also taken from the link.
function extensionExcluded(filename, url, value) {
  let name = (filename || "").split(/[\\/]/).pop();
  if (!name || !name.includes(".")) {
    try { name = decodeURIComponent(new URL(url).pathname.split("/").pop() || ""); } catch { name = ""; }
  }
  const extension = name.includes(".") ? name.split(".").pop().toLowerCase() : "";
  return Boolean(extension) && value.split(/[\n,]/).map((entry) => entry.trim().toLowerCase().replace(/^\./, ""))
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

async function sendLink(url, referrer) {
  return handoff({
    type: "download",
    url,
    filenameHint: null,
    referrer: referrer || null,
    userAgent: navigator.userAgent,
    cookies: await sessionFor(url)
  });
}

// Video pages Ratatosk downloads with yt-dlp.
const VIDEO_PAGES = [
  "*://*.youtube.com/*",
  "*://youtu.be/*",
  "*://*.aparat.com/*",
  "*://*.vimeo.com/*",
  "*://*.instagram.com/*",
  "*://*.x.com/*",
  "*://*.twitter.com/*",
  "*://*.tiktok.com/*",
  "*://*.dailymotion.com/*",
  "*://*.twitch.tv/*",
  "*://*.facebook.com/*",
  "*://*.reddit.com/*",
  "*://*.soundcloud.com/*",
  "*://*.bilibili.com/*",
  "*://fb.watch/*",
  "*://*.youtube-nocookie.com/*",
  "*://v.redd.it/*"
];

const VIDEO_HOSTS = ["youtube.com", "youtu.be", "youtube-nocookie.com", "aparat.com", "vimeo.com",
  "dailymotion.com", "twitch.tv", "soundcloud.com", "bilibili.com", "fb.watch", "v.redd.it"];

// A link to a video page (not a file) on a site Ratatosk downloads with yt-dlp.
function isVideoPage(url) {
  try {
    const parsed = new URL(url);
    const host = parsed.hostname.toLowerCase();
    if (/\.(mp4|mkv|webm|mp3|m4a|zip|exe)$/i.test(parsed.pathname)) return false;
    return VIDEO_HOSTS.some((site) => host === site || host.endsWith(`.${site}`));
  } catch { return false; }
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: "download-manager-link", title: chrome.i18n.getMessage("menuLink"), contexts: ["link"] });
    chrome.contextMenus.create({ id: "download-manager-selection", title: chrome.i18n.getMessage("menuSelection"), contexts: ["selection"] });
    chrome.contextMenus.create({
      id: "download-manager-page",
      title: chrome.i18n.getMessage("menuPage"),
      contexts: ["page", "video"],
      documentUrlPatterns: VIDEO_PAGES
    });
  });
});

chrome.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId === "download-manager-link" && info.linkUrl && isVideoPage(info.linkUrl)) {
    await handoff({ type: "inspect", text: info.linkUrl });
  } else if (info.menuItemId === "download-manager-link" && info.linkUrl) {
    await sendLink(info.linkUrl, info.pageUrl);
  } else if (info.menuItemId === "download-manager-page" && info.pageUrl) {
    // The app opens its Add dialog for a video page, so the quality can be
    // chosen before it downloads.
    await handoff({ type: "inspect", text: info.pageUrl });
  } else if (info.selectionText) {
    await handoff({ type: "inspect", text: info.selectionText });
  }
});

chrome.downloads.onCreated.addListener(async (download) => {
  const options = await settings();
  const url = download.finalUrl || download.url || "";
  if (!options.takeoverEnabled || !/^https?:\/\//i.test(url)) return;
  if (Date.now() < altUntil) {
    altUntil = 0;
    return;
  }
  const size = download.totalBytes > 0 ? download.totalBytes : download.fileSize;
  if (size > 0 && size < Number(options.minimumBytes)) return;
  if (hostExcluded(url, options.excludedHosts) || extensionExcluded(download.filename, url, options.excludedExtensions)) return;

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

chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (message?.type === "status") {
    handoff({ type: "ping" }).then(reply);
    return true;
  }
  // Only this extension's own content scripts send the ones below.
  if (sender.id !== chrome.runtime.id) return false;
  if (message?.type === "alt-click") {
    altUntil = Date.now() + ALT_WINDOW_MS;
    return false;
  }
  if (message?.type === "download-page" && /^https?:\/\//i.test(message.url || "")) {
    handoff({ type: "inspect", text: message.url }).then(reply);
    return true;
  }
  if (message?.type === "download-file" && /^https?:\/\//i.test(message.url || "")) {
    sendLink(message.url, message.referrer || null).then(reply);
    return true;
  }
  return false;
});
