/* Deus Vult service worker: offline-first for the whole game bundle.
 *
 * Trunk emits content-hashed js/wasm assets referenced from index.html, so
 * the install step parses the HTML to discover them; anything missed is
 * picked up by the runtime cache on first fetch. Bump CACHE when the precache
 * contract changes; old caches are dropped on activate.
 */
const CACHE = "deusvult-v1";

const CORE = [
  "./",
  "index.html",
  "manifest.webmanifest",
  "icon.svg",
  "icon-192.png",
  "icon-512.png",
];

async function discoverAssets() {
  const urls = new Set(CORE.map((u) => new URL(u, self.registration.scope).href));
  try {
    const res = await fetch(new URL("index.html", self.registration.scope), {
      cache: "no-cache",
    });
    const html = await res.text();
    const asset = /["'`](\/?[\w./-]+\.(?:js|wasm))["'`]/g;
    for (const match of html.matchAll(asset)) {
      const url = new URL(match[1], self.registration.scope);
      if (url.origin === self.location.origin) urls.add(url.href);
    }
  } catch (err) {
    // Discovery failure is non-fatal: the runtime cache below backfills.
  }
  return [...urls];
}

self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      await cache.addAll(await discoverAssets());
      await self.skipWaiting();
    })(),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      for (const key of await caches.keys()) {
        if (key !== CACHE) await caches.delete(key);
      }
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;
  event.respondWith(
    (async () => {
      // Cache-first...
      const cached = await caches.match(request);
      if (cached) return cached;
      // ...network fallback, and runtime-cache new same-origin responses.
      const response = await fetch(request);
      if (response.ok && new URL(request.url).origin === self.location.origin) {
        const cache = await caches.open(CACHE);
        cache.put(request, response.clone());
      }
      return response;
    })(),
  );
});
