// Holding Alt while clicking a link leaves that download to the browser, as
// in other download managers. Only registered when the user turns it on in
// the extension's options. It tells the extension that such a click
// happened; the link itself is not read or sent anywhere.
document.addEventListener(
  "click",
  (event) => {
    if (!event.altKey) return;
    const link = event.target instanceof Element ? event.target.closest("a[href]") : null;
    if (!link) return;
    chrome.runtime.sendMessage({ type: "alt-click" });
  },
  true
);
