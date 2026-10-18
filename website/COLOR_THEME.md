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
| `--surface-strong` | `#f7fafb` | Quiet cards |
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
- Keep the original v2 dark command panels on homepages and guides.
  `--code-surface` is `#0c2635`, `--code-ink` is `#e9f4f7`, and
  `--code-muted` is `#a5c4cd`. Preserve the original pale teal prompts,
  captions, terminal dots, and dark dividers. These focused code surfaces
  sit within the light brand theme.
- Keep the v1 dark download section as the closing call to action. Use the
  existing pale teal for links there; light download cards retain dark text.
- Preserve the original colors inside screenshots, recordings, and store
  badges. These are product evidence or third-party assets, not theme surfaces.

## Verification

Run `npm test` in `website/`. Check both locales at desktop and phone widths,
including the hero, command examples, guides, download cards, navigation, and
keyboard focus. Normal text must meet WCAG AA's 4.5:1 contrast ratio. The theme's
code text and comments exceed that ratio on the dark code surface; white on
strong teal and pale teal on the dark download surface do as well.
