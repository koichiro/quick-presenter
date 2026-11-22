// Progressive enhancement: posters and original GIF links also work without JS.
(() => {
  const demos = [...document.querySelectorAll('[data-demo]')];
  const stops = new Map();
  for (const demo of demos) {
    const image = demo.querySelector('img');
    const button = demo.querySelector('[data-playback]');
    const poster = image.getAttribute('src');
    let timer;
    const stop = () => {
      clearTimeout(timer);
      image.onload = null;
      image.onerror = null;
      image.src = poster;
      button.textContent = demo.dataset.play;
      button.setAttribute('aria-pressed', 'false');
    };
    stops.set(demo, stop);
    button.hidden = false;
    button.addEventListener('click', () => {
      if (button.getAttribute('aria-pressed') === 'true') {
        stop();
        return;
      }
      // Keep only the selected demo moving. Stop remains available during loading.
      stops.forEach(stopDemo => stopDemo());
      button.setAttribute('aria-pressed', 'true');
      button.textContent = demo.dataset.stop;
      image.onload = () => { timer = setTimeout(stop, 27000); };
      image.onerror = stop;
      image.src = image.dataset.animation;
    });
  }
  const stopAll = () => stops.forEach(stop => stop());
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) stopAll();
  });
  window.addEventListener('pagehide', stopAll);
  window.matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change', stopAll);

  for (const comparison of document.querySelectorAll('[data-comparison]')) {
    const tablist = comparison.querySelector('[data-tabs]');
    const tabs = [...tablist.querySelectorAll('[data-tab]')];
    const panels = [...comparison.querySelectorAll('[data-panel]')];
    const select = (index, focus = false) => {
      stopAll();
      tabs.forEach((tab, i) => {
        tab.setAttribute('aria-selected', String(i === index));
        tab.tabIndex = i === index ? 0 : -1;
        panels[i].hidden = i !== index;
      });
      if (focus) tabs[index].focus();
    };
    tablist.setAttribute('role', 'tablist');
    tabs.forEach((tab, i) => {
      tab.setAttribute('role', 'tab');
      tab.setAttribute('aria-controls', tab.dataset.tab);
      panels[i].setAttribute('role', 'tabpanel');
      panels[i].setAttribute('aria-labelledby', tab.id);
      panels[i].tabIndex = 0;
      tab.addEventListener('click', () => select(i));
      tab.addEventListener('keydown', event => {
        const index = event.key === 'ArrowRight' ? (i + 1) % tabs.length
          : event.key === 'ArrowLeft' ? (i + tabs.length - 1) % tabs.length
          : event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : null;
        if (index !== null) {
          event.preventDefault();
          select(index, true);
        }
      });
    });
    select(0);
    tablist.hidden = false;
  }
})();
