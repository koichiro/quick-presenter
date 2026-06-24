import { cp, mkdir, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const repoRoot = dirname(root);
const dist = join(root, "dist");

const files = [
  ["src/index.html", "index.html"],
  ["src/styles.css", "styles.css"],
  ["../docs/assets/presenter-window-en.png", "assets/presenter-window-en.png"],
  ["../docs/assets/slide-window-title-en.png", "assets/slide-window-title-en.png"],
  ["../docs/assets/slide-window-en.png", "assets/slide-window-en.png"],
  ["../docs/assets/quick-presenter-demo.gif", "assets/quick-presenter-demo.gif"],
  ["../assets/icons/png/quick-presenter-icon-256.png", "assets/quick-presenter-icon-256.png"],
  ["src/assets/mac-app-store-badge.svg", "assets/mac-app-store-badge.svg"],
  ["src/assets/microsoft-store-badge.svg", "assets/microsoft-store-badge.svg"],
];

await rm(dist, { recursive: true, force: true });

for (const [source, target] of files) {
  const outputPath = join(dist, target);
  await mkdir(dirname(outputPath), { recursive: true });
  await cp(join(root, source), outputPath);
}

console.log(`Built website to ${dist.replace(`${repoRoot}/`, "")}`);
