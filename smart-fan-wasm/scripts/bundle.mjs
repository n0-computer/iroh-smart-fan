// Assemble self-contained bundles from `variants/*.html` + the shared `public/`
// front-end (style.css, main.js) and wasm-bindgen output. Each variant becomes a
// drop-in directory `dist/<name>/` — index.html with CSS+JS inlined, plus its own
// copy of the `wasm/` glue. Copy any of them anywhere and serve statically.
// No dependencies: plain Node fs.
import { readFileSync, writeFileSync, rmSync, mkdirSync, cpSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pub = join(root, "public");
const variantsDir = join(root, "variants");
const dist = join(root, "dist");

const css = readFileSync(join(pub, "style.css"), "utf8");
const js = readFileSync(join(pub, "main.js"), "utf8");

// Function replacement so `$` in the CSS/JS isn't treated as a replacement pattern;
// throw if a marker is missing so a broken build never ships silently.
function inline(html, marker, replacement) {
  if (!html.includes(marker)) throw new Error(`bundle: marker not found: ${marker}`);
  return html.replace(marker, () => replacement);
}

rmSync(dist, { recursive: true, force: true });

const variants = readdirSync(variantsDir).filter((f) => f.endsWith(".html"));
for (const file of variants) {
  const name = file.replace(/\.html$/, "");
  let html = readFileSync(join(variantsDir, file), "utf8");
  html = inline(html, '<link rel="stylesheet" href="./style.css" />', `<style>\n${css}</style>`);
  html = inline(html, '<script src="./main.js" type="module"></script>', `<script type="module">\n${js}</script>`);
  const out = join(dist, name);
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, "index.html"), html);
  // Only the JS glue + .wasm binary are needed at runtime; skip the .d.ts type
  // declarations (they're for TS tooling, never fetched by the browser).
  cpSync(join(pub, "wasm"), join(out, "wasm"), {
    recursive: true,
    filter: (src) => !src.endsWith(".d.ts"),
  });
  console.log(`bundled → dist/${name}/`);
}
