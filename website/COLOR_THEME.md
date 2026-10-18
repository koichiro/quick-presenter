# Quick Presenter color theme

## Concept: calm, clear, dependable

Quick Presenter's website uses the light palette shipped in
`v1.0.0:website/src/styles.css` as its brand baseline. White space, dark neutral
text, restrained teal actions, and a small amber highlight express a focused,
dependable PDF player. Version 2.0's AI and CLI workflows belong to that same
product and use the same visual language.

## Palette and roles

The shared implementation is `src/styles.css`; all six English and Japanese
home, guide, and privacy pages use it.

| Token | Value | Role |
| --- | --- | --- |
| `--surface` | `#ffffff` | Main page background and secondary buttons |
| `--surface-strong` | `#f7fafb` | Quiet cards and code examples |
| `--subtle` | `#eef3f5` | Reserved baseline neutral tint |
| `--ink` | `#172026` | Headings, body text, and download-section background |
| `--muted` | `#5a6670` | Supporting copy and code comments |
| `--line` | `#d8e2e7` | Subtle card outlines and dividers |
| `--accent` | `#206a73` | Eyebrows, focus indicators, and small accents |
| `--accent-strong` | `#124c55` | Primary buttons, links, and code prompts |
| `--accent-soft` | `#e2f3f1` | Selected language and supporting badges |
| `--warm` | `#f7b955` | Small decorative highlight behind the hero screenshot |
| `--on-dark` | `#b7c9cf` | Supporting text on the dark download section |

Keep the v1 shadow (`0 24px 60px rgba(23, 32, 38, 0.14)`) restrained.
Amber is decorative; do not use it for body text on white.

## Version 2.0 application

- Retain the existing content, section order, typography, responsive layout,
  screenshots, videos, and download destinations.
- Restore the amber hero highlight used in v1.0.0.
- Present CLI examples as light, bordered cards alongside the other features.
  `--code-surface`, `--code-ink`, and `--code-muted` alias the existing surface,
  text, and supporting-text tokens. Use strong teal for prompts and neutral
  supporting text for comments and captions. Do not introduce a separate navy
  terminal palette for AI or automation features.
- Keep the v1 dark download section as the closing call to action. Use the
  existing pale teal for links there; light download cards retain dark text.
- Preserve the original colors inside screenshots, recordings, and store
  badges. These are product evidence or third-party assets, not theme surfaces.

## Verification

Run `npm test` in `website/`. Check both locales at desktop and phone widths,
including the hero, command examples, guides, download cards, navigation, and
keyboard focus. Normal text must meet WCAG AA's 4.5:1 contrast ratio. The theme's
ink, muted text, and strong teal all exceed that ratio on the light code surface;
white on strong teal and pale teal on the dark download surface do as well.
