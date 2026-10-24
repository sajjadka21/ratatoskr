// YouTube replaces the browser's first right-click menu with its own player
// menu. Add the same deliberate download action there as well. No page
// scripts are evaluated and only the current video page URL is sent.
(() => {
  if (window.top !== window || !/(^|\.)youtube\.com$/.test(location.hostname)) return;
  document.addEventListener("contextmenu", () => {
    if (!/^\/(watch|shorts\/|live\/|embed\/)/.test(location.pathname)) return;
    setTimeout(() => {
      const menu = document.querySelector(".ytp-contextmenu .ytp-panel-menu");
      if (!menu || menu.querySelector("[data-ratatoskr-download]")) return;
      const item = document.createElement("button");
      item.type = "button";
      item.className = "ytp-menuitem";
      item.dataset.ratatoskrDownload = "true";
      item.setAttribute("role", "menuitem");
      item.style.cssText = "width:100%;border:0;background:transparent;color:inherit;font:inherit;text-align:start;cursor:pointer";
      const label = document.createElement("span");
      label.className = "ytp-menuitem-label";
      label.textContent = chrome.i18n.getMessage("menuPage");
      item.append(label);
      item.addEventListener("click", async (event) => {
        event.preventDefault();
        event.stopPropagation();
        item.disabled = true;
        try {
          const result = await chrome.runtime.sendMessage({ type: "download-page", url: location.href });
          label.textContent = chrome.i18n.getMessage(result?.accepted ? "overlaySent" : "popupImageFailed");
        } catch {
          label.textContent = chrome.i18n.getMessage("popupImageFailed");
        } finally {
          item.disabled = false;
        }
      });
      menu.append(item);
    }, 0);
  }, true);
})();
