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

// Fresh readings are mid-blue and fade to a readable gray over ~30s (see markFresh).
const FRESH = "#2b6cff";
const STALE = "#8a8f98";

let node = null;
let current = null; // active Subscription handle
let connectedTicket = null; // the ticket `current` is polling
let lastReading = null;

// Prefill the ticket: ?ticket=… on the URL wins (and auto-connects below);
// otherwise fall back to the last ticket we stored.
const urlTicket = new URLSearchParams(location.search).get("ticket");
$ticket.value = (urlTicket ?? localStorage.getItem(TICKET_KEY) ?? "").trim();

// The flourish: a fresh reading snaps to mid-blue then transitions to gray over
// ~30s. Each reading restarts it, so a live link stays blue while a stalled one
// greys out.
function markFresh(el) {
  el.style.transition = "none";
  el.style.color = FRESH;
  void el.offsetWidth; // force a reflow so the snap lands before the transition
  el.style.transition = "color 30s linear";
  el.style.color = STALE;
}

function onReading(temp, hum) {
  $temp.textContent = temp.toFixed(1);
  $hum.textContent = hum.toFixed(1);
  markFresh($temp);
  markFresh($hum);
  lastReading = new Date();
  $status.textContent = `last reading ${lastReading.toLocaleTimeString()}`;
}

function onStatus(text) {
  // Once we've had a reading, keep showing when it was rather than clobbering it
  // with raw rpc/connection errors — the greyed-out numbers already signal stale.
  if (lastReading) {
    $status.textContent = `last reading ${lastReading.toLocaleTimeString()}`;
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
  // Reset the display for the new device.
  lastReading = null;
  for (const el of [$temp, $hum]) {
    el.textContent = "—";
    el.style.transition = "none";
    el.style.color = STALE;
  }
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
