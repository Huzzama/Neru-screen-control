// Copies the bundled fonts (../assets/fonts) into public/fonts so the editor
// preview uses exactly the fonts the renderer uses.
import { copyFileSync, existsSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const src = join(here, "..", "..", "assets", "fonts");
const dst = join(here, "..", "public", "fonts");

if (!existsSync(src)) {
  console.warn(`[copy-fonts] ${src} not found; skipping`);
  process.exit(0);
}
mkdirSync(dst, { recursive: true });
let n = 0;
for (const f of readdirSync(src)) {
  if (f.toLowerCase().endsWith(".ttf")) {
    copyFileSync(join(src, f), join(dst, f));
    n++;
  }
}
console.log(`[copy-fonts] ${n} fonts → public/fonts`);
