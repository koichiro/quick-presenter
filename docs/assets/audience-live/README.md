# Audience Live README demos

The demo material in this directory, including the synthetic comment text in
`comments.json`, is provided under **GPL-3.0-only**, the repository's [license](../../../LICENSE).
Use the repository-owned [sample PDF](../../samples/quick-presenter-demo.pdf)
and its [Markdown source](../../samples/quick-presenter-demo.md) when recording.
The reaction artwork comes from `assets/audience/reactions/` in this repository.
Do not copy the separately distributed Shop recording or real attendee messages.

## Recording recipe

Record two versions of the real Audience Window, with the same deck, size, and
synthetic event schedule:

- **Flowing:** Settings > Comment display > Flowing comments ON, Bubble comments OFF.
- **Bubbles:** Settings > Comment display > Flowing comments OFF, Bubble comments ON.

Use a dedicated temporary owner-only `QP_PROFILE_DIR` and, on Unix, a separate
owner-only `XDG_RUNTIME_DIR` for the recording GUI and matching CLI. Keep its
configuration and logs inside that temporary profile. Use local hosting on
127.0.0.1, start Audience Live, and advance past the join screen before capturing.
Disable the experimental sticky QR. Capture only the Audience Window; exclude
join links, QR codes, presenter notes, capture indicators, and other applications.
End the recording-only session and close the recording app afterward.

The event sender needs Node.js and `ws`. To keep dependencies out of this repo:

```sh
npm install --prefix /tmp/qp-demo-tools ws
NODE_PATH=/tmp/qp-demo-tools/node_modules node scripts/send_audience_demo.cjs
```

Paste the dedicated local join URL into the interactive prompt. It is not
printed by the script or saved in the repository. After authentication the
script counts down three seconds, then prints START. Capture 27 seconds from
START so the last flowing comment can leave the screen. Repeat with a fresh
session for the other style. A failed or timed-out event aborts the schedule
without resending an uncertain message.

Each version sends exactly **ten comments: six English and four Japanese**.
Every handle is `Anonymous`; the application omits this name from the overlay.
Ten reactions cycle through all five supported kinds. The comments and their
timestamps are the editable source in `comments.json`.

Convert the captured frames or video with FFmpeg, preserving elapsed time.
For example, for an Audience Window content recording at 16:9:

```sh
ffmpeg -i flowing.mov -t 27 -filter_complex \
  '[0:v]fps=10,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=96[p];[b][p]paletteuse=dither=bayer:bayer_scale=3' \
  -loop 0 flowing.gif
```

Repeat for `bubbles.gif`. Inspect the actual GIFs at the README display size,
including Japanese glyphs, comment entry/exit, bubble stacking, reactions, and
the loop boundary. Retain static posters for readers who prefer still images.
Record source revision, app/package identity, dimensions, duration, frame count,
and final file hashes alongside the finished assets. Describe them as local
feature demonstrations, not hosted-relay performance or package qualification.
