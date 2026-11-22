import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, readdir, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { runInNewContext } from "node:vm";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createInterface } from "node:readline";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const dist = join(root, "dist");
const origin = "https://quickpresenter.com";
const oldOrigin = "https://koichiro.github.io/quick-presenter";
const pages = [
  ["audience-live/index.html", "/audience-live/"],
  ["ja/audience-live/index.html", "/ja/audience-live/"],
  ["index.html", "/"],
  ["privacy/index.html", "/privacy/"],
  ["ja/index.html", "/ja/"],
  ["ja/privacy/index.html", "/ja/privacy/"],
  ["guides/interactive-ai/index.html", "/guides/interactive-ai/"],
  ["ja/guides/interactive-ai/index.html", "/ja/guides/interactive-ai/"],
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
    if (pathname === "/" || pathname === "/ja/") {
      assert.equal(tags.find((tag) => tag.property === "og:url")?.content, `${origin}${pathname}`);
      assert.equal(
        tags.find((tag) => tag.property === "og:image")?.content,
        `${origin}/assets/presenter-window-v2-macos.png`,
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
    "assets/interactive-ai-short.mp4",
    "assets/interactive-ai-poster.jpg",
    "assets/interactive-ai-sample.pdf",
    "assets/autopilot-sample.pdf",
    "assets/agentic-presentation-demo.mp4",
    "assets/agentic-presentation-poster.jpg",
    "assets/kokoro-demo.mp4",
    "assets/kokoro-demo-poster.jpg",
    "styles.css",
    "audience-live.css",
    "audience-live.js",
    "assets/audience-live/flowing.gif",
    "assets/audience-live/bubbles.gif",
    "assets/audience-live/flowing.jpg",
    "assets/audience-live/bubbles.jpg",
    "assets/presenter-window-v2-macos.png",
    "assets/audience-window-title-en.png",
    "assets/audience-window-en.png",
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
      for (const ref of [attrs.href, attrs.src, attrs.poster].filter(Boolean)) {
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
    const localeRoot = pathname.startsWith("/ja/") ? "/ja/" : "/";
    assert.ok(resolved.includes(pathname.endsWith("privacy/") ? localeRoot : `${localeRoot}privacy/`));
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


test("preview serves video metadata and seek ranges", { timeout: 10000 }, async () => {
  const child = spawn(process.execPath, [join(root, "scripts/preview.mjs")], {
    env: { ...process.env, HOST: "127.0.0.1", PORT: "0" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const lines = createInterface({ input: child.stdout });
  try {
    const [line] = await once(lines, "line");
    const base = line.match(/http:\/\/127\.0\.0\.1:\d+\//)?.[0];
    assert.ok(base, line);
    for (const asset of ["assets/kokoro-demo.mp4", "assets/agentic-presentation-demo.mp4", "assets/interactive-ai-short.mp4"]) {
      const url = `${base}${asset}`;
      const info = await stat(join(dist, asset));
      const head = await fetch(url, { method: "HEAD" });
      assert.equal(head.status, 200);
      assert.equal(head.headers.get("content-type"), "video/mp4");
      assert.equal(Number(head.headers.get("content-length")), info.size);
      const first = await fetch(url, { headers: { Range: "bytes=0-31" } });
      assert.equal(first.status, 206);
      assert.equal(first.headers.get("content-range"), `bytes 0-31/${info.size}`);
      assert.equal((await first.arrayBuffer()).byteLength, 32);
      const suffix = await fetch(url, { headers: { Range: "bytes=-16" } });
      assert.equal(suffix.status, 206);
      assert.equal(suffix.headers.get("content-range"), `bytes ${info.size - 16}-${info.size - 1}/${info.size}`);
      assert.equal((await suffix.arrayBuffer()).byteLength, 16);
      const invalid = await fetch(url, { headers: { Range: `bytes=${info.size}-` } });
      assert.equal(invalid.status, 416);
      assert.equal(invalid.headers.get("content-range"), `bytes */${info.size}`);
      await invalid.arrayBuffer();
    }
  } finally {
    lines.close();
    const exited = once(child, "exit");
    child.kill();
    await exited;
  }
});


test("localized pages declare their language and offer reciprocal language navigation", async () => {
  for (const [file, pathname] of pages) {
    const html = await readFile(join(dist, file), "utf8");
    const japanese = pathname.startsWith("/ja/");
    assert.match(html, new RegExp(`<html lang="${japanese ? "ja" : "en"}">`));
    const englishPath = japanese ? pathname.slice(3) : pathname;
    const japanesePath = `/ja${englishPath}`;
    const alternateTags = [...html.matchAll(/<link\b[^>]*>/g)]
      .map(([tag]) => attributes(tag)).filter((tag) => tag.rel === "alternate");
    assert.deepEqual(Object.fromEntries(alternateTags.map((tag) => [tag.hreflang, tag.href])), {
      en: `${origin}${englishPath}`,
      ja: `${origin}${japanesePath}`,
      "x-default": `${origin}${englishPath}`,
    });
    const switchHtml = html.match(/<span class="language-switch"[^>]*>([\s\S]*?)<\/span>/)?.[1];
    assert.ok(switchHtml, file);
    const links = [...switchHtml.matchAll(/<a\b[^>]*>/g)].map(([tag]) => attributes(tag));
    assert.deepEqual(links.map((link) => link.href), [englishPath, japanesePath]);
    assert.deepEqual(links.filter((link) => link["aria-current"] === "page").map((link) => link.href), [pathname]);
  }
});


test("language routing remembers choices and preserves destinations without redirect loops", async () => {
  const script = await readFile(join(dist, "language.js"), "utf8");
  function visit({ path = "/", languages = ["ja-JP", "en"], saved = null, blocked = false, search = "?from=demo", hash = "#download" } = {}) {
    let redirect;
    let click;
    let stored = saved;
    runInNewContext(script, {
      URL, URLSearchParams,
      document: {
        documentElement: { lang: path.startsWith("/ja/") ? "ja" : "en" },
        addEventListener: (name, handler) => { if (name === "click") click = handler; },
      },
      navigator: { languages, language: "ja-JP" },
      location: {
        pathname: path, search, hash, href: `${origin}${path}${search}${hash}`,
        replace: (url) => { redirect = url; },
      },
      localStorage: {
        getItem: () => { if (blocked) throw Error("Blocked"); return stored; },
        setItem: (key, value) => { if (blocked) throw Error("Blocked"); stored = value; },
      },
    });
    return { redirect, stored: () => stored, click };
  }
  assert.equal(visit().redirect, "/ja/?from=demo#download");
  assert.equal(visit({ path: "/audience-live/" }).redirect, "/ja/audience-live/?from=demo#download");
  assert.equal(visit({ path: "/ja/audience-live/", saved: "en" }).redirect, "/audience-live/?from=demo#download");
  assert.equal(visit({ path: "/privacy/" }).redirect, "/ja/privacy/?from=demo#download");
  assert.equal(visit({ languages: ["en-US", "ja"] }).redirect, undefined);
  assert.equal(visit({ languages: ["fr-FR"] }).stored(), "en");
  assert.equal(visit({ languages: [] }).stored(), "ja");
  assert.equal(visit({ path: "/ja/", languages: ["en-US"] }).redirect, undefined);
  assert.equal(visit({ saved: "en" }).redirect, undefined);
  assert.equal(visit({ saved: "ja", languages: ["en-US"] }).redirect, "/ja/?from=demo#download");
  assert.equal(visit({ path: "/ja/privacy/", saved: "en" }).redirect, "/privacy/?from=demo#download");
  assert.equal(visit({ saved: "invalid" }).stored(), "ja");
  const manual = visit();
  manual.click({ target: { closest: () => ({ hreflang: "en" }) } });
  assert.equal(manual.stored(), "en");
  assert.equal(visit({ saved: manual.stored() }).redirect, undefined);
  const blocked = visit({ blocked: true });
  assert.equal(blocked.redirect, "/ja/?from=demo#download");
  const englishLink = { hreflang: "en", href: "/?from=manual#demo" };
  blocked.click({ target: { closest: () => englishLink } });
  const destination = new URL(englishLink.href);
  assert.equal(destination.searchParams.get("lang"), "en");
  assert.equal(destination.searchParams.get("from"), "manual");
  assert.equal(destination.hash, "#demo");
  assert.equal(visit({ path: destination.pathname, search: destination.search,
    hash: destination.hash, blocked: true }).redirect, undefined);
  assert.equal(visit({ path: "/ja/", search: "?lang=ja", saved: "en" }).redirect, undefined);
  assert.equal(visit({ search: "?lang=invalid" }).redirect, "/ja/?lang=invalid#download");
  assert.equal(visit({ path: "/ja/", blocked: true }).redirect, undefined);
  for (const [file] of pages) {
    assert.match(await readFile(join(dist, file), "utf8"), /<script src="\/language\.js"><\/script>/);
  }
});


test("preview survives a range stream read failure", { timeout: 10000 }, async () => {
  // Simulate a file disappearing between stat and the range stream opening it.
  const bootstrap = `
    import fs from 'node:fs';
    import { syncBuiltinESMExports } from 'node:module';
    import { Readable } from 'node:stream';
    const original = fs.createReadStream;
    fs.createReadStream = (path, options) => options?.start === 1 && options?.end === 2
      ? new Readable({ read() { this.destroy(new Error('Simulated read failure')); } })
      : original(path, options);
    syncBuiltinESMExports();
    await import(${JSON.stringify(new URL('./preview.mjs', import.meta.url).href)});
  `;
  const child = spawn(process.execPath, ["--input-type=module", "-e", bootstrap], {
    env: { ...process.env, HOST: "127.0.0.1", PORT: "0" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const lines = createInterface({ input: child.stdout });
  try {
    const [line] = await once(lines, "line");
    const base = line.match(/http:\/\/127\.0\.0\.1:\d+\//)?.[0];
    assert.ok(base, line);
    await assert.rejects(async () => {
      const response = await fetch(`${base}assets/kokoro-demo.mp4`, {
        headers: { Range: "bytes=1-2" },
      });
      await response.arrayBuffer();
    });
    const healthy = await fetch(base);
    assert.equal(healthy.status, 200);
    await healthy.arrayBuffer();
    assert.equal(child.exitCode, null);
  } finally {
    lines.close();
    if (child.exitCode === null) {
      const exited = once(child, "exit");
      child.kill();
      await exited;
    }
  }
});


test("Audience Live media retain PR #561 byte identity and provide static no-JS fallbacks", async () => {
  const hashes = {
    "flowing.gif": "695ba41579bd16913d61aff11b4e408f8678aeaacf9e93cfc9dc41ec11979e33",
    "bubbles.gif": "16b4c084a7629a39897b3a741dab2f3488d7a5d2576f44e79ca50fcba5f96a1f",
    "flowing.jpg": "821924546f27baf70eecef13f25495040a83645d16cacd59ab8de025a160c654",
    "bubbles.jpg": "fbc38773724bb299149bd38d4b09b94f2f6f9f9f9f1f22de4288b935705d37ee",
  };
  for (const [file, hash] of Object.entries(hashes)) {
    const data = await readFile(join(dist, "assets/audience-live", file));
    assert.equal(createHash("sha256").update(data).digest("hex"), hash, file);
  }
  for (const [file] of pages.filter(([, path]) => path.endsWith("audience-live/"))) {
    const html = await readFile(join(dist, file), "utf8");
    const images = [...html.matchAll(/<img\b[^>]*>/g)].map(([tag]) => attributes(tag));
    const demos = images.filter(image => image["data-animation"]);
    assert.equal(demos.length, 3);
    for (const demo of demos) {
      assert.ok(demo.src.endsWith(".jpg"), "No-JS content must remain static");
      assert.ok((await stat(outputPath(new URL(demo["data-poster"], origin)))).isFile());
      assert.ok(demo.alt.length > 0);
      assert.ok((await stat(outputPath(new URL(demo["data-animation"], origin)))).isFile());
    }
    const controls = [...html.matchAll(/<button\b[^>]*data-playback[^>]*>/g)]
      .map(([tag]) => attributes(tag));
    assert.equal(controls.length, demos.length);
    for (const [i, control] of controls.entries()) {
      assert.equal(control["aria-controls"], demos[i].id);
      assert.ok(control["aria-describedby"]);
    }
    assert.doesNotMatch(html, /<img[^>]*\bsrc=["'][^"']*\.gif/);
    assert.doesNotMatch(html, /<[^>]+\sautoplay(?:[\s=>])/);
    assert.equal((html.match(/<div id="panel-[^>]*data-panel>/g) || []).length, 2,
      "Both demos remain available when JavaScript is disabled");
  }
});

// Exercise the browser controller without adding a DOM dependency.
async function demoController({ reduced = false, observe = true, deferred = false, decodeFailures = 0 } = {}) {
  const script = await readFile(join(dist, 'audience-live.js'), 'utf8');
  function element(dataset = {}) {
    return {
      dataset, attrs: {}, events: {}, hidden: false,
      setAttribute(name, value) { this.attrs[name] = value; },
      getAttribute(name) { return this.attrs[name]; },
      addEventListener(name, callback) { this.events[name] = callback; },
      focus() { this.focused = true; },
    };
  }
  const panels = [element(), element()];
  const demos = ['hero', 'flowing', 'bubble'].map((name, i) => {
    const demo = element({ pause: 'Pause', play: 'Play', reduced: 'Still image' });
    demo.image = element({ animation: `${name}.gif`, poster: `${name}.jpg` });
    demo.image.attrs.src = `${name}.jpg`;
    Object.defineProperty(demo.image, 'src', {
      set(value) { this.attrs.src = value; }, get() { return this.attrs.src; },
    });
    demo.button = element();
    demo.querySelector = selector => selector === 'img' ? demo.image : demo.button;
    demo.closest = () => i ? panels[i - 1] : null;
    return demo;
  });
  const tabs = panels.map((_, i) => element({ tab: `panel-${i}` }));
  const tablist = element();
  tablist.querySelectorAll = () => tabs;
  const comparison = {
    querySelector: () => tablist,
    querySelectorAll: () => panels,
  };
  const document = element();
  document.querySelectorAll = selector => selector === '[data-demo]' ? demos : [comparison];
  const motion = element();
  motion.matches = reduced;
  let observerCallback;
  class Observer {
    constructor(callback) { observerCallback = callback; }
    observe() {}
  }
  const window = { matchMedia: () => motion };
  if (observe) window.IntersectionObserver = Observer;
  const revoked = [];
  class Image {
    async decode() {
      if (decodeFailures > 0) {
        decodeFailures--;
        throw new Error('Invalid image data');
      }
    }
  }
  const requests = [];
  const pending = new Map();
  const response = (source, ok = true) => ({ ok, blob: async () => ({ source }) });
  const fetch = source => {
    requests.push(source);
    return deferred ? new Promise(resolve => pending.set(source, resolve))
      : Promise.resolve(response(source));
  };
  const settle = () => new Promise(resolve => setImmediate(resolve));
  runInNewContext(script, {
    window, document, IntersectionObserver: Observer, fetch, Image,
    URL: { createObjectURL: blob => `blob:${blob.source}`, revokeObjectURL: source => revoked.push(source) },
  });
  await settle();
  return { demos, tabs, panels, document, requests, revoked, settle,
    complete(source, ok = true) { pending.get(source)(response(source, ok)); },
    visible(index, value = true) { observerCallback([{ target: demos[index], isIntersecting: value }]); },
    reduced(value) { motion.matches = value; motion.events.change(); },
  };
}

test('demo controls autoplay only visible active demos and retain individual pause choices', async () => {
  const c = await demoController();
  const [hero, flowing, bubble] = c.demos;
  const source = demo => demo.image.src.replace('blob:', '');
  assert.deepEqual(c.demos.map(source), ['hero.jpg', 'flowing.jpg', 'bubble.jpg']);
  c.visible(0);
  c.visible(1);
  c.visible(2); // A hidden panel must remain static even with a stale observation.
  await c.settle();
  assert.deepEqual(c.demos.map(source), ['hero.gif', 'flowing.gif', 'bubble.jpg']);
  hero.button.events.click();
  assert.equal(source(hero), 'hero.jpg');
  assert.equal(hero.button.textContent, 'Play');
  flowing.button.events.click();
  c.tabs[1].events.click();
  await c.settle();
  assert.equal(source(bubble), 'bubble.gif');
  c.tabs[0].events.click();
  assert.equal(source(flowing), 'flowing.jpg');
  assert.equal(flowing.button.textContent, 'Play');
  flowing.button.events.click();
  await c.settle();
  assert.equal(source(flowing), 'flowing.gif');
  c.visible(1, false);
  assert.equal(source(flowing), 'flowing.jpg');
  c.visible(1);
  await c.settle();
  assert.equal(source(flowing), 'flowing.gif');
  c.document.hidden = true;
  c.document.events.visibilitychange();
  assert.equal(source(flowing), 'flowing.jpg');
  c.document.hidden = false;
  c.document.events.visibilitychange();
  await c.settle();
  assert.equal(source(flowing), 'flowing.gif');
  assert.equal(source(hero), 'hero.jpg');
  let prevented = false;
  c.tabs[0].events.keydown({ key: 'ArrowRight', preventDefault() { prevented = true; } });
  assert.equal(prevented, true);
  assert.equal(c.tabs[1].focused, true);
  assert.equal(c.tabs[1].attrs['aria-selected'], 'true');
  assert.equal(c.panels[0].hidden, true);
});

test('reduced motion prevents GIF loading and preserves pause intent across preference changes', async () => {
  const c = await demoController({ reduced: true });
  c.demos.forEach((_, i) => c.visible(i));
  c.tabs[1].events.click();
  for (const demo of c.demos) {
    assert.ok(demo.image.src.endsWith('.jpg'));
    assert.equal(demo.button.disabled, true);
    assert.equal(demo.button.textContent, 'Still image');
  }
  c.reduced(false);
  await c.settle();
  assert.equal(c.demos[2].image.src, 'blob:bubble.gif');
  c.demos[2].button.events.click();
  c.reduced(true);
  c.reduced(false);
  assert.equal(c.demos[2].image.src, 'bubble.jpg');
  assert.equal(c.demos[2].button.textContent, 'Play');
  const fallback = await demoController({ observe: false });
  assert.equal(fallback.demos[0].image.src, 'blob:hero.gif');
  assert.equal(fallback.demos[1].image.src, 'blob:flowing.gif');
  assert.equal(fallback.demos[2].image.src, 'bubble.jpg');
});

test('GIF fetches are shared and reused across pause, tab and viewport changes', async () => {
  const c = await demoController();
  const [hero, flowing] = c.demos;
  // The real Hero and Flowing images use the same asset URL.
  flowing.image.dataset.animation = hero.image.dataset.animation;
  c.visible(0);
  c.visible(1);
  await c.settle();
  assert.deepEqual(c.requests, ['hero.gif']);
  for (let i = 0; i < 3; i++) {
    hero.button.events.click();
    hero.button.events.click();
    c.visible(0, false);
    c.visible(0);
    c.tabs[1].events.click();
    c.visible(2);
    c.tabs[0].events.click();
    await c.settle();
  }
  assert.deepEqual(c.requests, ['hero.gif', 'bubble.gif']);
  assert.equal(flowing.image.src, 'blob:hero.gif');
  const reduced = await demoController({ reduced: true });
  reduced.demos.forEach((_, i) => reduced.visible(i));
  reduced.tabs[1].events.click();
  await reduced.settle();
  assert.deepEqual(reduced.requests, []);
});

test('a late GIF response cannot restart a paused, hidden or reduced-motion demo', async () => {
  for (const action of ['pause', 'hide', 'reduce']) {
    const c = await demoController({ deferred: true });
    c.visible(0);
    if (action === 'pause') c.demos[0].button.events.click();
    if (action === 'hide') c.visible(0, false);
    if (action === 'reduce') c.reduced(true);
    c.complete('hero.gif');
    await c.settle();
    assert.equal(c.demos[0].image.src, 'hero.jpg', action);
    assert.deepEqual(c.requests, ['hero.gif']);
  }
});

test('a failed GIF request keeps a poster and permits an explicit retry', async () => {
  const c = await demoController({ deferred: true });
  c.visible(0);
  c.complete('hero.gif', false);
  await c.settle();
  assert.equal(c.demos[0].image.src, 'hero.jpg');
  assert.equal(c.demos[0].button.textContent, 'Play');
  c.demos[0].button.events.click();
  c.complete('hero.gif');
  await c.settle();
  assert.equal(c.demos[0].image.src, 'blob:hero.gif');
  assert.deepEqual(c.requests, ['hero.gif', 'hero.gif']);
});


test('invalid image bodies evict and revoke the shared URL before a successful retry', async () => {
  const c = await demoController({ decodeFailures: 1 });
  const [hero, flowing] = c.demos;
  flowing.image.dataset.animation = hero.image.dataset.animation;
  c.visible(0);
  c.visible(1);
  await c.settle();
  assert.deepEqual(c.requests, ['hero.gif']);
  assert.equal(hero.image.src, 'hero.jpg');
  assert.equal(flowing.image.src, 'flowing.jpg');
  assert.equal(hero.button.textContent, 'Play');
  assert.equal(flowing.button.textContent, 'Play');
  assert.deepEqual(c.revoked, ['blob:hero.gif']);
  hero.button.events.click();
  flowing.button.events.click();
  await c.settle();
  assert.deepEqual(c.requests, ['hero.gif', 'hero.gif']);
  assert.equal(hero.image.src, 'blob:hero.gif');
  assert.equal(flowing.image.src, hero.image.src);
  hero.button.events.click();
  hero.button.events.click();
  await c.settle();
  assert.deepEqual(c.requests, ['hero.gif', 'hero.gif']);
});
