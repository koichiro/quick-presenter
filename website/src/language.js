(() => {
  const key = "quick-presenter-language";
  const current = document.documentElement.lang === "ja" ? "ja" : "en";
  let saved;
  try {
    saved = localStorage.getItem(key);
  } catch {
    // Language links still work when browser storage is unavailable.
  }
  const explicit = new URLSearchParams(location.search).get("lang");
  const preferred = (navigator.languages?.[0] || navigator.language || "en")
    .toLowerCase().startsWith("ja") ? "ja" : "en";
  // Respect a direct Japanese URL on the first visit.
  const language = explicit === "ja" || explicit === "en" ? explicit
    : saved === "ja" || saved === "en" ? saved
    : current === "ja" ? "ja" : preferred;
  try {
    localStorage.setItem(key, language);
  } catch {
    // A Japanese destination will not redirect back if storage is blocked.
  }
  document.addEventListener("click", (event) => {
    const link = event.target.closest?.(".language-switch a[hreflang]");
    if (!link) return;
    try {
      localStorage.setItem(key, link.hreflang);
    } catch {
      // Carry the explicit choice to the destination without browser storage.
      const destination = new URL(link.href, location.href);
      destination.searchParams.set("lang", link.hreflang);
      link.href = destination.href;
    }
  });
  if (language !== current) {
    const path = current === "ja"
      ? location.pathname.slice(3) : `/ja${location.pathname}`;
    location.replace(path + location.search + location.hash);
  }
})();
