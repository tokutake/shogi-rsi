// Run after `sh scripts/build-web.sh`: node tests/web-offline.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

const scope = "https://example.github.io/shogi-rsi/";
const cachePrefix = `shogi-rsi-offline:${scope}:`;
const source = await readFile(new URL("../dist/sw.js", import.meta.url), "utf8");
assert.match(source, /CACHE_PREFIX \+ "[a-f0-9]{16}"/);
const handlers = new Map();
const stores = new Map([
  [`${cachePrefix}previous`, new Map()],
  ["shogi-rsi-offline:https://example.github.io/another-app/:keep", new Map()],
  ["unrelated-cache", new Map()],
]);
let claimed = false;
const context = vm.createContext({
  URL,
  self: {
    registration: { scope },
    location: { origin: new URL(scope).origin },
    clients: { claim: async () => { claimed = true; } },
    addEventListener: (event, handler) => handlers.set(event, handler),
  },
  caches: {
    keys: async () => [...stores.keys()],
    delete: async key => stores.delete(key),
    open: async key => {
      if (!stores.has(key)) stores.set(key, new Map());
      const entries = stores.get(key);
      return {
        addAll: async urls => {
          for (const url of urls) {
            assert.ok(url.startsWith("./"), "assets must be relative to the Pages subdirectory");
            const filename = url === "./" ? "index.html" : url.slice(2);
            entries.set(new URL(url, scope).href, await readFile(new URL(`../dist/${filename}`, import.meta.url)));
          }
        },
        match: async request => entries.get(request.url),
      };
    },
  },
  fetch: async () => { throw new Error("Network is offline"); },
});
vm.runInContext(source, context);
let pending;
handlers.get("install")({ waitUntil: task => { pending = task; } });
await pending;
handlers.get("activate")({ waitUntil: task => { pending = task; } });
await pending;
assert.equal(claimed, true);
assert.equal(stores.has(`${cachePrefix}previous`), false);
assert.equal(stores.has("shogi-rsi-offline:https://example.github.io/another-app/:keep"), true);
assert.equal(stores.has("unrelated-cache"), true);
for (const filename of ["", "index.html", "engine-worker.js", "shogi_rsi.wasm", "manifest.webmanifest", "icon.svg", "apple-touch-icon.png"]) {
  pending = undefined;
  handlers.get("fetch")({
    request: { method: "GET", url: new URL(filename, scope).href },
    respondWith: task => { pending = task; },
  });
  assert.ok((await pending).length > 0, `${filename || "start URL"} must load without network`);
}
for (const request of [
  { method: "POST", url: `${scope}api/game` },
  { method: "GET", url: "https://other.example/file" },
]) {
  handlers.get("fetch")({ request, respondWith: () => assert.fail("request must not be intercepted") });
}
console.log("Pages subdirectory and offline cache checks passed");
