const $ticket = document.querySelector("#ticket");
const $connect = document.querySelector("#connect");
const $temp = document.querySelector("#temp");
const $hum = document.querySelector("#hum");
const $fan = document.querySelector("#fan-icon"); // absent in the thermometer variant
const $secret = document.querySelector("#secret"); // absent unless the fan is controllable
const $slider = document.querySelector("#threshold"); // "
const $thresholdLabel = document.querySelector("#threshold-label");
const $tempMarker = document.querySelector("#temp-marker"); // subtle current-temp tick (control variant only)
const $status = document.querySelector("#status");
const $conn = document.querySelector("#conn"); // panel grouping ticket + secret
const $connToggle = document.querySelector("#conn-toggle"); // gear that shows/hides it

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
let NodeClass = null; // the wasm `Node` class, captured at boot so we can re-spawn
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

// The connection group (ticket + secret) is just setup clutter once you have a ticket,
// so it's hidden behind the gear. The gear lights up while the panel is open.
function refreshConn() {
  // Highlight the gear while the setup panel is open.
  if ($connToggle && $conn) $connToggle.classList.toggle("active", !$conn.hidden);
}
// Start collapsed if a ticket is already prefilled; open otherwise so the first thing
// you see is where to paste one. (Not tied to typing, so it never folds mid-edit.)
// Hidden when a ticket is already set (URL fragment or stored); open when blank.
if ($conn) $conn.hidden = $ticket.value.trim() !== "";
refreshConn();

// The flourish: the numbers and the fan share one color driven off `lastReading` —
// mid-blue when fresh, fading to gray over FADE_MS. The fan is blue whether resting or
// spinning; only staleness greys it out. Driving it all from the shared timestamp keeps
// them exactly in sync.
function paintFreshness() {
  const t = lastReading ? Math.min((Date.now() - lastReading) / FADE_MS, 1) : 1;
  const c = FRESH.map((f, i) => Math.round(f + (STALE[i] - f) * t));
  const color = `rgb(${c[0]}, ${c[1]}, ${c[2]})`;
  $temp.style.color = color;
  $hum.style.color = color;
  if ($fan) $fan.style.color = color;
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
  // Mark where the current temperature sits on the threshold track.
  if ($tempMarker && $slider) {
    const min = Number($slider.min);
    const max = Number($slider.max);
    const pct = Math.min(1, Math.max(0, (temp - min) / (max - min)));
    $tempMarker.style.setProperty("--pct", pct);
    $tempMarker.classList.add("visible");
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
  // Fold the setup fields away now that we're connected.
  if ($conn) $conn.hidden = true;
  refreshConn();
}

$connect.addEventListener("click", connect);
$ticket.addEventListener("input", refreshConnectButton);
$ticket.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !$connect.disabled) connect();
});

// Gear toggles the connection setup panel (ticket + secret).
if ($connToggle) {
  $connToggle.addEventListener("click", () => {
    if (!$conn) return;
    $conn.hidden = !$conn.hidden;
    refreshConn();
  });
}

// Fast reconnect after the tab/phone was frozen. While suspended, the endpoint's relay
// and device connections go stale, and iroh would otherwise wait out its reconnect
// backoff — slow. A manual reload is instant because it starts from scratch, so on
// return to the foreground we do the same thing in place: tear the node down and
// re-spawn + resubscribe (keeping the page — chart, fade — intact, unlike a reload).
let hiddenAt = null;
async function reconnectFresh() {
  const ticket = connectedTicket;
  if (!ticket || !NodeClass) return;
  onStatus("reconnecting…");
  if (current) {
    current.free();
    current = null;
  }
  if (node) {
    try {
      node.free();
    } catch (_) {}
    node = null;
  }
  connectedTicket = null; // so connect() proceeds with the same (still-filled) ticket
  try {
    node = await NodeClass.spawn(localStorage.getItem(SECRET_KEY));
    localStorage.setItem(SECRET_KEY, node.secret_hex());
    connect();
  } catch (err) {
    $status.textContent = `reconnect failed: ${err}`;
    console.error(err);
  }
}

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "hidden") {
    hiddenAt = Date.now();
    return;
  }
  const staleFor = hiddenAt == null ? 0 : Date.now() - hiddenAt;
  hiddenAt = null;
  // Only rebuild if we were connected and away long enough to have gone stale — a quick
  // tab flip doesn't need it (and would waste a history backfill).
  if (connectedTicket && staleFor > 5000) reconnectFresh();
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
  NodeClass = Node;
  node = await Node.spawn(localStorage.getItem(SECRET_KEY));
  localStorage.setItem(SECRET_KEY, node.secret_hex());
  $status.textContent = "ready — paste a ticket and connect";
  refreshConnectButton();
  if (urlTicket && $ticket.value) connect();
} catch (err) {
  $status.textContent = `failed to start: ${err}`;
  console.error(err);
}
