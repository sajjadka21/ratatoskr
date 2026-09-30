// On video sites Ratatosk downloads with yt-dlp, a small "Download with
// Ratatosk" button appears over the video while the pointer is on it, like
// other download managers show. It sends only the page address; the app
// then asks for the quality. Nothing on the page is read or changed besides
// placing the button.
(() => {
  if (window.top !== window) return;

  const MIN_WIDTH = 200;
  const MIN_HEIGHT = 120;
  const HIDE_AFTER_MS = 2500;

  const host = document.createElement("div");
  host.style.cssText = "position:fixed;z-index:2147483647;top:0;left:0;display:none;";
  const shadow = host.attachShadow({ mode: "closed" });
  const style = document.createElement("style");
  style.textContent = `
    button {
      display: inline-flex; align-items: center; gap: 6px;
      padding: 6px 10px 6px 8px; border: 0; border-radius: 8px;
      font: 600 12.5px system-ui, "Segoe UI", Tahoma, sans-serif;
      color: #04201d; background: #34c3b4; cursor: pointer;
      box-shadow: 0 4px 14px rgba(0, 0, 0, .35); opacity: .95;
    }
    button:hover { opacity: 1; }
    button:focus-visible { outline: 2px solid #fff; outline-offset: 2px; }
    svg { width: 15px; height: 15px; }
  `;
  const button = document.createElement("button");
  button.type = "button";
  button.innerHTML =
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 4v11"/><path d="m7 10 5 5 5-5"/><path d="M5 20h14"/></svg>';
  const label = document.createElement("span");
  label.textContent = chrome.i18n.getMessage("overlayButton");
  button.append(label);
  button.dir = /^(fa|ar|he|ur)\b/i.test(chrome.i18n.getUILanguage()) ? "rtl" : "ltr";
  shadow.append(style, button);

  let current = null;
  let hideTimer = 0;

  function place() {
    if (!current || !current.isConnected) return hide();
    const box = current.getBoundingClientRect();
    if (box.width < MIN_WIDTH || box.height < MIN_HEIGHT || box.bottom < 0 || box.top > innerHeight) {
      return hide();
    }
    host.style.display = "block";
    const width = host.offsetWidth || 170;
    host.style.top = `${Math.max(8, box.top + 10)}px`;
    host.style.left = `${Math.max(8, Math.min(innerWidth - width - 8, box.right - width - 10))}px`;
  }

  function hide() {
    host.style.display = "none";
    current = null;
  }

  // Feed and home pages play previews that are not the page's own video.
  function pageIsVideo() {
    const host = location.hostname;
    const path = location.pathname;
    if (/(^|\.)youtube\.com$/.test(host)) {
      return /^\/(watch|shorts\/|live\/|embed\/)/.test(path);
    }
    return path.length > 1;
  }

  function directSource(video) {
    const source = video?.currentSrc || video?.src || "";
    return /^https?:\/\//i.test(source) && /\.(mp4|webm|mkv|mov|m4v|mp3|m4a)(\?|$)/i.test(source)
      ? source
      : null;
  }

  function show(video) {
    if (!pageIsVideo() && !directSource(video)) return;
    current = video;
    if (!host.isConnected) document.documentElement.append(host);
    place();
    clearTimeout(hideTimer);
    hideTimer = setTimeout(hide, HIDE_AFTER_MS);
  }

  // The video under the pointer, even with the site's controls over it.
  function videoAt(x, y) {
    for (const video of document.querySelectorAll("video")) {
      const box = video.getBoundingClientRect();
      if (x >= box.left && x <= box.right && y >= box.top && y <= box.bottom) return video;
    }
    return null;
  }

  document.addEventListener(
    "mousemove",
    (event) => {
      if (host.contains(event.target)) return;
      const video = videoAt(event.clientX, event.clientY);
      if (video) show(video);
    },
    { passive: true }
  );
  host.addEventListener("mouseenter", () => clearTimeout(hideTimer));
  host.addEventListener("mouseleave", () => {
    hideTimer = setTimeout(hide, 800);
  });
  addEventListener("scroll", place, { passive: true });
  addEventListener("resize", place, { passive: true });

  button.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    // A plain file (not a stream the page assembles) downloads directly;
    // otherwise the page goes to yt-dlp, which finds the video itself.
    const direct = directSource(current);
    chrome.runtime.sendMessage(
      direct
        ? { type: "download-file", url: direct, referrer: location.href }
        : { type: "download-page", url: location.href }
    );
    label.textContent = chrome.i18n.getMessage("overlaySent");
    setTimeout(() => {
      label.textContent = chrome.i18n.getMessage("overlayButton");
      hide();
    }, 1500);
  });
})();
