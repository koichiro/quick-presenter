// Static posters are the no-JS fallback. Only visible, active demos animate.
(() => {
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const demos = new Map();
  const render = (demo) => {
    const { image, button, paused, visible } = demos.get(demo);
    const active = !demo.closest('[data-panel]')?.hidden;
    const playing = !motion.matches && !document.hidden && active && visible && !paused;
    const source = playing ? image.dataset.animation : image.dataset.poster;
    if (image.getAttribute('src') !== source) image.src = source;
    button.disabled = motion.matches;
    button.textContent = motion.matches ? demo.dataset.reduced
      : paused ? demo.dataset.play : demo.dataset.pause;
  };
  const renderAll = () => demos.forEach((_, demo) => render(demo));
  const observer = 'IntersectionObserver' in window ? new IntersectionObserver(entries => {
    for (const entry of entries) {
      demos.get(entry.target).visible = entry.isIntersecting;
      render(entry.target);
    }
  }) : null;
  for (const demo of document.querySelectorAll('[data-demo]')) {
    const image = demo.querySelector('img');
    const button = demo.querySelector('[data-playback]');
    demos.set(demo, { image, button, paused: false, visible: !observer });
    button.hidden = false;
    button.addEventListener('click', () => {
      const state = demos.get(demo);
      state.paused = !state.paused;
      render(demo);
    });
    observer?.observe(demo);
  }
  motion.addEventListener('change', renderAll);
  document.addEventListener('visibilitychange', renderAll);
  for (const comparison of document.querySelectorAll('[data-comparison]')) {
    const tablist = comparison.querySelector('[data-tabs]');
    const tabs = [...tablist.querySelectorAll('[data-tab]')];
    const panels = [...comparison.querySelectorAll('[data-panel]')];
    const select = (index, focus = false) => {
      tabs.forEach((tab, i) => {
        tab.setAttribute('aria-selected', String(i === index));
        tab.tabIndex = i === index ? 0 : -1;
        panels[i].hidden = i !== index;
      });
      renderAll();
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
  renderAll();
})();
