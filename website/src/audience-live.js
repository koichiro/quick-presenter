// Native looping GIFs also work without JS; picture sources respect reduced motion.
(() => {
  for (const comparison of document.querySelectorAll('[data-comparison]')) {
    const tablist = comparison.querySelector('[data-tabs]');
    const tabs = [...tablist.querySelectorAll('[data-tab]')];
    const panels = [...comparison.querySelectorAll('[data-panel]')];
    const select = (index, focus = false) => {
      tabs.forEach((tab, i) => {
        tab.setAttribute('aria-selected', String(i === index));
        tab.tabIndex = i === index ? 0 : -1;
        panels[i].hidden = i !== index;
        const image = panels[i].querySelector('img');
        image.src = i === index ? image.dataset.animation : image.dataset.poster;
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
