// Assemble a self-contained `dist/` from `public/`: inline style.css and main.js
// into index.html, then copy the wasm-bindgen output alongside. The result is a
// drop-in directory (index.html + wasm/) — copy it anywhere and serve it statically.
// No dependencies: plain Node fs.
import { readFileSync, writeFileSync, rmSync, mkdirSync, cpSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pub = join(root, "public");
const dist = join(root, "dist");

const css = readFileSync(join(pub, "style.css"), "utf8");
const js = readFileSync(join(pub, "main.js"), "utf8");
let html = readFileSync(join(pub, "index.html"), "utf8");

// Replace the external references with inline blocks. Use a function replacement so
// `$` sequences in the CSS/JS aren't interpreted as replacement patterns, and fail
// loudly if a marker is missing (so a broken build never ships silently).
function inline(marker, replacement) {
  if (!html.includes(marker)) {
    throw new Error(`bundle: marker not found in index.html: ${marker}`);
  }
  html = html.replace(marker, () => replacement);
}

inline('<link rel="stylesheet" href="./style.css" />', `<style>\n${css}</style>`);
inline('<script src="./main.js" type="module"></script>', `<script type="module">\n${js}</script>`);

rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });
writeFileSync(join(dist, "index.html"), html);
cpSync(join(pub, "wasm"), join(dist, "wasm"), { recursive: true });

console.log("bundled → dist/ (self-contained: index.html + wasm/)");
