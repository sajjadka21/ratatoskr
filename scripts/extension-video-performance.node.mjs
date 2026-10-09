import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";

test("video overlay coalesces rapid mouse movement and checks only elements under the pointer", async () => {
  const documentListeners = new Map();
  const pointChecks = [];
  const host = {
    style: {},
    isConnected: false,
    offsetWidth: 170,
    contains: () => false,
    attachShadow: () => ({ append() {} }),
    addEventListener() {},
  };
  const video = {
    tagName: "VIDEO",
    isConnected: true,
    getBoundingClientRect: () => ({ left: 20, top: 20, right: 400, bottom: 300, width: 380, height: 280 }),
  };
  const document = {
    createElement: (tag) => tag === "div" ? host : ({ style: {}, addEventListener() {}, append() {} }),
    documentElement: {
      append(element) {
        element.isConnected = true;
      },
    },
    addEventListener(name, listener) {
      documentListeners.set(name, listener);
    },
    elementsFromPoint(x, y) {
      pointChecks.push([x, y]);
      return [video];
    },
    querySelectorAll() {
      throw new Error("the overlay must not scan every video in the document");
    },
  };
  const window = { innerWidth: 800, innerHeight: 600, setTimeout, clearTimeout };
  window.top = window;
  const chrome = { i18n: { getMessage: (key) => key, getUILanguage: () => "en" } };
  const location = { hostname: "www.youtube.com", pathname: "/watch", href: "https://www.youtube.com/watch?v=test" };

  runInNewContext(readFileSync("browser-extension/video.js", "utf8"), {
    window,
    document,
    chrome,
    location,
    innerHeight: window.innerHeight,
    innerWidth: window.innerWidth,
    setTimeout,
    clearTimeout,
    addEventListener() {},
  });

  const onMouseMove = documentListeners.get("mousemove");
  assert.equal(typeof onMouseMove, "function");
  for (let index = 0; index < 1_000; index += 1) {
    onMouseMove({ target: {}, clientX: index, clientY: index });
  }

  await new Promise((resolve) => setTimeout(resolve, 80));
  assert.deepEqual(pointChecks, [[999, 999]]);
  assert.equal(host.style.display, "block");
});
