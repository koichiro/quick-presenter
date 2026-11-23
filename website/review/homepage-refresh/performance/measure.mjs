// Standalone lab capture; uses an existing Playwright installation, not a product dependency.
import { writeFile, mkdir } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { join } from 'node:path';
import os from 'node:os';

const { chromium } = await import(pathToFileURL(process.env.PLAYWRIGHT_MODULE).href);
const output = process.env.OUTPUT_DIR;
await mkdir(output, { recursive: true });
const settings = {
  viewport: { width: 390, height: 844 }, deviceScaleFactor: 1, isMobile: true, hasTouch: true,
  locale: 'en-US', timezoneId: 'Asia/Tokyo', reducedMotion: 'no-preference', serviceWorkers: 'block',
};
const network = { offline: false, latency: 150, downloadThroughput: 200000,
  uploadThroughput: 93750, connectionType: 'cellular4g' };
const browser = await chromium.launch({ executablePath: process.env.CHROME_PATH, headless: true });
const version = browser.version();
settings.userAgent = `Mozilla/5.0 (Linux; Android 10; Lab) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/${version} Mobile Safari/537.36`;
const servers = [];
for (const revision of ['before', 'after']) {
  const child = spawn(process.execPath, [join(process.env.CHECKOUT_ROOT, revision, 'website/scripts/preview.mjs')], {
    env: { ...process.env, HOST: '127.0.0.1', PORT: '0' }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  servers.push(child);
  const lines = createInterface({ input: child.stdout });
  const [line] = await once(lines, 'line');
  servers[revision] = line.match(/http:\/\/127\.0\.0\.1:\d+/)[0];
  lines.close();
}
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const pathname = value => {
  if (!value) return null;
  if (value.startsWith('blob:')) return 'blob:local-demo';
  try { return new URL(value).pathname; } catch { return value; }
};
async function setup(revision, language, options = {}) {
  const context = await browser.newContext({ ...settings, ...options });
  const page = await context.newPage();
  const cdp = await context.newCDPSession(page);
  await cdp.send('Network.enable');
  await cdp.send('Network.setCacheDisabled', { cacheDisabled: true });
  await cdp.send('Network.setBypassServiceWorker', { bypass: true });
  await cdp.send('Network.emulateNetworkConditions', network);
  await cdp.send('Emulation.setCPUThrottlingRate', { rate: 4 });
  const requests = new Map();
  cdp.on('Network.requestWillBeSent', e => {
    if (!e.request.url.startsWith('http:')) return;
    requests.set(e.requestId, { path: pathname(e.request.url), type: e.type, start: e.timestamp,
      receivedBodyBytes: 0, partialEncodedBodyBytes: 0, finished: false });
  });
  cdp.on('Network.responseReceived', e => {
    const r = requests.get(e.requestId); if (!r) return;
    Object.assign(r, { status: e.response.status, mimeType: e.response.mimeType,
      protocol: e.response.protocol, contentEncoding: e.response.headers['content-encoding'] || 'identity',
      responseHeadersBytes: e.response.encodedDataLength, cached: !!(e.response.fromDiskCache || e.response.fromServiceWorker) });
  });
  cdp.on('Network.dataReceived', e => {
    const r = requests.get(e.requestId); if (!r) return;
    r.receivedBodyBytes += e.dataLength; r.partialEncodedBodyBytes += e.encodedDataLength;
  });
  cdp.on('Network.loadingFinished', e => {
    const r = requests.get(e.requestId); if (!r) return;
    Object.assign(r, { finished: true, end: e.timestamp, transferredBytes: e.encodedDataLength });
  });
  cdp.on('Network.loadingFailed', e => {
    const r = requests.get(e.requestId); if (r) r.failure = e.errorText;
  });
  await context.addInitScript(language => {
    localStorage.setItem('quick-presenter-language', language);
    window.lab = { lcp: [], shifts: [] };
    const identify = node => node ? `${node.tagName?.toLowerCase()}${node.id ? '#'+node.id : ''}${node.className ? '.'+String(node.className).trim().replaceAll(' ','.') : ''}` : null;
    const local = value => !value ? null : value.startsWith('blob:') ? 'blob:local-demo' : new URL(value, location.href).pathname;
    new PerformanceObserver(list => {
      for (const e of list.getEntries()) window.lab.lcp.push({ time: e.startTime, renderTime: e.renderTime,
        loadTime: e.loadTime, size: e.size, element: identify(e.element), url: local(e.url),
        originalAnimation: e.element?.dataset.animation || null });
    }).observe({ type: 'largest-contentful-paint', buffered: true });
    new PerformanceObserver(list => {
      for (const e of list.getEntries()) window.lab.shifts.push({ time: e.startTime, value: e.value,
        hadRecentInput: e.hadRecentInput, sources: e.sources.map(s => ({ node: identify(s.node),
          previousRect: s.previousRect.toJSON(), currentRect: s.currentRect.toJSON() })) });
    }).observe({ type: 'layout-shift', buffered: true });
  }, language);
  const base = servers[revision];
  return { context, page, cdp, requests, url: `${base}${language === 'ja' ? '/ja/' : '/'}` };
}
async function snapshot(run) {
  const perf = await run.page.evaluate(() => {
    let max = 0, sum = 0, start = 0, previous = 0;
    for (const e of window.lab.shifts.filter(e => !e.hadRecentInput)) {
      if (!start || e.time - previous > 1000 || e.time - start > 5000) { start = e.time; sum = 0; }
      sum += e.value; max = Math.max(max, sum); previous = e.time;
    }
    const image = document.querySelector('#hero-demo');
    const button = document.querySelector('[data-playback]');
    return { time: performance.now(), pathname: location.pathname, lang: document.documentElement.lang,
      languageChoice: localStorage.getItem('quick-presenter-language'), lcp: window.lab.lcp.at(-1),
      lcpCandidates: window.lab.lcp, cls: max, layoutShifts: window.lab.shifts,
      navigation: { loadEventEnd: performance.getEntriesByType('navigation')[0].loadEventEnd },
      resources: performance.getEntriesByType('resource').filter(e=>e.name.startsWith('http:')).map(e=>({path:new URL(e.name).pathname,initiatorType:e.initiatorType,transferSize:e.transferSize,encodedBodySize:e.encodedBodySize,decodedBodySize:e.decodedBodySize})),
      demo: image ? { source: image.src.startsWith('blob:') ? 'blob:local-demo' : new URL(image.src).pathname,
        complete: image.complete, rect: image.getBoundingClientRect().toJSON(),
        button: { hidden: button.hidden, disabled: button.disabled, text: button.textContent, rect: button.getBoundingClientRect().toJSON() } } : null,
    };
  });
  const records = [...run.requests.values()].map(r => ({ ...r }));
  const earliest = Math.min(...records.map(r => r.start));
  for (const r of records) { r.startMs = (r.start - earliest) * 1000; if (r.end) r.endMs = (r.end - earliest) * 1000; delete r.start; delete r.end; }
  return { ...perf, network: { transferredBytes: records.reduce((n,r)=>n+(r.finished ? r.transferredBytes : (r.responseHeadersBytes || 0)+r.partialEncodedBodyBytes),0),
    requests: records.length, completeRequests: records.filter(r=>r.finished).length,
    records } };
}
try {
  const metadata = { capturedOn: '2026-11-23', before: 'a6a38d1acd14faaf60e34d8b71bf96bf0cab221e',
    after: process.env.AFTER_REVISION || 'd5ed4d5fea9d62ef014dd0170b8d3d30c7310a61', browser: version,
    tool: 'Chromium CDP Network + PerformanceObserver via existing Playwright 1.62.1',
    node: process.version, host: { osVersion: execFileSync('sw_vers', ['-productVersion'], {encoding:'utf8'}).trim(), osBuild: execFileSync('sw_vers', ['-buildVersion'], {encoding:'utf8'}).trim(), platform: os.platform(), release: os.release(), architecture: os.arch(), cpu: os.cpus()[0]?.model },
    context: settings, network, cpuSlowdown: 4, server: 'Unmodified website/scripts/preview.mjs; HTTP/1.1, no compression',
    initialWindow: 'load event + 5000 ms, no input or scrolling',
    extendedWindow: 'At least 20000 ms after navigation AND at least 5000 ms after GIF Fetch completion/Blob display; before pages use navigation + 20000 ms minimum',
    networkByteDefinition: 'Completed responses use CDP Network.loadingFinished.encodedDataLength (response headers + encoded body, excluding request/TCP overhead). In-flight responses use only header/body bytes already notified by CDP; fetch body notifications can be deferred, so initial totals are lower bounds until all requests finish.',
    cache: 'Fresh browser context per run; empty HTTP cache/cookies/storage, CDP cache disabled, Service Workers blocked and bypassed. Only local language choice is seeded before page scripts.',
    sanitization: 'Only pathnames, media metadata, byte counts, rectangles and performance entries are retained. No headers, cookies, bodies, host ports, user profile, or local absolute paths are exported.',
  };
  await writeFile(join(output, 'conditions.json'), JSON.stringify(metadata,null,2)+'\n');
  if (process.env.FRAGMENT_ONLY) {
    const run = await setup('after', 'en');
    await run.context.addInitScript(() => {
      window.visibilityProbe = [];
      const Native = window.IntersectionObserver;
      window.IntersectionObserver = class extends Native {
        constructor(callback, options) {
          super((entries, observer) => {
            window.visibilityProbe.push(...entries.map(e=>({time:performance.now(),visible:e.isIntersecting,top:e.boundingClientRect.top,bottom:e.boundingClientRect.bottom,viewportHeight:innerHeight})));
            callback(entries, observer);
          }, options);
        }
      };
    });
    await run.page.goto(run.url+'#download', {waitUntil:'load'});
    await sleep(5000);
    await writeFile(join(output,'fragment-diagnostic.json'),JSON.stringify({language:'en',viewport:settings.viewport,
      observation:await snapshot(run),visibilityTimeline:await run.page.evaluate(()=>window.visibilityProbe)},null,2)+'\n');
    await run.context.close();
    console.log('FRAGMENT diagnostic complete');
  } else if (process.env.VISIBILITY_ONLY) {
    const evidence = [];
    for (const language of ['en', 'ja']) {
      const run = await setup('after', language, {viewport:{width:390,height:480}});
      await run.page.goto(run.url, {waitUntil:'load'});
      await sleep(5000);
      const offscreen = await snapshot(run);
      if ([...run.requests.values()].some(r=>r.path.endsWith('/flowing.gif'))) throw Error('Offscreen demo fetched GIF');
      await run.page.evaluate(()=>document.querySelector('#hero-demo').scrollIntoView({block:'center'}));
      await run.page.waitForFunction(()=>document.querySelector('#hero-demo').src.startsWith('blob:') && document.querySelector('#hero-demo').complete, null, {timeout:120000});
      await sleep(5000);
      evidence.push({language,viewport:{width:390,height:480},offscreen,visible:await snapshot(run)});
      await writeFile(join(output,'visibility.json'),JSON.stringify(evidence,null,2)+'\n');
      console.log(`VISIBILITY ${language} complete`);
      await run.context.close();
    }
  } else {
  const results = [];
  // Alternate revision order to reduce time/thermal-order bias; only one measured page runs at a time.
  for (let iteration = 1; iteration <= 3; iteration++) for (const language of ['en','ja']) {
    for (const revision of process.env.AFTER_ONLY ? ['after'] : iteration % 2 ? ['before','after'] : ['after','before']) {
      const run = await setup(revision, language);
      console.log(`START ${language} ${revision} ${iteration}`);
      await run.page.goto(run.url, { waitUntil: 'load', timeout: 120000 });
      await sleep(5000);
      const initial = await snapshot(run);
      if (initial.pathname !== (language === 'ja' ? '/ja/' : '/')) throw Error('Unexpected language redirect');
      if (revision === 'after') {
        await run.page.waitForFunction(() => document.querySelector('#hero-demo').src.startsWith('blob:') && document.querySelector('#hero-demo').complete, null, {timeout:120000});
        const gif = [...run.requests.values()].find(r=>r.path.endsWith('/flowing.gif') && r.finished);
        if (!gif) throw Error('GIF Fetch did not finish');
        await sleep(5000);
      }
      const elapsed = await run.page.evaluate(()=>performance.now());
      if (elapsed < 20000) await sleep(20000-elapsed);
      const extended = await snapshot(run);
      const result = { language, revision, iteration, initial, extended };
      results.push(result);
      await writeFile(join(output, 'runs.json'),JSON.stringify(results,null,2)+'\n');
      console.log(`DONE ${language} ${revision} ${iteration}: LCP=${extended.lcp?.time.toFixed(0)}ms CLS=${extended.cls} bytes=${extended.network.transferredBytes} requests=${extended.network.requests}`);
      await run.context.close();
    }
  }
  const controls = [];
  for (const language of process.env.SKIP_CONTROLS ? [] : ['en','ja']) {
    const run = await setup('after',language);
    await run.page.goto(run.url,{waitUntil:'load'});
    await run.page.waitForFunction(()=>document.querySelector('#hero-demo').src.startsWith('blob:') && document.querySelector('#hero-demo').complete,null, {timeout:120000});
    await sleep(5000);
    const states = [{ step:'loaded-and-observed-5s', data:await snapshot(run) }];
    for (let i=0;i<3;i++) {
      await run.page.locator('[data-playback]').click(); await sleep(600);
      states.push({step:`pause-${i+1}`,data:await snapshot(run)});
      await run.page.locator('[data-playback]').click(); await sleep(600);
      states.push({step:`play-${i+1}`,data:await snapshot(run)});
    }
    await run.page.evaluate(()=>window.scrollTo(0,1800)); await sleep(1200);
    states.push({step:'offscreen',data:await snapshot(run)});
    await run.page.evaluate(()=>window.scrollTo(0,0)); await sleep(5000);
    states.push({step:'returned-and-observed-5s',data:await snapshot(run)});
    controls.push({language,mode:'normal',states});
    await run.context.close();
    for (const mode of ['reduced-motion','no-javascript']) {
      const run = await setup('after',language, mode==='reduced-motion'?{reducedMotion:'reduce'}:{javaScriptEnabled:false});
      await run.page.goto(run.url,{waitUntil:'load'}); await sleep(20000);
      const image = await run.page.locator('#hero-demo').getAttribute('src');
      const buttons = { visible:await run.page.locator('[data-playback]').isVisible(),text:await run.page.locator('[data-playback]').textContent() };
      const ctas = await run.page.locator('.home-message .hero-actions a').evaluateAll(as=>as.map(a=>({text:a.textContent,href:a.getAttribute('href'),visible:a.getBoundingClientRect().width>0})));
      controls.push({language,mode,image,buttons,ctas,network:[...run.requests.values()].map(r=>({path:r.path,type:r.type,finished:r.finished,transferredBytes:r.transferredBytes}))});
      await run.context.close();
    }
    await writeFile(join(output,'controls.json'),JSON.stringify(controls,null,2)+'\n');
    console.log(`CONTROLS ${language} complete`);
  }
  }
} finally { await browser.close(); for(const child of servers) child.kill(); }
