// Browser regression evidence; reuse an existing Playwright/Chrome installation.
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { join } from 'node:path';
import assert from 'node:assert/strict';
const { chromium } = await import(pathToFileURL(process.env.PLAYWRIGHT_MODULE).href);
const output = process.env.OUTPUT_DIR;
await mkdir(output,{recursive:true});
const server = spawn(process.execPath,[join(process.env.CHECKOUT_ROOT,'after/website/scripts/preview.mjs')],{
  env:{...process.env,HOST:'127.0.0.1',PORT:'0'},stdio:['ignore','pipe','pipe'],
});
const lines = createInterface({input:server.stdout});
const [line] = await once(lines,'line');
const origin = line.match(/http:\/\/127\.0\.0\.1:\d+/)[0];
lines.close();
const browser = await chromium.launch({headless:true,executablePath:process.env.CHROME_PATH});
const results=process.env.ONLY_LANDING && !process.env.NO_OBSERVER ? JSON.parse(await readFile(join(output,'browser-checks.json'),'utf8')).cases.filter(c=>c.name!=='audience-live-shared-controller') : [];
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function session(language,slow=false,options={}) {
  const context=await browser.newContext({viewport:{width:390,height:844},deviceScaleFactor:1,isMobile:true,hasTouch:true,
    locale:'en-US',timezoneId:'Asia/Tokyo',reducedMotion:'no-preference',serviceWorkers:'block',
    userAgent:`Mozilla/5.0 (Linux; Android 10; Lab) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/${browser.version()} Mobile Safari/537.36`,...options});
  await context.addInitScript(({language,noObserver})=>{
    localStorage.setItem('quick-presenter-language',language);
    window.intersections=[];
    if(noObserver) { delete window.IntersectionObserver; return; }
    const Native=window.IntersectionObserver;
    window.IntersectionObserver=class extends Native {
      constructor(callback,options) {
        super((entries,observer)=>{
          window.intersections.push(...entries.map(e=>({time:performance.now(),visible:e.isIntersecting,top:e.boundingClientRect.top,bottom:e.boundingClientRect.bottom})));
          callback(entries,observer);
        },options);
      }
    };
  },{language,noObserver:Boolean(process.env.NO_OBSERVER)});
  const page=await context.newPage();
  const cdp=await context.newCDPSession(page);
  await cdp.send('Network.enable');
  await cdp.send('Network.setCacheDisabled',{cacheDisabled:true});
  await cdp.send('Network.setBypassServiceWorker',{bypass:true});
  await cdp.send('Emulation.setCPUThrottlingRate',{rate:4});
  if(slow) await cdp.send('Network.emulateNetworkConditions',{offline:false,latency:150,downloadThroughput:200000,uploadThroughput:93750,connectionType:'cellular4g'});
  const requests=new Map();
  let started;
  cdp.on('Network.requestWillBeSent',e=>{
    if(!e.request.url.startsWith(origin))return;
    started??=e.timestamp;
    requests.set(e.requestId,{path:new URL(e.request.url).pathname,type:e.type,startMs:(e.timestamp-started)*1000});
  });
  cdp.on('Network.responseReceived',e=>{const r=requests.get(e.requestId);if(r)r.status=e.response.status;});
  cdp.on('Network.loadingFinished',e=>{const r=requests.get(e.requestId);if(r)Object.assign(r,{finished:true,endMs:(e.timestamp-started)*1000,transferredBytes:e.encodedDataLength});});
  const states=[];
  async function state(step) {
    const dom=await page.evaluate(()=>({pathname:location.pathname,hash:location.hash,time:performance.now(),scrollY,viewport:{width:innerWidth,height:innerHeight},observerAvailable:typeof window.IntersectionObserver==='function',
      images:[...document.querySelectorAll('[data-demo] img')].map(i=>({id:i.id,src:i.src.startsWith('blob:')?'blob:local-demo':new URL(i.src).pathname,complete:i.complete,rect:i.getBoundingClientRect().toJSON()})),
      buttons:[...document.querySelectorAll('[data-playback]')].map(b=>({text:b.textContent,hidden:b.hidden,disabled:b.disabled})),
      intersections:window.intersections||[]}));
    const record={step,...dom,network:[...requests.values()].map(r=>({...r}))};states.push(record);return record;
  }
  return {context,page,requests,states,state,settings:{viewport:{width:390,height:844},deviceScaleFactor:1,cpuSlowdown:4,network:slow?'1.6 Mbps down / 750 Kbps up / 150 ms latency':'unthrottled'},language};
}
const gifs=r=>[...r.requests.values()].filter(x=>x.path.endsWith('.gif'));
const playing=async(page,id='hero-demo')=>page.waitForFunction(id=>{
  const i=document.getElementById(id);return i?.src.startsWith('blob:')&&i.complete;
},id,{timeout:120000});
async function save(r,name){results.push({name,language:r.language,settings:r.settings,states:r.states});await writeFile(join(output,'browser-checks.json'),JSON.stringify({revision:process.env.AFTER_REVISION,browser:browser.version(),cases:results},null,2)+'\n');await r.context.close();console.log(`PASS ${r.language} ${name}`);}
try {
  for(const language of ['en','ja']) {
    const root=language==='ja'?'/ja/':'/';
    for(const anchor of process.env.ONLY_LANDING ? [] : ['download','features','automation','authoring','scope','demo']) {
      const r=await session(language,anchor==='download');
      await r.page.goto(origin+root+'#'+anchor,{waitUntil:'load',timeout:120000});
      await sleep(5000);
      const landed=await r.state('anchor-settled-load-plus-5s');
      assert.equal(gifs(r).length,0);
      assert.ok(landed.images[0].src.endsWith('.jpg'));
      assert.ok(landed.images[0].rect.bottom<0);
      if(anchor==='download')await r.page.screenshot({path:join(output,`download-${language}.jpg`)});
      await r.page.evaluate(()=>window.scrollTo({top:0,behavior:'instant'}));
      await playing(r.page);
      await sleep(anchor==='download'?5000:300);
      assert.equal(gifs(r).length,1);
      await r.state('returned-to-hero-playing');
      if(anchor==='download') {
        await r.page.screenshot({path:join(output,`returned-${language}.jpg`)});
        for(let i=0;i<3;i++) {
          await r.page.locator('[data-playback]').click();await sleep(150);
          assert.equal((await r.state(`pause-${i+1}`)).images[0].src,'/assets/audience-live/flowing.jpg');
          await r.page.locator('[data-playback]').click();await playing(r.page);await r.state(`play-${i+1}`);
        }
        await r.page.evaluate(()=>window.scrollTo({top:1800,behavior:'instant'}));await sleep(400);
        assert.equal((await r.state('offscreen')).images[0].src,'/assets/audience-live/flowing.jpg');
        await r.page.evaluate(()=>window.scrollTo({top:0,behavior:'instant'}));await playing(r.page);await sleep(5000);
        await r.state('returned-again-plus-5s');assert.equal(gifs(r).length,1);
      }
      await save(r,`home-${anchor}`);
    }
    for(const hash of process.env.ONLY_LANDING ? [] : ['','#missing-demo','#%E0%A4%A','#top']) {
      const r=await session(language);
      await r.page.goto(origin+root+hash,{waitUntil:'load'});await playing(r.page);
      await r.state('visible-hero-playing');assert.equal(gifs(r).length,1);await save(r,`visible-${hash||'no-fragment'}`);
    }
    for(const mode of process.env.ONLY_LANDING ? [] : ['reduced-motion','no-javascript']) {
      const r=await session(language,false,mode==='reduced-motion'?{reducedMotion:'reduce'}:{javaScriptEnabled:false});
      await r.page.goto(origin+root,{waitUntil:'load'});await sleep(5000);
      const s=await r.state(mode+'-load-plus-5s');assert.equal(gifs(r).length,0);assert.ok(s.images[0].src.endsWith('.jpg'));
      assert.equal(await r.page.locator('.home-message .hero-actions a').count(),2);
      if(mode==='no-javascript')assert.equal(await r.page.locator('[data-playback]').isVisible(),false);
      else assert.equal(s.buttons[0].disabled,true);
      await save(r,mode);
    }
    const r=await session(language);
    await r.page.goto(origin+root+'audience-live/#connect',{waitUntil:'load'});await sleep(5000);
    await r.state('landing-connect-settled');assert.equal(gifs(r).length,0);
    await r.page.locator('#hero-demo').scrollIntoViewIfNeeded();await playing(r.page);assert.equal(gifs(r).length,1);await r.state('landing-hero-visible');
    if(process.env.NO_OBSERVER) {
      const button=r.page.locator('[data-playback]').first();
      await button.click();await sleep(200);
      assert.equal((await r.state('fallback-paused')).images[0].src,'/assets/audience-live/flowing.jpg');
      await button.click();await playing(r.page);await r.state('fallback-play');
      await r.page.evaluate(()=>window.scrollTo({top:0,behavior:'instant'}));
      await r.page.setViewportSize({width:390,height:200});await sleep(400);
      assert.equal((await r.state('fallback-resize-small')).images[0].src,'/assets/audience-live/flowing.jpg');
      await r.page.setViewportSize({width:390,height:2000});await playing(r.page);
      await r.state('fallback-resize-large');assert.equal(gifs(r).length,1);
      await r.page.setViewportSize({width:390,height:844});
      await r.page.locator('#hero-demo').scrollIntoViewIfNeeded();await playing(r.page);
    }
    await r.page.locator('#panel-flowing').scrollIntoViewIfNeeded();await playing(r.page,'flowing-demo');assert.equal(gifs(r).length,1);
    await r.page.locator('#tab-bubbles').click();await r.page.locator('#bubble-demo').scrollIntoViewIfNeeded();await playing(r.page,'bubble-demo');assert.equal(gifs(r).length,2);
    await r.page.locator('#tab-flowing').click();await r.page.locator('#flowing-demo').scrollIntoViewIfNeeded();await playing(r.page,'flowing-demo');assert.equal(gifs(r).length,2);
    await r.state('landing-hero-tabs-shared-fetch');await save(r,'audience-live-shared-controller');
  }
} finally {await browser.close();server.kill();}
