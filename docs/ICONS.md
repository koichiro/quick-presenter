# Icon Assets

Quick Presenter uses a single high-resolution source icon and generated platform assets.

## Source

- `assets/icons/source/quick-presenter-icon-1024.png`
- Size: 1024x1024 px
- Design: approved B-1 concept with a bright blue gradient background, a white presentation screen, a play symbol, and a small note marker.

Keep this file as the source of truth. Do not hand-edit generated size variants.

## Generated Assets

The committed generated assets cover the common platform packaging requirements:

- `assets/icons/png/quick-presenter-icon-16.png`
- `assets/icons/png/quick-presenter-icon-24.png`
- `assets/icons/png/quick-presenter-icon-32.png`
- `assets/icons/png/quick-presenter-icon-48.png`
- `assets/icons/png/quick-presenter-icon-64.png`
- `assets/icons/png/quick-presenter-icon-128.png`
- `assets/icons/png/quick-presenter-icon-256.png`
- `assets/icons/png/quick-presenter-icon-512.png`
- `assets/icons/png/quick-presenter-icon-1024.png`
- `assets/icons/windows/quick-presenter.ico`
- `assets/icons/macos/QuickPresenter.icns`

The PNG set is intended for Linux and generic packaging flows. The Windows ICO contains 16, 24, 32, 48, 64, 128, and 256 px entries. The macOS ICNS contains the standard iconset entries from 16 px through 1024 px.

The older `assets/icon.png` file is kept unchanged until packaging and runtime icon wiring are migrated explicitly.

## Regeneration

Run the generator after replacing the source PNG:

```sh
python3 scripts/generate_icons.py
```

The generator requires Pillow:

```sh
python3 -m pip install Pillow
```

The script writes the PNG set, Windows ICO, and macOS ICNS from the same source PNG.
