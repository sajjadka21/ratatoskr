import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { setImmediate } from "node:timers/promises";
import test from "node:test";
import vm from "node:vm";

const background = await readFile(new URL("../browser-extension/background.js", import.meta.url), "utf8");
const heartbeat = "ratatoskr-native-heartbeat";

function event() {
  const listeners = [];
  return {
    addListener(listener) { listeners.push(listener); },
    async emit(...args) { await Promise.all(listeners.map(listener => listener(...args))); },
  };
}

async function settle() { await setImmediate(); }

async function worker({ userAgent = "Chrome/128.0", brave = false, existingAlarm = false, nativeFailure = false,
  firefoxConsent = false, permissionFailure = false } = {}) {
  const alarms = new Map(existingAlarm ? [[heartbeat, { name: heartbeat, periodInMinutes: 1 }]] : []);
  const sent = [];
  const createdAlarms = [];
  const effects = [];
  const permissionChecks = [];
  const chrome = {
    runtime: {
      id: "test-extension",
      onStartup: event(), onInstalled: event(), onMessage: event(),
      async sendNativeMessage(host, message) {
        sent.push({ host, message: JSON.parse(JSON.stringify(message)) });
        if (nativeFailure) throw new Error("fixture native host unavailable");
        return { accepted: true, appFound: true };
      },
    },
    alarms: {
      onAlarm: event(),
      async get(name) { return alarms.get(name); },
      async create(name, options) {
        createdAlarms.push({ name, ...JSON.parse(JSON.stringify(options)) });
        alarms.set(name, { name, ...options });
      },
    },
    storage: { local: { async get(defaults) { effects.push("read-settings"); return { ...defaults }; } } },
    permissions: {
      async contains(query) {
        permissionChecks.push(JSON.parse(JSON.stringify(query)));
        if (permissionFailure) throw new Error("fixture permission unavailable");
        if (query.data_collection) return firefoxConsent;
        effects.push("read-private-data-permission");
        return false;
      },
      async request() { effects.push("request-permission-without-user-click"); return false; },
    },
    cookies: { async getAll() { effects.push("read-cookies"); return []; } },
    history: { async search() { effects.push("read-history"); return []; } },
    tabs: { async create() { effects.push("open-tab"); } },
    downloads: {
      onCreated: event(),
      async pause() { effects.push("pause-download"); },
      async resume() { effects.push("resume-download"); },
      async cancel() { effects.push("cancel-download"); },
      async erase() { effects.push("erase-download"); },
    },
    contextMenus: { onClicked: event(), removeAll(callback) { callback(); }, create() {} },
    i18n: { getMessage(key) { return key; } },
  };
  const navigator = { userAgent, ...(brave ? { brave: { isBrave: async () => true } } : {}) };
  vm.runInNewContext(background, { chrome, navigator, URL, Date, console }, { filename: "background.js" });
  await settle();
  return { chrome, alarms, sent, createdAlarms, effects, permissionChecks };
}

test("worker startup restores the missing one-minute heartbeat alarm and sends a ping", async () => {
  const fixture = await worker();
  assert.deepEqual(fixture.createdAlarms, [{ name: heartbeat, periodInMinutes: 1 }]);
  assert.equal(fixture.sent.length, 1);
  assert.equal(fixture.sent[0].host, "com.download_manager.native");
  assert.deepEqual(fixture.sent[0].message, { type: "ping", browser: "chrome" });
});

test("an existing alarm survives worker startup without being duplicated", async () => {
  const fixture = await worker({ existingAlarm: true });
  assert.deepEqual(fixture.createdAlarms, []);
  assert.equal(fixture.sent.length, 1);
});

test("browser startup recreates an alarm that was removed after worker initialization", async () => {
  const fixture = await worker();
  fixture.alarms.delete(heartbeat);
  await fixture.chrome.runtime.onStartup.emit();
  await settle();
  assert.equal(fixture.createdAlarms.length, 2);
  assert.deepEqual(fixture.createdAlarms[1], { name: heartbeat, periodInMinutes: 1 });
  assert.equal(fixture.sent.length, 2);
});

test("only the heartbeat alarm contacts the native host", async () => {
  const fixture = await worker();
  await fixture.chrome.alarms.onAlarm.emit({ name: "some-other-extension-job" });
  await settle();
  assert.equal(fixture.sent.length, 1);
  await fixture.chrome.alarms.onAlarm.emit({ name: heartbeat });
  await settle();
  assert.equal(fixture.sent.length, 2);
  assert.deepEqual(fixture.sent[1].message, { type: "ping", browser: "chrome" });
});

for (const [family, userAgent, brave] of [
  ["firefox", "Mozilla/5.0 Firefox/128.0 fixture-history-marker", false],
  ["edge", "Mozilla/5.0 Chrome/128.0 Edg/128.0 fixture-history-marker", false],
  ["brave", "Mozilla/5.0 Chrome/128.0 fixture-history-marker", true],
  ["chrome", "Mozilla/5.0 Chrome/128.0 fixture-history-marker", false],
]) {
  test(`${family} heartbeat transmits only the browser family and never reads private browser data`, async () => {
    const fixture = await worker({ userAgent, brave, firefoxConsent: family === "firefox" });
    assert.deepEqual(fixture.sent[0].message, { type: "ping", browser: family });
    assert.deepEqual(Object.keys(fixture.sent[0].message).sort(), ["browser", "type"]);
    assert.equal(JSON.stringify(fixture.sent).includes(userAgent), false);
    assert.equal(JSON.stringify(fixture.sent).includes("fixture-history-marker"), false);
    assert.deepEqual(fixture.effects, []);
    assert.deepEqual(fixture.permissionChecks, family === "firefox"
      ? [{ data_collection: ["technicalAndInteraction"] }] : []);
  });
}

test("native host failure keeps retries passive and never performs a download or opens an application tab", async () => {
  const fixture = await worker({ nativeFailure: true });
  await fixture.chrome.alarms.onAlarm.emit({ name: heartbeat });
  await settle();
  assert.equal(fixture.sent.length, 2);
  assert.deepEqual(fixture.sent.map(entry => entry.message), [
    { type: "ping", browser: "chrome" }, { type: "ping", browser: "chrome" },
  ]);
  assert.deepEqual(fixture.effects, []);
  assert.equal(fixture.alarms.get(heartbeat).periodInMinutes, 1);
});

test("the popup status request sends an identified Firefox ping only with technical consent", async () => {
  const fixture = await worker({ userAgent: "Firefox/128.0", firefoxConsent: true });
  let reply;
  await fixture.chrome.runtime.onMessage.emit({ type: "status" }, { id: "test-extension" }, value => { reply = value; });
  await settle();
  assert.deepEqual(fixture.sent.at(-1).message, { type: "ping", browser: "firefox" });
  assert.equal(reply.accepted, true);
  assert.deepEqual(fixture.effects, []);
});

test("Firefox without optional technical consent keeps its anonymous legacy ping and never asks for cookies", async () => {
  const fixture = await worker({ userAgent: "Mozilla/5.0 Firefox/140.0" });
  assert.deepEqual(fixture.sent[0].message, { type: "ping" });
  assert.deepEqual(fixture.permissionChecks, [{ data_collection: ["technicalAndInteraction"] }]);
  let reply;
  await fixture.chrome.runtime.onMessage.emit({ type: "status" }, { id: "test-extension" }, value => { reply = value; });
  await settle();
  assert.deepEqual(fixture.sent.at(-1).message, { type: "ping" });
  assert.equal(reply.accepted, true);
  assert.deepEqual(fixture.effects, []);
});

test("a Firefox consent lookup failure fails closed to an anonymous ping", async () => {
  const fixture = await worker({ userAgent: "Firefox/140.0", permissionFailure: true });
  assert.equal(fixture.sent.length, 1);
  assert.deepEqual(fixture.sent[0].message, { type: "ping" });
  assert.deepEqual(fixture.effects, []);
});
