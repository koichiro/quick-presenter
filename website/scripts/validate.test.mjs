import assert from "node:assert/strict";
import { readFile, readdir, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const dist = join(root, "dist");
const origin = "https://quickpresenter.com";
const oldOrigin = "https://koichiro.github.io/quick-presenter/";
const pages = [
  ["index.html", "/"],
  ["privacy/index.html", "/privacy/"],
];

function attributes(tag) {
  return Object.fromEntries(
    [...tag.matchAll(/([\w:-]+)\s*=\s*["']([^"']*)["']/g)].map(
      ([, name, value]) => [name, value],
    ),
  );
}

function outputPath(url) {
  const path = decodeURIComponent(url.pathname);
  return join(dist, path.endsWith("/") ? `${path}index.html` : path);
}

test("generated pages have canonical and Open Graph URLs on the new domain", async () => {
  for (const [file, pathname] of pages) {
    const html = await readFile(join(dist, file), "utf8");
    const tags = [...html.matchAll(/<(?:link|meta)\b[^>]*>/g)].map(
      ([tag]) => attributes(tag),
    );
    assert.deepEqual(
      tags.filter((tag) => tag.rel === "canonical").map((tag) => tag.href),
      [`${origin}${pathname}`],
    );
    if (pathname === "/") {
      assert.equal(tags.find((tag) => tag.property === "og:url")?.content, `${origin}/`);
      assert.equal(
        tags.find((tag) => tag.property === "og:image")?.content,
        `${origin}/assets/presenter-window-en.png`,
      );
    }
    for (const tag of tags.filter((tag) => ["og:url", "og:image"].includes(tag.property))) {
      assert.equal(new URL(tag.content).origin, origin);
      assert.ok((await stat(outputPath(new URL(tag.content)))).isFile());
    }
  }
});

test("required stylesheet and image assets are generated and nonempty", async () => {
  for (const file of [
    "styles.css",
    "assets/presenter-window-en.png",
    "assets/slide-window-title-en.png",
    "assets/slide-window-en.png",
    "assets/quick-presenter-demo.gif",
    "assets/quick-presenter-icon-256.png",
    "assets/mac-app-store-badge.svg",
    "assets/microsoft-store-badge.svg",
  ]) {
    const info = await stat(join(dist, file));
    assert.ok(info.isFile() && info.size > 0, file);
  }
});

test("internal navigation, fragments, and assets resolve at the domain root", async () => {
  for (const [file, pathname] of pages) {
    const html = await readFile(join(dist, file), "utf8");
    assert.doesNotMatch(html, /<base\b/i);
    const resolved = [];
    for (const [tag] of html.matchAll(/<[a-z][^>]*>/gi)) {
      const attrs = attributes(tag);
      for (const ref of [attrs.href, attrs.src].filter(Boolean)) {
        const url = new URL(ref, `${origin}${pathname}`);
        if (url.origin !== origin) continue;
        resolved.push(url.pathname);
        const target = outputPath(url);
        assert.ok((await stat(target)).isFile(), `${file}: ${ref}`);
        if (url.hash) {
          const targetHtml = await readFile(target, "utf8");
          const ids = [...targetHtml.matchAll(/\bid=["']([^"']+)["']/g)].map(
            ([, id]) => id,
          );
          assert.ok(ids.includes(decodeURIComponent(url.hash.slice(1))), `${file}: ${ref}`);
        }
      }
    }
    assert.ok(resolved.includes(pathname === "/" ? "/privacy/" : "/"));
    assert.ok(resolved.includes("/styles.css"));
  }
  const css = await readFile(join(dist, "styles.css"), "utf8");
  for (const [, ref] of css.matchAll(/url\(\s*["']?([^\s"')]+)["']?\s*\)/g)) {
    const url = new URL(ref, `${origin}/styles.css`);
    if (url.origin === origin) assert.ok((await stat(outputPath(url))).isFile(), ref);
  }
});

test("published output and current public documentation do not use the old URL", async () => {
  async function checkDirectory(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await checkDirectory(path);
      else if (/\.(html|css|svg)$/.test(entry.name)) {
        assert.ok(!(await readFile(path, "utf8")).includes(oldOrigin), path);
      }
    }
  }
  await checkDirectory(dist);
  await checkDirectory(join(root, "src"));
  for (const file of ["README.md", "docs/PACKAGING.md"]) {
    assert.ok(!(await readFile(join(root, "..", file), "utf8")).includes(oldOrigin), file);
  }
});
