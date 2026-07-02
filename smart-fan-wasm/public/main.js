const $ticket = document.querySelector("#ticket");
const $connect = document.querySelector("#connect");
const $temp = document.querySelector("#temp");
const $hum = document.querySelector("#hum");
const $fan = document.querySelector("#fan-icon"); // absent in the thermometer variant
const $secret = document.querySelector("#secret"); // absent unless the fan is controllable
const $slider = document.querySelector("#threshold"); // "
const $thresholdLabel = document.querySelector("#threshold-label");
const $status = document.querySelector("#status");

// Namespace persisted state by the page's path segment, so multiple GUI variants
// embedded on one page (each in its own same-origin iframe) don't share an endpoint
// identity / ticket / secret via localStorage.
const NS = location.pathname.replace(/\/(index\.html)?$/, "").split("/").pop() || "smart-fan";
const SECRET_KEY = `${NS}:secret`;
const TICKET_KEY = `${NS}:ticket`;
const API_SECRET_KEY = `${NS}:api-secret`;

// Fresh readings are mid-blue and fade to a readable gray over FADE_MS.
const FRESH = [43, 108, 255]; // #2b6cff
const STALE = [138, 143, 152]; // #8a8f98
const FADE_MS = 30_000;

let node = null;
let current = null; // active Subscription handle
let connectedTicket = null; // the ticket `current` is polling
let lastReading = null;
let deviceThreshold = null; // the threshold the device last reported

// Params in the URL fragment (#…) — never sent to the server, so they stay out of
// access logs. This is where the secret must live.
function fragmentParams() {
  return new URLSearchParams(location.hash.replace(/^#/, ""));
}
// The ticket is lower-stakes, so accept it from the query (?ticket=) or the fragment.
function paramFromUrl(name) {
  return new URLSearchParams(location.search).get(name) ?? fragmentParams().get(name);
}

// Prefill the ticket: a ticket on the URL wins (and auto-connects below); otherwise
// fall back to the last ticket we stored.
const urlTicket = paramFromUrl("ticket");
$ticket.value = (urlTicket ?? localStorage.getItem(TICKET_KEY) ?? "").trim();

// The API secret (only present in the control variant) can also come from the URL, but
// the fragment ONLY — never a query string, which would reach the server. That's the
// "control" QR. Persist it like a manually-entered one.
if ($secret) {
  const urlSecret = fragmentParams().get("secret");
  $secret.value = (urlSecret ?? localStorage.getItem(API_SECRET_KEY) ?? "").trim();
  if (urlSecret) localStorage.setItem(API_SECRET_KEY, $secret.value);
}
refreshSlider();

// The flourish: both numbers share one color driven off `lastReading` — mid-blue when
// fresh, fading to gray over FADE_MS. Driving it from the shared timestamp keeps them
// exactly in sync.
function paintFreshness() {
  const t = lastReading ? Math.min((Date.now() - lastReading) / FADE_MS, 1) : 1;
  const c = FRESH.map((f, i) => Math.round(f + (STALE[i] - f) * t));
  const color = `rgb(${c[0]}, ${c[1]}, ${c[2]})`;
  $temp.style.color = color;
  $hum.style.color = color;
  requestAnimationFrame(paintFreshness);
}
requestAnimationFrame(paintFreshness);

function setFan(on) {
  if (!$fan) return;
  $fan.classList.toggle("spinning", on);
  $fan.setAttribute("aria-label", on ? "fan on" : "fan off");
}

// The slider is greyed out unless it exists and an API secret is present.
function refreshSlider() {
  if (!$slider) return;
  $slider.disabled = !$secret || $secret.value.trim() === "";
}

function setThresholdLabel(v) {
  if ($thresholdLabel) $thresholdLabel.textContent = `${Math.round(v)}°C`;
}

// Snap the slider back to the device's actual threshold (used when a set is refused).
function snapBack() {
  if ($slider && deviceThreshold != null) {
    $slider.value = deviceThreshold;
    setThresholdLabel(deviceThreshold);
  }
}

function onReading(temp, hum, fan, threshold) {
  $temp.textContent = temp.toFixed(1);
  $hum.textContent = hum.toFixed(1);
  setFan(fan);
  deviceThreshold = threshold;
  // Sync the slider to the device — but don't yank it while the user is adjusting it.
  if ($slider && document.activeElement !== $slider) {
    $slider.value = threshold;
    setThresholdLabel(threshold);
  }
  lastReading = Date.now();
  $status.textContent = `last reading ${new Date(lastReading).toLocaleTimeString()}`;
}

function onStatus(text) {
  // Once we've had a reading, keep showing when it was rather than clobbering it with
  // raw rpc/connection errors — the greyed-out numbers already signal stale.
  if (lastReading) {
    $status.textContent = `last reading ${new Date(lastReading).toLocaleTimeString()}`;
  } else {
    $status.textContent = text;
  }
}

// Offer "connect" only once the node is up, the field is non-empty, and it differs
// from what we're already polling.
function refreshConnectButton() {
  const t = $ticket.value.trim();
  $connect.disabled = !node || t === "" || t === connectedTicket;
}

function connect() {
  const ticket = $ticket.value.trim();
  if (!node || !ticket || ticket === connectedTicket) return;
  localStorage.setItem(TICKET_KEY, ticket);
  // Switch devices: stop the previous poll loop (and close its connection) first.
  if (current) {
    current.free();
    current = null;
  }
  // Reset the display for the new device.
  lastReading = null;
  deviceThreshold = null;
  $temp.textContent = "—";
  $hum.textContent = "—";
  if ($thresholdLabel) $thresholdLabel.textContent = "—";
  setFan(false);
  onStatus("connecting…");
  current = node.subscribe(ticket, onReading, onStatus);
  connectedTicket = ticket;
  refreshConnectButton();
}

$connect.addEventListener("click", connect);
$ticket.addEventListener("input", refreshConnectButton);
$ticket.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !$connect.disabled) connect();
});

// Fan control wiring only exists when this variant has the secret + slider.
if ($secret && $slider) {
  $secret.addEventListener("input", () => {
    localStorage.setItem(API_SECRET_KEY, $secret.value);
    refreshSlider();
  });

  // Live label while dragging.
  $slider.addEventListener("input", () => setThresholdLabel($slider.value));

  // Commit on release: try to set it; on any refusal or error, snap back.
  $slider.addEventListener("change", async () => {
    const secret = $secret.value.trim();
    if (!secret) return;
    if (!connectedTicket) {
      snapBack();
      $status.textContent = "connect first to set the threshold";
      return;
    }
    const value = Number($slider.value);
    $status.textContent = `setting threshold to ${value}°C…`;
    try {
      const resp = await node.set_threshold(connectedTicket, secret, value, onReading);
      if (resp === "ok") {
        deviceThreshold = value;
        $status.textContent = `threshold set to ${value}°C`;
      } else {
        snapBack();
        $status.textContent =
          resp === "unauthorized" ? "rejected — wrong secret" : "rejected — out of range";
      }
    } catch (err) {
      snapBack();
      $status.textContent = `set failed: ${err}`;
    }
  });
}

// Boot the endpoint.
try {
  // Resolve the wasm relative to THIS page's directory. The page may be served with
  // or without a trailing slash (Vercel with trailingSlash:false canonicalizes to no
  // slash) — a bare `./wasm/…` import would then resolve against the parent dir and
  // 404. Normalize to a directory path, then import from there.
  let dir = location.pathname;
  if (!dir.endsWith("/")) {
    dir = dir.endsWith(".html") ? dir.slice(0, dir.lastIndexOf("/") + 1) : `${dir}/`;
  }
  const { default: init, Node } = await import(`${dir}wasm/smart_fan_wasm.js`);
  await init();
  node = await Node.spawn(localStorage.getItem(SECRET_KEY));
  localStorage.setItem(SECRET_KEY, node.secret_hex());
  $status.textContent = "ready — paste a ticket and connect";
  refreshConnectButton();
  if (urlTicket && $ticket.value) connect();
} catch (err) {
  $status.textContent = `failed to start: ${err}`;
  console.error(err);
}
