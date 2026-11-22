# Audience Live README demos

The demo material in this directory, including the synthetic comment text in
`comments.json`, is provided under **GPL-3.0-only**, the repository's [license](../../../LICENSE).
Use the repository-owned [sample PDF](../../samples/quick-presenter-demo.pdf)
and its [Markdown source](../../samples/quick-presenter-demo.md) when recording.
The reaction artwork comes from `assets/audience/reactions/` in this repository.
These are new recordings following the Official Relay Shop demo approach;
Shop media and real attendee messages are not reused.

## Recorded assets (2026-11-22)

Both GIFs show the real macOS Audience Window from the published v3.0.0 RC1
Developer ID package, with application source
`3fd02c9a914b120a7eb4b7cd949ecc38e951c458`. The installed executable SHA-256 is
`f30dfde1822249765e51efc2be3d223c6f6824ac61c224c0202fad511060aa0b`.
The app and PDF renderer were not modified. See the
[RC1 package qualification](../../validation/RC1_DIRECT_CHANNELS_20261122.md)
for the package identity; this recording adds no service or platform qualification.

Capture used app-scoped native screenshots with recorded frame timestamps:
511 source frames over 36.039 seconds for flowing comments and 567 over 36.017
seconds for bubbles. Each capture includes the complete 27-second event window.
Frames were sampled at 10 fps against the sender's recorded start time, using
the nearest real screenshot. Both sessions accepted all ten comments and ten
reactions. The recording-only sessions and application were closed afterward.

The native 1024 × 576 window is cropped by 32 pixels at the top to remove the
macOS capture indicator, then scaled to 960 × 510. FFmpeg encodes 270 frames
per GIF, 27 seconds, infinite loop, 96-color palette, Bayer dithering (scale 3).
Only cropping, scaling, frame sampling, and palette conversion were applied;
comments, reactions, and their positions were not reconstructed or retouched.
The posters are real frames at 8.4 seconds (flowing) and 17.6 seconds (bubbles).

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| `flowing.gif` | 4,538,342 | `695ba41579bd16913d61aff11b4e408f8678aeaacf9e93cfc9dc41ec11979e33` |
| `bubbles.gif` | 1,048,144 | `16b4c084a7629a39897b3a741dab2f3488d7a5d2576f44e79ca50fcba5f96a1f` |
| `flowing.jpg` | 52,165 | `821924546f27baf70eecef13f25495040a83645d16cacd59ab8de025a160c654` |
| `bubbles.jpg` | 51,340 | `fbc38773724bb299149bd38d4b09b94f2f6f9f9f9f1f22de4288b935705d37ee` |

Visual review of decoded GIF frames checked Japanese glyphs, anonymous labels,
flowing motion, bubble entry/stacking, all five reaction kinds, and the loop
reset. The bubble stack remains visible until the loop restarts, matching the
application's behavior. No QR code, join URL, credentials, presenter notes, or
other applications appear in the published images.

## Recording recipe

Record two versions of the real Audience Window, with the same deck, size, and
synthetic event schedule:

- **Flowing:** Settings > Comment display > Flowing comments ON, Bubble comments OFF.
- **Bubbles:** Settings > Comment display > Flowing comments OFF, Bubble comments ON.

Use a dedicated temporary owner-only `QP_PROFILE_DIR` and, on Unix, a separate
owner-only `XDG_RUNTIME_DIR` for the recording GUI and matching CLI. Keep its
configuration and logs inside that temporary profile. Use local hosting on
a trusted private LAN, start Audience Live, and advance past the join screen before capturing.
Disable the experimental sticky QR. Capture only the Audience Window; exclude
join links, QR codes, presenter notes, capture indicators, and other applications.
End the recording-only session and close the recording app afterward.

The event sender needs Node.js and `ws`. To keep dependencies out of this repo:

```sh
npm install --prefix /tmp/qp-demo-tools ws
NODE_PATH=/tmp/qp-demo-tools/node_modules node scripts/send_audience_demo.cjs
```

Paste the dedicated local join URL (including its `#k=` token fragment) into the
interactive prompt. The sender accepts private-LAN and loopback HTTP URLs only.
The prompt hides the pasted URL and restores the original terminal mode on
completion or interruption. The URL is not saved in the repository. After
authentication the script counts down three seconds, then prints START. Capture 27 seconds from
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
