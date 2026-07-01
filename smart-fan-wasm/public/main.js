import init, { Node } from "./wasm/smart_fan_wasm.js";

const $ticket = document.querySelector("#ticket");
const $connect = document.querySelector("#connect");
const $temp = document.querySelector("#temp");
const $hum = document.querySelector("#hum");
const $fan = document.querySelector("#fan-icon");
const $secret = document.querySelector("#secret");
const $slider = document.querySelector("#threshold");
const $thresholdLabel = document.querySelector("#threshold-label");
const $status = document.querySelector("#status");

// Persist our secret key so the browser keeps a stable endpoint id across reloads.
const SECRET_KEY = "smart-fan:secret";
// Persist the last ticket so a reload reconnects without re-pasting.
const TICKET_KEY = "smart-fan:ticket";
// Persist the API secret so control survives reloads (it's a bearer token, so this
// is the same trust level as leaving yourself logged in).
const API_SECRET_KEY = "smart-fan:api-secret";

// Fresh readings are mid-blue and fade to a readable gray over FADE_MS.
const FRESH = [43, 108, 255]; // #2b6cff
const STALE = [138, 143, 152]; // #8a8f98
const FADE_MS = 30_000;

let node = null;
let current = null; // active Subscription handle
let connectedTicket = null; // the ticket `current` is polling
let lastReading = null;
let deviceThreshold = null; // the threshold the device last reported

// Prefill the ticket: ?ticket=… on the URL wins (and auto-connects below);
// otherwise fall back to the last ticket we stored.
const urlTicket = new URLSearchParams(location.search).get("ticket");
$ticket.value = (urlTicket ?? localStorage.getItem(TICKET_KEY) ?? "").trim();

// Restore the API secret; the slider is only enabled while a secret is present.
$secret.value = localStorage.getItem(API_SECRET_KEY) ?? "";
refreshSlider();

// The flourish: both numbers share one color driven off `lastReading` — mid-blue
// when fresh, fading to gray over FADE_MS. Driving it from the shared timestamp
// (rather than per-element CSS transitions) keeps temperature and humidity exactly
// in sync, so one can't be stuck blue while the other is gray.
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
  $fan.classList.toggle("spinning", on);
  $fan.setAttribute("aria-label", on ? "fan on" : "fan off");
}

// The slider is greyed out unless an API secret is present.
function refreshSlider() {
  $slider.disabled = $secret.value.trim() === "";
}

function setThresholdLabel(v) {
  $thresholdLabel.textContent = `${Math.round(v)}°C`;
}

// Snap the slider back to the device's actual threshold (used when a set is refused).
function snapBack() {
  if (deviceThreshold != null) {
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
  if (document.activeElement !== $slider) {
    $slider.value = threshold;
    setThresholdLabel(threshold);
  }
  lastReading = Date.now();
  $status.textContent = `last reading ${new Date(lastReading).toLocaleTimeString()}`;
}

function onStatus(text) {
  // Once we've had a reading, keep showing when it was rather than clobbering it
  // with raw rpc/connection errors — the greyed-out numbers already signal stale.
  if (lastReading) {
    $status.textContent = `last reading ${new Date(lastReading).toLocaleTimeString()}`;
  } else {
    $status.textContent = text;
  }
}

// Offer "connect" only once the node is up, the field is non-empty, and it differs
// from what we're already polling — so it greys out while you're on that ticket and
// lights up the moment you edit it to point at another thermometer.
function refreshConnectButton() {
  const t = $ticket.value.trim();
  $connect.disabled = !node || t === "" || t === connectedTicket;
}

function connect() {
  const ticket = $ticket.value.trim();
  if (!node || !ticket || ticket === connectedTicket) return;
  localStorage.setItem(TICKET_KEY, ticket);
  // Switch devices: stop the previous poll loop (and close its connection) first,
  // otherwise we'd poll two thermometers and the display would flip-flop.
  if (current) {
    current.free();
    current = null;
  }
  // Reset the display for the new device (paintFreshness greys it out via lastReading).
  lastReading = null;
  deviceThreshold = null;
  $temp.textContent = "—";
  $hum.textContent = "—";
  $thresholdLabel.textContent = "—";
  setFan(false);
  onStatus("connecting…");
  current = node.subscribe(ticket, onReading, onStatus);
  connectedTicket = ticket;
  refreshConnectButton();
}

$connect.addEventListener("click", connect);
$ticket.addEventListener("input", refreshConnectButton);
// Enter in the ticket field connects.
$ticket.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !$connect.disabled) connect();
});

// Secret drives whether the slider is usable, and is persisted.
$secret.addEventListener("input", () => {
  localStorage.setItem(API_SECRET_KEY, $secret.value);
  refreshSlider();
});

// Live label while dragging.
$slider.addEventListener("input", () => setThresholdLabel($slider.value));

// Commit on release: try to set it; on any refusal or error, snap back to the
// device's actual threshold. (Status is written directly since onStatus prefers the
// "last reading" line once a reading has arrived.)
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
    const resp = await node.set_threshold(connectedTicket, secret, value);
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

// Boot the endpoint.
try {
  await init();
  node = await Node.spawn(localStorage.getItem(SECRET_KEY));
  localStorage.setItem(SECRET_KEY, node.secret_hex());
  $status.textContent = "ready — paste a ticket and connect";
  refreshConnectButton();
  // A ticket passed on the URL connects immediately.
  if (urlTicket && $ticket.value) connect();
} catch (err) {
  $status.textContent = `failed to start: ${err}`;
  console.error(err);
}
