---
marp: true
theme: default
paginate: true
size: 16:9
style: |
  :root {
    --qp-blue: #2563eb;
    --qp-cyan: #06b6d4;
    --qp-ink: #172033;
    --qp-muted: #64748b;
    --qp-soft: #eef7ff;
    --qp-line: #cbd5e1;
  }

  section {
    background: linear-gradient(135deg, #ffffff 0%, #f7fbff 62%, #eaf6ff 100%);
    color: var(--qp-ink);
    font-family: "Inter", "Aptos", "Helvetica Neue", Arial, sans-serif;
    letter-spacing: 0;
    padding: 68px 78px;
  }

  section.lead {
    background: linear-gradient(135deg, #58b7ff 0%, #2d7df6 48%, #13b6d1 100%);
    color: #ffffff;
  }

  section.japanese {
    font-family: "Hiragino Sans", "Yu Gothic", "Noto Sans CJK JP", "Helvetica Neue", Arial, sans-serif;
  }

  h1 {
    font-size: 74px;
    line-height: 1.02;
    margin: 0 0 28px;
    color: inherit;
  }

  h2 {
    font-size: 52px;
    line-height: 1.08;
    margin: 0 0 34px;
    color: var(--qp-ink);
  }

  .lead h1 {
    max-width: 860px;
  }

  .subtitle {
    font-size: 30px;
    line-height: 1.32;
    max-width: 820px;
    color: rgba(255, 255, 255, 0.92);
  }

  .eyebrow {
    display: inline-block;
    margin-bottom: 26px;
    padding: 8px 14px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.18);
    color: rgba(255, 255, 255, 0.92);
    font-size: 20px;
    font-weight: 700;
  }

  .kicker {
    color: var(--qp-blue);
    font-size: 20px;
    font-weight: 800;
    margin-bottom: 18px;
    text-transform: uppercase;
  }

  .grid {
    display: grid;
    gap: 22px;
    grid-template-columns: repeat(3, 1fr);
  }

  .two {
    grid-template-columns: repeat(2, 1fr);
  }

  .panel {
    border: 1px solid var(--qp-line);
    border-radius: 18px;
    background: rgba(255, 255, 255, 0.82);
    box-shadow: 0 18px 46px rgba(31, 65, 118, 0.12);
    padding: 28px;
    min-height: 180px;
  }

  .panel strong {
    display: block;
    margin-bottom: 14px;
    color: var(--qp-blue);
    font-size: 28px;
  }

  .panel p,
  li {
    color: var(--qp-muted);
    font-size: 25px;
    line-height: 1.34;
  }

  ul {
    margin: 0;
    padding-left: 32px;
  }

  li + li {
    margin-top: 16px;
  }

  .timeline {
    display: grid;
    gap: 18px;
  }

  .step {
    display: grid;
    grid-template-columns: 70px 1fr;
    align-items: center;
    gap: 20px;
    border-bottom: 1px solid var(--qp-line);
    padding: 17px 0;
  }

  .number {
    align-items: center;
    background: linear-gradient(135deg, var(--qp-blue), var(--qp-cyan));
    border-radius: 50%;
    color: white;
    display: flex;
    font-size: 28px;
    font-weight: 800;
    height: 60px;
    justify-content: center;
    width: 60px;
  }

  .step strong {
    display: block;
    font-size: 30px;
    margin-bottom: 4px;
  }

  .step span {
    color: var(--qp-muted);
    font-size: 24px;
  }

  .screen {
    position: absolute;
    right: 84px;
    bottom: 74px;
    width: 330px;
    height: 210px;
    border-radius: 22px;
    background: rgba(255, 255, 255, 0.18);
    border: 2px solid rgba(255, 255, 255, 0.48);
    box-shadow: 0 28px 70px rgba(14, 43, 91, 0.28);
  }

  .screen::before {
    content: "";
    position: absolute;
    left: 58px;
    right: 58px;
    bottom: -34px;
    height: 12px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.72);
  }

  .play {
    position: absolute;
    left: 138px;
    top: 65px;
    width: 0;
    height: 0;
    border-top: 40px solid transparent;
    border-bottom: 40px solid transparent;
    border-left: 68px solid rgba(255, 255, 255, 0.88);
  }

  .note {
    position: absolute;
    right: 30px;
    top: 30px;
    width: 54px;
    height: 70px;
    border-radius: 8px;
    background: #ffffff;
    box-shadow: 0 12px 32px rgba(14, 43, 91, 0.18);
  }

  .note::before,
  .note::after {
    content: "";
    position: absolute;
    left: 12px;
    right: 12px;
    height: 5px;
    border-radius: 999px;
    background: #93c5fd;
  }

  .note::before {
    top: 18px;
  }

  .note::after {
    top: 34px;
  }
---

<!-- _class: lead -->

<div class="eyebrow">PDF-first presentation playback</div>

# Quick Presenter

<div class="subtitle">A focused presenter tool for reliable live talks, online meetings, and conference rooms.</div>

<div class="screen"><div class="play"></div><div class="note"></div></div>

<!--
Open with the product promise: Quick Presenter does one thing well by playing prepared PDF slide decks with presenter-focused controls.
-->

---

<div class="kicker">The problem</div>

## PDF decks are stable. PDF viewers are not presenter tools.

<div class="grid">
  <div class="panel"><strong>Context is hidden</strong><p>Speakers need current slide, next slide, notes, timing, and page status at a glance.</p></div>
  <div class="panel"><strong>Sharing is fragile</strong><p>Online meetings need a clean audience window that stays separate from private notes.</p></div>
  <div class="panel"><strong>Live talks are unforgiving</strong><p>Navigation and fullscreen behavior must feel predictable under pressure.</p></div>
</div>

<!--
Use this slide to explain why Quick Presenter exists even though every operating system already has a PDF viewer.
-->

---

<div class="kicker">Presenter workflow</div>

## Everything needed during the talk stays in one place.

<div class="grid two">
  <div class="panel"><strong>Presenter window</strong><p>Current slide, next preview, notes, elapsed time, clock, and page count.</p></div>
  <div class="panel"><strong>Slide window</strong><p>A clean output surface for projectors, displays, and screen sharing.</p></div>
  <div class="panel"><strong>Keyboard first</strong><p>Move through slides with keys, menus, buttons, and common presenter remotes.</p></div>
  <div class="panel"><strong>PDF only</strong><p>No editing tools during a talk, no document management workflow, no distractions.</p></div>
</div>

<!--
This is a good slide for README presenter screenshots because it has enough structure to show the current slide and next-slide preview clearly.
-->

---

<div class="kicker">Basic flow</div>

## Keep the live presentation path short.

<div class="timeline">
  <div class="step"><div class="number">1</div><div><strong>Open the PDF</strong><span>Use the final PDF exported from Keynote, PowerPoint, Marp, or Beamer.</span></div></div>
  <div class="step"><div class="number">2</div><div><strong>Check speaker notes</strong><span>Keep private notes aligned with the current slide in the presenter window.</span></div></div>
  <div class="step"><div class="number">3</div><div><strong>Share the slide window</strong><span>Send only the clean audience view to a projector or online meeting.</span></div></div>
</div>

<!--
This page is the English counterpart to the Japanese README screenshot page.
It shows the same audience-facing workflow without exposing presenter-only UI.
-->

---

<!-- _class: japanese lead -->

<div class="eyebrow">研究発表と技術発表のための再生ツール</div>

# PDF スライドを安心して発表する

<div class="subtitle">発表者ビュー、次スライド、ノート、タイマーを確認しながら、聴衆にはきれいなスライドだけを表示します。</div>

<div class="screen"><div class="play"></div><div class="note"></div></div>

<!--
日本語スライドでも本文とノートが問題なく扱えることを確認するためのページです。
-->

---

<!-- _class: japanese -->

<div class="kicker">基本の流れ</div>

## 本番中の操作は、できるだけ短く。

<div class="timeline">
  <div class="step"><div class="number">1</div><div><strong>PDF を開く</strong><span>Keynote、PowerPoint、Marp、Beamer から書き出した PDF を使います。</span></div></div>
  <div class="step"><div class="number">2</div><div><strong>ノートを確認する</strong><span>発表者だけが見るメモを、現在のスライドに合わせて表示します。</span></div></div>
  <div class="step"><div class="number">3</div><div><strong>聴衆用ウィンドウを共有する</strong><span>プロジェクターやオンライン会議には、余計な情報を出しません。</span></div></div>
</div>

<!--
このページは日本語の README スクリーンショット向けです。聴衆ウィンドウにはノートや操作 UI が出ないことを示します。
-->

---

<div class="kicker">Online meetings</div>

## Share the slide window. Keep the presenter window private.

<div class="grid">
  <div class="panel"><strong>Zoom</strong><p>Share only the audience-facing window and keep notes on your own display.</p></div>
  <div class="panel"><strong>Google Meet</strong><p>Use a stable PDF output while tracking timing and the next slide locally.</p></div>
  <div class="panel"><strong>Hybrid rooms</strong><p>Use the same PDF workflow for projector output and remote participants.</p></div>
</div>

<!--
Mention that the target audience includes speakers who present over online meeting tools, not only in physical conference rooms.
-->

---

<!-- _class: lead -->

<div class="eyebrow">MVP scope</div>

# Stable PDF playback first

<div class="subtitle">Quick Presenter keeps the live presentation path small: open a PDF, present confidently, and avoid editor complexity on stage.</div>

<div class="screen"><div class="play"></div><div class="note"></div></div>

<!--
Close by reinforcing scope. Quick Presenter is intentionally not a slide editor or a file manager.
-->
