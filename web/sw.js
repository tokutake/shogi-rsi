"use strict";
const CACHE_PREFIX = `shogi-rsi-offline:${self.registration.scope}:`;
const CACHE = CACHE_PREFIX + "development";
const ASSETS = ["./", "./index.html", "./engine-worker.js", "./shogi_rsi.wasm", "./manifest.webmanifest", "./icon.svg", "./apple-touch-icon.png"];
self.addEventListener("install", event => {
  event.waitUntil(caches.open(CACHE).then(cache => cache.addAll(ASSETS)));
});
self.addEventListener("activate", event => {
  event.waitUntil(caches.keys().then(keys => Promise.all(
    keys.filter(key => key.startsWith(CACHE_PREFIX) && key !== CACHE)
      .map(key => caches.delete(key)),
  )).then(() => self.clients.claim()));
});
self.addEventListener("fetch", event => {
  if (event.request.method !== "GET" || new URL(event.request.url).origin !== self.location.origin) return;
  event.respondWith(caches.open(CACHE).then(async cache =>
    (await cache.match(event.request)) || fetch(event.request),
  ));
});
