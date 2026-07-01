import init, { Node } from "./wasm/smart_fan_wasm.js";

const $ticket = document.querySelector("#ticket");
const $connect = document.querySelector("#connect");
const $temp = document.querySelector("#temp");
const $hum = document.querySelector("#hum");
const $status = document.querySelector("#status");

// Persist our secret key so the browser keeps a stable endpoint id across reloads.
const SECRET_KEY = "smart-fan:secret";
// Persist the last ticket so a reload reconnects without re-pasting.
const TICKET_KEY = "smart-fan:ticket";

// Fresh readings are mid-blue and fade to a readable gray over FADE_MS.
const FRESH = [43, 108, 255]; // #2b6cff
const STALE = [138, 143, 152]; // #8a8f98
const FADE_MS = 30_000;

let node = null;
let current = null; // active Subscription handle
let connectedTicket = null; // the ticket `current` is polling
let lastReading = null;

// Prefill the ticket: ?ticket=… on the URL wins (and auto-connects below);
// otherwise fall back to the last ticket we stored.
const urlTicket = new URLSearchParams(location.search).get("ticket");
$ticket.value = (urlTicket ?? localStorage.getItem(TICKET_KEY) ?? "").trim();

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

function onReading(temp, hum) {
  $temp.textContent = temp.toFixed(1);
  $hum.textContent = hum.toFixed(1);
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
  $temp.textContent = "—";
  $hum.textContent = "—";
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
