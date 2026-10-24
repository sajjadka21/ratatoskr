import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";
import { Window } from "happy-dom";
const source = await readFile(new URL("../youtube-menu.js", import.meta.url), "utf8");
test("YouTube's own first right-click menu gets one working download action", async () => {
  const window = new Window({ url: "https://www.youtube.com/watch?v=test" });
  const messages = [];
  window.document.body.innerHTML = '<div class="ytp-contextmenu"><div class="ytp-panel-menu"></div></div>';
  vm.runInNewContext(source, { window, document: window.document, location: window.location,
    setTimeout: (fn) => fn(), chrome: { i18n: { getMessage: key => key }, runtime: { sendMessage: async message => { messages.push(message); return { accepted: true }; } } } });
  window.document.dispatchEvent(new window.MouseEvent("contextmenu", { bubbles: true }));
  window.document.dispatchEvent(new window.MouseEvent("contextmenu", { bubbles: true }));
  const items = window.document.querySelectorAll("[data-ratatoskr-download]");
  assert.equal(items.length, 1);
  assert.equal(items[0].getAttribute("role"), "menuitem");
  items[0].click();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(messages.length, 1);
  assert.equal(messages[0].type, "download-page");
  assert.equal(messages[0].url, "https://www.youtube.com/watch?v=test");
  assert.equal(items[0].textContent, "overlaySent");
  await window.happyDOM.close();
});
