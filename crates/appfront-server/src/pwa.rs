//! Progressive Web App (offline) support: generates a `service-worker.js`,
//! a `manifest.webmanifest`, and the HTML glue (manifest `<link>` +
//! registration `<script>`) needed to make an `appfront` app installable and

/// Configuration for the generated PWA assets.
#[derive(Debug, Clone)]
pub struct PwaConfig {
    /// Human-readable app name (used in the install prompt / launcher).
    pub name: String,
    /// Short name shown under the home-screen icon.
    pub short_name: String,
    /// One-line description (also reused for the manifest).
    pub description: String,
    /// Theme color (address bar / splash). Any CSS color is fine.
    pub theme_color: String,
    /// Background color for the splash screen.
    pub background_color: String,
    /// URL the PWA opens at (usually `/`).
    pub start_url: String,
    /// Cache storage name — bump to force a service-worker update.
    pub cache_name: String,
    /// Asset paths (relative to the origin) to precache on install, e.g.
    /// `"/"`, `"/app.wasm"`, `"/app.js"`.
    pub precache: Vec<String>,
    /// Web Push: the application server key (VAPID public key, base64url) the
    /// client uses to subscribe to push notifications. When `None`, push
    /// subscription code is omitted from the generated service worker.
    pub push_vapid_public_key: Option<String>,
    /// Background sync: the tag name registered so queued offline actions flush
    /// on reconnect. When `None`, background-sync registration is omitted.
    pub background_sync_tag: Option<String>,
    /// When `true`, the generated service worker posts a `message` to clients
    /// on `controllerchange` so the page can show a "new version ready, reload"
    /// prompt instead of silently swapping in the new worker.
    pub update_available_prompt: bool,
}

impl Default for PwaConfig {
    fn default() -> Self {
        PwaConfig {
            name: "AppFront App".to_string(),
            short_name: "App".to_string(),
            description: String::new(),
            theme_color: "#3b82f6".to_string(),
            background_color: "#ffffff".to_string(),
            start_url: "/".to_string(),
            cache_name: "appfront-pwa-v1".to_string(),
            precache: vec!["/".to_string()],
            push_vapid_public_key: None,
            background_sync_tag: None,
            update_available_prompt: true,
        }
    }
}

/// Generates a `service-worker.js` (offline-first: precache on install,
/// serve cache-first, fall back to network and re-cache, then to the
/// start URL when both fail). When `cfg.push_vapid_public_key` is set, the
/// worker also handles `push` events and subscribes on activation; when
/// `cfg.background_sync_tag` is set, a `sync` listener flushes the named
/// queue; when `cfg.update_available_prompt` is set, a new worker posts a
/// `message` to clients on `controllerchange` so the page can prompt a reload.
pub fn service_worker(cfg: &PwaConfig) -> String {
    let assets_literal = cfg
        .precache
        .iter()
        .map(|a| format!("{a:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    // Used as the final offline fallback when both cache and network miss.
    let fallback = cfg.start_url.clone();

    let push_block = match &cfg.push_vapid_public_key {
        Some(key) => format!(
            r#"
// Web Push: subscribe on activation and surface incoming pushes to clients.
self.addEventListener('push', (event) => {{
  const data = event.data ? event.data.text() : '';
  event.waitUntil(
    self.registration.showNotification({name:?}, {{ body: data }})
  );
}});

self.addEventListener('notificationclick', (event) => {{
  event.notification.close();
  event.waitUntil(self.clients.matchAll({{ type: 'window' }}).then((cs) => {{
    if (cs[0]) return cs[0].focus();
  }}));
}});

self.addEventListener('activate', (event) => {{
  const sub = {{
    userVisibleOnly: true,
    applicationServerKey: urlBase64ToUint8Array({key:?}),
  }};
  event.waitUntil(
    self.registration.pushManager.getSubscription().then((s) =>
      s || self.registration.pushManager.subscribe(sub)
    ).catch(() => {{}})
  );
}});
"#,
            name = cfg.name,
            key = key,
        ),
        None => String::new(),
    };

    let sync_block = match &cfg.background_sync_tag {
        Some(tag) => format!(
            r#"
// Background sync: when connectivity returns, replay the named queue so
// offline actions (queued by the page) flush automatically.
self.addEventListener('sync', (event) => {{
  if (event.tag === {tag:?}) {{
    event.waitUntil(
      self.clients.matchAll().then((cs) => cs.forEach((c) => c.postMessage({{ type: 'sync', tag: {tag:?} }})))
    );
  }}
}});
"#,
            tag = tag,
        ),
        None => String::new(),
    };

    let update_block = if cfg.update_available_prompt {
        r#"
// Update-available UX: when a new worker takes control, tell the page so it
// can show a "new version ready, reload to update" prompt instead of silently
// swapping in the new build.
self.addEventListener('controllerchange', () => {
  self.clients.matchAll().then((cs) => cs.forEach((c) => c.postMessage({ type: 'appfront-update-available' })));
});
"#
        .to_string()
    } else {
        String::new()
    };

    format!(
        r#"// Generated by appfront — offline-first service worker.
const CACHE = {cache:?};
const ASSETS = [{assets}];

self.addEventListener('install', (event) => {{
  event.waitUntil(
    caches.open(CACHE).then((cache) => cache.addAll(ASSETS)).then(() => self.skipWaiting())
  );
}});

self.addEventListener('activate', (event) => {{
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))
    ).then(() => self.clients.claim())
  );
}});

self.addEventListener('fetch', (event) => {{
  const req = event.request;
  if (req.method !== 'GET') return;
  // Navigations are network-first so a redeploy isn't served stale from the
  // cache; only when the network is unreachable do we fall back to cached
  // content (cache_name still bounds asset staleness for non-navigation GETs).
  if (req.mode === 'navigate') {{
    event.respondWith(
      fetch(req).then((resp) => {{
        const copy = resp.clone();
        caches.open(CACHE).then((cache) => cache.put(req, copy));
        return resp;
      }}).catch(() => caches.match(req).then((c) => c || caches.match({fallback:?})))
    );
    return;
  }}
  event.respondWith(
    caches.match(req).then((cached) =>
      cached ||
      fetch(req).then((resp) => {{
        const copy = resp.clone();
        caches.open(CACHE).then((cache) => cache.put(req, copy));
        return resp;
      }}).catch(() => caches.match({fallback:?}))
    )
  );
}});
{update_block}{sync_block}{push_block}

// base64url -> Uint8Array (VAPID applicationServerKey helper)
function urlBase64ToUint8Array(base64String) {{
  const padding = '='.repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding).replace(/-/g, '+').replace(/_/g, '/');
  const raw = atob(base64);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out;
}}
"#,
        cache = cfg.cache_name,
        assets = assets_literal,
        fallback = fallback,
        update_block = update_block,
        sync_block = sync_block,
        push_block = push_block,
    )
}


/// Generates a `manifest.webmanifest` (JSON).
pub fn manifest(cfg: &PwaConfig) -> String {
    let json = serde_json::json!({
        "name": cfg.name,
        "short_name": cfg.short_name,
        "description": cfg.description,
        "start_url": cfg.start_url,
        "display": "standalone",
        "background_color": cfg.background_color,
        "theme_color": cfg.theme_color,
        "icons": [],
    });
    serde_json::to_string_pretty(&json).unwrap_or_default()
}

/// `<link>` tag pointing at the web manifest. Embed once in `<head>`.
pub fn manifest_link() -> String {
    r#"<link rel="manifest" href="/manifest.webmanifest">"#.to_string()
}

/// `<script>` registering the service worker. Embed once in `<body>`;
/// registration is a no-op where `serviceWorker` is unavailable. `nonce` must
/// match the `Content-Security-Policy` `nonce` set on the document so the
/// inline script is allowed to run.
///
/// When `cfg.background_sync_tag` is set, the script asks the active worker to
/// register that `sync` tag so queued offline actions replay on reconnect.
pub fn registration_script(cfg: &PwaConfig, nonce: &str) -> String {
    let sync_register = match &cfg.background_sync_tag {
        Some(tag) => format!(
            r#"
  .then((reg) => reg.sync.register({tag:?}).catch(() => {{}}))"#,
            tag = tag,
        ),
        None => String::new(),
    };
    format!(
        r#"<script nonce="{nonce}">
if ('serviceWorker' in navigator) {{
  window.addEventListener('load', () => {{
    navigator.serviceWorker.register('/service-worker.js')
      .then((reg) => {{ if (reg.sync) {{ reg.sync.getTags().catch(() => {{}}){sync_register}; }} }}))
      .catch((e) => console.error('SW registration failed', e));
  }});
}}
</script>"#,
        nonce = nonce,
        sync_register = sync_register,
    )
}

/// `<script>` that listens for the `appfront-update-available` message posted
/// by the service worker on `controllerchange` and reloads the page so the new
/// build takes effect (today a new service worker installs silently with no
/// signal to the page). `nonce` must match the document CSP nonce. Replace the
/// body with a custom prompt by editing the listener.
pub fn update_available_script(nonce: &str) -> String {
    format!(
        r#"<script nonce="{nonce}">
if ('serviceWorker' in navigator) {{
  navigator.serviceWorker.addEventListener('message', (event) => {{
    if (event.data && event.data.type === 'appfront-update-available') {{
      window.location.reload();
    }}
    if (event.data && event.data.type === 'sync') {{
      window.dispatchEvent(new CustomEvent('appfront-bg-sync', {{ detail: event.data.tag }}));
    }}
  }});
}}
</script>"#,
        nonce = nonce,
    )
}


#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> PwaConfig {
        PwaConfig {
            precache: vec!["/".to_string(), "/app.wasm".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn service_worker_precaches_assets_and_fallbacks() {
        let sw = service_worker(&cfg());
        assert!(sw.contains("const CACHE = \"appfront-pwa-v1\""));
        assert!(sw.contains("\"/\""));
        assert!(sw.contains("\"/app.wasm\""));
        assert!(sw.contains("cache.addAll(ASSETS)"));
        assert!(sw.contains("cache.put(req, copy)"));
        assert!(sw.contains("caches.match(\"/\")"));
    }

    #[test]
    fn manifest_is_valid_json_with_expected_fields() {
        let m = manifest(&cfg());
        let v: serde_json::Value = serde_json::from_str(&m).expect("manifest is JSON");
        assert_eq!(v["name"], "AppFront App");
        assert_eq!(v["display"], "standalone");
        assert_eq!(v["start_url"], "/");
        assert!(v["icons"].is_array());
    }

    #[test]
    fn glue_snippets_are_well_formed() {
        assert!(manifest_link().contains(r#"rel="manifest""#));
        assert!(manifest_link().contains("/manifest.webmanifest"));
        assert!(registration_script(&cfg(), "test-nonce").contains("/service-worker.js"));
        assert!(registration_script(&cfg(), "test-nonce").contains("serviceWorker"));
        assert!(registration_script(&cfg(), "test-nonce").contains("nonce=\"test-nonce\""));
        assert!(update_available_script("test-nonce").contains("appfront-update-available"));
        assert!(update_available_script("test-nonce").contains("nonce=\"test-nonce\""));
    }

    #[test]
    fn service_worker_includes_push_when_configured() {
        let push_cfg = PwaConfig {
            push_vapid_public_key: Some("BLBx_abc123".to_string()),
            ..cfg()
        };
        let sw = service_worker(&push_cfg);
        assert!(sw.contains("pushManager.subscribe"));
        assert!(sw.contains("BLBx_abc123"));
        assert!(sw.contains("showNotification"));
        // And the plain config does NOT include push code.
        assert!(!service_worker(&cfg()).contains("pushManager"));
    }

    #[test]
    fn service_worker_includes_sync_when_configured() {
        let sync_cfg = PwaConfig {
            background_sync_tag: Some("appfront-sync".to_string()),
            ..cfg()
        };
        let sw = service_worker(&sync_cfg);
        assert!(sw.contains("event.tag === \"appfront-sync\""));
        assert!(!service_worker(&cfg()).contains("addEventListener('sync'"));
    }

    #[test]
    fn registration_script_wires_background_sync() {
        let sync_cfg = PwaConfig {
            background_sync_tag: Some("appfront-sync".to_string()),
            ..cfg()
        };
        let s = registration_script(&sync_cfg, "n");
        assert!(s.contains("reg.sync.register(\"appfront-sync\")"));
    }
}
