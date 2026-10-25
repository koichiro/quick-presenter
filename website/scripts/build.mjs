import { cp, mkdir, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const repoRoot = dirname(root);
const dist = join(root, "dist");

const files = [
  ["src/guides/interactive-ai/index.html", "guides/interactive-ai/index.html"],
  ["src/ja/guides/interactive-ai/index.html", "ja/guides/interactive-ai/index.html"],
  ["src/assets/interactive-ai-short.mp4", "assets/interactive-ai-short.mp4"],
  ["src/assets/interactive-ai-poster.jpg", "assets/interactive-ai-poster.jpg"],
  ["src/assets/interactive-ai-sample.pdf", "assets/interactive-ai-sample.pdf"],
  ["src/assets/autopilot-sample.pdf", "assets/autopilot-sample.pdf"],
  ["src/assets/agentic-presentation-demo.mp4", "assets/agentic-presentation-demo.mp4"],
  ["src/assets/agentic-presentation-poster.jpg", "assets/agentic-presentation-poster.jpg"],
  ["src/assets/kokoro-demo.mp4", "assets/kokoro-demo.mp4"],
  ["src/assets/kokoro-demo-poster.jpg", "assets/kokoro-demo-poster.jpg"],
  ["src/index.html", "index.html"],
  ["src/ja/index.html", "ja/index.html"],
  ["src/ja/privacy/index.html", "ja/privacy/index.html"],
  ["src/privacy/index.html", "privacy/index.html"],
  ["src/styles.css", "styles.css"],
  ["src/language.js", "language.js"],
  ["src/assets/presenter-window-v2-macos.png", "assets/presenter-window-v2-macos.png"],
  ["../docs/assets/audience-window-title-en-macos.png", "assets/audience-window-title-en.png"],
  ["../docs/assets/audience-window-en-macos.png", "assets/audience-window-en.png"],
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
