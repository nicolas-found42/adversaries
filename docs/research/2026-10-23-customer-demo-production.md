# Customer demo production with open-source tooling

Researched 2026-10-23. Scope: a polished introductory video of Adversarial Council on macOS, showing actual UI interaction and a real model-backed debate. This is a production recommendation; software capabilities below come from the linked maintainers, while narrative, timing, and visual choices are editorial judgments.

## Recommendation

Record a real native-app session, retain the unedited capture and associated session diagnostics, and produce an approximately 90-second customer cut with clear chapter captions and restrained camera moves. Use the existing FFmpeg installation for capture and finishing if its AVFoundation capture works under current permissions. This is the shortest dependency path. Use OBS as the established capture fallback. A dedicated recorder is optional polish, not a prerequisite.

FFmpeg documents macOS AVFoundation screen input, device enumeration, explicit frame rate, cursor capture, and click capture. Cursor and click capture are disabled by default, so request them deliberately and test a short clip first. The input index must come from the actual machine's device list. [AVFoundation documentation](https://ffmpeg.org/ffmpeg-devices.html#avfoundation)

For an interactive all-in-one recorder, the strongest permissively licensed candidate found is **OpenScreen** (`getopenscreen/openscreen`, MIT), which advertises native capture, animated zooms, captions, and GPU export. It should be evaluated with a short disposable recording before trusting a live take. This is a different project from AGPL-licensed **OpenScreenStudio**. [OpenScreen repository](https://github.com/getopenscreen/openscreen), [MIT license](https://github.com/getopenscreen/openscreen/blob/main/LICENSE)

## Discovery and source policy

The requested GitHub/awesome-list search included [Awesome Mac's screen-recording section](https://github.com/jaywcjlove/awesome-mac#screen-recording), [Awesome Video](https://github.com/sitkevij/awesome-video), and [Awesome Video as Code](https://github.com/voidforall/awesome-video-as-code). These identify capture, editing, and programmable-animation projects; a list's inclusion does not establish its license or reliability. Capability and license decisions below were checked against the projects' own repositories and documentation.

## Tool comparison

| Tool | License and primary evidence | Fit for this task / limitation |
| --- | --- | --- |
| FFmpeg | LGPL-2.1-or-later by default; optional GPL components change the build's license. [Legal page](https://ffmpeg.org/legal.html) | Existing command-line capture/compositing/encoding path; precise and reproducible, but camera choreography must be authored. Inspect the installed build rather than assuming its configuration. |
| OBS Studio | GPL-2.0. [Repository](https://github.com/obsproject/obs-studio), [license](https://github.com/obsproject/obs-studio/blob/master/COPYING) | Mature macOS capture, scenes, and local recording; editing/automatic cursor zoom requires a separate step. |
| OpenScreen | MIT. [Repository](https://github.com/getopenscreen/openscreen), [license](https://github.com/getopenscreen/openscreen/blob/main/LICENSE) | Purpose-built screen demos with camera movement, captions, and local export; best recorder candidate to trial if FFmpeg capture is awkward. |
| OpenScreenStudio | AGPL-3.0. [Repository](https://github.com/Glyph-Software/OpenScreenStudio), [license](https://github.com/Glyph-Software/OpenScreenStudio/blob/main/LICENSE) | Autozoom, cursor overlays, backgrounds, camera/audio tracks, MP4/GIF. Maintainer warns releases are not notarized; recording path needs macOS 15+. Adds setup/permissions risk. |
| Cap | Mostly AGPL-3.0, with MIT `cap-camera*`/`scap-*` crates. [Repository](https://github.com/CapSoftware/Cap), [license map](https://github.com/CapSoftware/Cap/blob/main/LICENSE) | Studio mode records locally and edits backgrounds/zooms/captions. Sharing/AI cloud services are separate from the local OSS production path. Do not call the entire project MIT. |
| Screenity | GPL-3.0 for current extension. [Repository and features](https://github.com/alyssaxuu/screenity), [license](https://github.com/alyssaxuu/screenity/blob/master/LICENSE) | Chrome extension supports tab/desktop/application capture, annotation, pointer emphasis and export. Local extension is usable offline. Screenity Pro is a separate hosted service; avoid assuming its multi-scene features belong to the OSS extension. |
| Motion Canvas | MIT. [Repository/license](https://github.com/motion-canvas/motion-canvas), [rendering guide](https://motioncanvas.io/docs/rendering/) | TypeScript animation and a live editor are well suited to bespoke intro/outro and explanatory motion graphics; more setup than needed for a single screen-led edit. |
| Editly | MIT. [Repository](https://github.com/mifi/editly), [license](https://github.com/mifi/editly/blob/master/LICENSE) | Declarative CLI timeline for clips, titles and transitions; an optional wrapper if a reusable branded series is desired. |
| Remotion | Custom Remotion License with entity-size-dependent commercial terms. [Actual license](https://github.com/remotion-dev/remotion/blob/main/LICENSE.md) | Exclude from this production's strict open-source dependency set. Public source and free use for some entities do not satisfy the stated constraint. |
| ScreenToGif | MS-PL; .NET Desktop/WPF application. [Repository](https://github.com/NickeManarin/ScreenToGif), [license](https://github.com/NickeManarin/ScreenToGif/blob/master/LICENSE.txt) | Useful Windows recorder/editor, not the appropriate macOS capture route. |

These are production tools, not dependencies to add to the shipped Council application. Retain a small production dependency/license manifest with the deliverable; check fonts, voices and music separately from recorder licenses.

## Story and visual treatment

Editorial recommendation: the customer should understand the outcome before learning the mechanics. Introduce Council as a way to pressure-test a decision through an opening case, independent critiques, a rebuttal, and a decision brief. Avoid claiming that model agreement proves correctness. Show a fictional but credible business decision with explicitly supplied facts, tradeoffs and success criteria; avoid customer data or provider credentials.

Suggested 90-second structure:

1. **0–7 seconds — Value.** Product title and a concrete question: “Should we launch a self-service pilot?” One sentence explains the benefit: examine the case, challenge assumptions, leave with a decision brief.
2. **7–24 seconds — Set up.** Show a new session, enter the decision, attach a short prepared context file, and choose the council. Briefly show role/model dropdowns so viewers understand the controls without enumerating settings.
3. **24–38 seconds — Run.** Press the real run button. Show actual opening and stage progress. Label compressed waiting time explicitly; retain the full real-time original separately.
4. **38–60 seconds — Challenge.** Read one consequential critique at a legible zoom; show the rebuttal addressing it. Captions explain why that exchange changes the decision rather than reciting all output.
5. **60–79 seconds — Decide.** Show the real completed brief: recommendation, conditions, risks, and next actions. Hold enough time to read the essential excerpt.
6. **79–90 seconds — Keep the work.** Reopen the saved session or copy/export the actual brief using a feature present in the UI, then close with a simple product/value card.

Use a dark neutral stage and one accent color consistent with the app. Keep the UI as the dominant object, with a modest border/shadow and generous outer margin. Move the camera only to guide attention. Preserve original text proportions; avoid stretching or a decorative device mockup that makes the app unreadable. Captions should explain user intent and stage purpose. Hover pauses and deliberate scrolling are more legible than rapid clicks.

A silent-first cut with strong captions is a complete deliverable; narration is optional. If speech is added, an OSS candidate is Kokoro: its inference library and advertised model weights are Apache-2.0. Its example pipeline depends on PyTorch, phonemization and eSpeak fallback, so installation and model download increase production time. Use a provided stock voice, not an impersonation. [Kokoro repository and usage](https://github.com/hexgrad/kokoro), [model card/license](https://huggingface.co/hexgrad/Kokoro-82M)

## Practical capture and finishing plan

Before spending inference calls, run a 5–10-second capture probe: confirm the app surface is present, cursor is visible, no permission dialog overlays it, and text remains sharp. Screen-recording permission is a real macOS integration requirement; grant only through the normal OS flow if requested. Do not build the take around a recorder that has not passed this probe.

For FFmpeg, enumerate devices with `ffmpeg -f avfoundation -list_devices true -i ""`. Capture the relevant screen at its native resolution, explicit 30 fps, and cursor enabled; crop to the app during finishing if the input is a whole display. Use a high-quality intermediate and check disk capacity before the run. These options are documented by FFmpeg; actual capture availability must be tested locally. [AVFoundation options and examples](https://ffmpeg.org/ffmpeg-devices.html#avfoundation)

FFmpeg's crop/scale/overlay/drawtext filters can compose the app on a designed background; `setpts` changes waiting speed and `xfade` provides restrained transitions. Normalize frame rate, resolution, pixel format and time base before crossfading. Captions may be burned into the presentation cut; also provide an SRT track for accessibility. [Filter reference](https://ffmpeg.org/ffmpeg-filters.html)

Suggested outputs are 1920×1080 H.264 MP4, 30 fps, YUV420p with fast-start metadata for common playback; an optional higher-resolution master; a poster/contact sheet; captions; and the source edit specification. Preserve the raw capture rather than repeatedly compressing it. This format choice is an interoperability/editorial recommendation, not a claim that every recorder exports identical encoding settings.

## Integrity and acceptance criteria

Record one real complete debate with the selected provider/model configuration, retaining the prior six-call ceiling unless the user changes it. Never rerun inference solely to retake a cursor movement: capture additional UI navigation against that completed saved session instead. Clearly distinguish this replay footage from the initial real execution in the production log.

Preserve the run ID, model IDs, call count, stage completion, finish reasons, reasoning policy, token usage and errors in the local session evidence. The presentation need not show technical diagnostics at length, but claims about a real run must be backed by them. Do not imply that a speed-ramped clip represents the actual elapsed time or that requested reasoning effort is provider-confirmed if diagnostics do not confirm it.

The finished customer cut passes only if a newcomer can identify the product's purpose, see the full input-to-brief flow, read the main result, and understand where the model challenge changed the analysis. Inspect beginning/middle/end frames, transitions, subtitles and final output playback. Confirm no credentials, unrelated windows, notifications, private histories or fabricated output entered the recording. Keep local evidence distinct from the customer-facing package; sending the video to customers is a separate user action.

## Implemented production and live evidence

The delivered production uses a small Swift window recorder with macOS ScreenCaptureKit and AVFoundation, then FFmpeg, Pillow and NumPy for the edit. The recorder targets only the isolated Council window, avoiding unrelated desktop content. Swift is open source; the existing macOS installation supplies its system capture APIs. No additional proprietary production application was installed. [Swift license](https://github.com/swiftlang/swift/blob/main/LICENSE.txt), [ScreenCaptureKit documentation](https://developer.apple.com/documentation/screencapturekit)

Narration uses the stock `af_sarah` voice through **kokoro-onnx** (MIT), with Kokoro-82M model weights (Apache-2.0). It does not imitate a named person. The score is an original procedural ambient composition generated with NumPy; no music recording was downloaded. Typography is Inter under SIL OFL 1.1. [kokoro-onnx](https://github.com/thewh1teagle/kokoro-onnx), [Kokoro model card](https://huggingface.co/hexgrad/Kokoro-82M), [Inter license](https://github.com/google/fonts/blob/main/ofl/inter/OFL.txt)

One fresh live run completed on 2026-10-23 with six requests, one attempt per stage, and no retries: opening, three critiques, rebuttal, and decision brief. Every response identified `deepseek-v4.1-flash` through OpenCode Go and finished with `stop`. Journal timestamps span approximately 108 seconds. Reported usage totals: 26,510 input tokens and 21,594 generated tokens, including 12,380 reasoning tokens. Generated tokens already include reasoning; do not add them again. The minimum documented `low` effort was requested. The provider does not echo an applied effort or supply cost; neither is presented as verified. Unknown output ceilings stayed at provider defaults, with no 4,000-token override. The largest response generated 4,647 tokens.

The customer cut labels shortened waiting time and uses the genuine returned recommendation and critique. The launch scenario is fictional. Raw footage, the session journal, snapshots, edit source, license notes and a per-stage verification table remain in the separate local production workspace, outside the customer delivery folder. No inference was repeated for editorial pickups. A `COUNCIL_REPORTS_DIR` environment override isolates this demo's history while preserving the app's existing default reports location.
