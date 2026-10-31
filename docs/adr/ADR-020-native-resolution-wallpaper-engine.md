# ADR-020: Native-resolution, hardware-decoded wallpaper engine

- **Status**: accepted (2026-10-31; native Windows validation deferred / release-blocked)
- **Date**: 2026-10-31
- **Phase**: Phase 3+ (appearance/theme)
- **Supersedes**: [ADR-017](ADR-017-cross-platform-video-decode.md)
- **Related**: platform guide §1.4 and §5.7; [ADR-014](ADR-014-theme-v2-appearance-skin.md); `docs/ui-design/13_NOTES.md`

## Context

ADR-017 avoided software decoding of oversized H.264 by generating a 1080p H.264
proxy. That traded away the user's source resolution and left a large recovery
system in the frontend: poster capture and IndexedDB persistence, route-based
compositor nudges, focus listeners, watchdog timers, repeated `load()` calls and
a proxy-upgrade source swap. Those mechanisms made playback lifecycle dependent
on routing and could themselves cause stalls, flashes and stale asynchronous
updates.

A local 4126×2596, 30000/1001 fps H.264 sample fails VideoToolbox initialization.
The same sample converted with macOS `avconvert PresetHEVCHighestQuality` retains
4126×2596, 30000/1001 fps, its exact frame count and duration, and decodes through
VideoToolbox. A native-resolution HEVC fallback therefore solves the decoder
limit without reducing wallpaper quality.

## Goals

1. Play local 4K and moderately higher-resolution wallpapers at native resolution.
2. Keep decode and composition on the system WebView GPU path on macOS and Windows.
3. Use one platform-neutral policy, cache and validation implementation; platform
   backends only execute media transcoding.
4. Replace playback recovery machinery with one persistent HTML video element.
5. Fail visibly when a hardware-safe asset cannot be prepared; never silently
   select a known software-decode fallback.
6. Add no multimedia runtime or large binary dependency.

HDR tone mapping, online streams, fragmented MP4 transcoding and Linux VA-API
support are outside this slice. Actual hardware support still depends on the
machine's GPU, drivers and installed codecs.

## Options

### A. Keep the 1080p H.264 proxy

This is compatible but violates the native-resolution requirement and retains an
unnecessary source upgrade in the frontend. Rejected.

### B. Bundle FFmpeg, mpv or a custom native texture pipeline

This would add a large runtime, platform-specific GPU interop, color management,
window lifecycle and distribution work. It conflicts with the 20 MiB binary budget
and the requirement to implement the feature in the simplest efficient way.
Rejected.

### C. System WebView playback plus native-resolution HEVC preparation (adopted)

The existing system WebViews remain the decoder and compositor. Only H.264 sources
outside the conservative 4096×2304 hardware envelope are converted. Conversion
keeps native dimensions and the original timing. macOS uses the system `avconvert`;
Windows uses WinRT `MediaTranscoder` with hardware acceleration required.

## Decision

### Shared engine

`rmccc-platform::video` owns MP4 probing, playback decisions, cache identity,
unique staging files, stale-file cleanup and output validation.

- H.264 at or below 4096×2304 and HEVC sources play directly without a copy.
- Oversized H.264 is converted once to native-resolution HEVC in MP4.
- Other MP4 codecs are rejected with an explicit unsupported-codec error rather
  than being represented as hardware-decoded.
- The cache recipe is `hevc-native-timing-v1`; old 1080p entries cannot match.
- Output codec, exact width/height, exact rational frame rate and exact frame
  count must match the contract. Duration may differ by at most 200 ms only for
  container timestamp rounding.
- Staging names include process ID and a process-local sequence. Existing valid
  cache entries win races; fresh temporary files are never shared or deleted by
  another preparation in the same process.

### Platform backends

- **macOS**: `/usr/bin/avconvert -p PresetHEVCHighestQuality`. The backend does not
  specify a target size or frame rate; validation enforces the contract.
- **Windows**: `MediaEncodingProfile::CreateHevc`, explicit source dimensions and
  rational frame rate, `SetAlwaysReencode(true)` and
  `SetHardwareAccelerationEnabled(true)`. Failure to enable or prepare the
  hardware path is an error, not a silent software-transcode fallback. HEVC codec
  availability is a machine prerequisite and is checked by `CanTranscode`.
- **Other platforms**: the same engine API returns `UnsupportedPlatform` when
  conversion is required. A backend can be added without changing UI or IPC.

### Tauri and frontend

`background_video_prepare(path)` is the single video-preparation IPC. It probes,
prepares, grants asset access to the exact returned file and returns a DTO with
path, dimensions, codec, mode and backend. The separate allow command is removed.
The picker only returns the canonical selected path.

The frontend starts preparation from `main.tsx`, caches the in-flight promise per
source path and renders one persistent muted, looping, inline video. Route changes
never access it. There are no posters, IndexedDB frame copies, watchdogs,
compositor nudges or forced `load()` resets. An `ended` handler rewinds and plays;
visibility changes pause/resume. Decode and preparation failures are shown in
Settings. While preparing an oversized source, no software-decoded source is
shown as a temporary fallback.

### Addendum: still-image wallpaper (2026-10-31)

Static image wallpapers reuse the same asset-scope contract instead of introducing
an image runtime or a second security model. `dialog_pick_background_image`
returns a canonical path; `background_image_prepare(path)` validates that it is a
regular PNG/JPEG/WebP/GIF/BMP/AVIF file of at most 32 MiB and grants asset access
to exactly that file. The frontend renders one `object-fit: cover` image layer
above the video layer and samples it first for the adaptive palette. Decode and
scaling stay in the system WebView, so no new codec dependency enters the binary.

The image and video wallpapers are mutually exclusive in Appearance: enabling or
picking one turns the other off. The image layer nevertheless paints above the
video layer so an inconsistent stored preference cannot hide the selected image.
A still image does not honor the motion-effects switch and does not activate the
video performance fallback, because it is decoded once and never repaints.

## Consequences

- Native dimensions and timing are preserved; no 1080p quality loss.
- Direct sources incur only a small MP4 metadata read on startup.
- The first oversized-source preparation still requires a full transcode and can
  use substantial temporary disk space. Later launches reuse the validated cache.
- HEVC playback on Windows may require the Microsoft HEVC codec and a compatible
  GPU/driver. Missing support must produce a visible error; this design cannot
  honestly promise hardware HEVC on every Windows installation.
- No new cross-platform dependency is added; the existing Windows-only `windows`
  crate remains target-gated.
- Windows native compilation, transcoding and WebView2 GPU verification remain
  deferred / release-blocked until a native Windows runner is available.

## Migration

The recipe version prevents old 1080p cache entries from being selected. Users can
remove old entries through Settings → Storage. The removed `background_video_allow`
IPC is replaced by preparation's atomic grant-and-return behavior.

## Validation

- Shared unit tests cover direct/transcode decisions, native dimensions, exact
  timing validation, invalid cache rejection and unique staging names.
- macOS integration coverage uses an oversized real MP4 and checks native HEVC
  output plus VideoToolbox decode.
- Windows native CI must compile the WinRT backend, verify HEVC output timing and
  dimensions, and confirm WebView2 Video Decode GPU activity on representative
  Intel, AMD and NVIDIA systems before release sign-off.

## Status history

- 2026-10-31: accepted; supersedes ADR-017. macOS oversized native-resolution
  conversion and VideoToolbox feasibility verified; Windows native validation
  deferred / release-blocked.
- 2026-10-31: added the still-image wallpaper contract (`background_image_prepare`,
  top image layer, mutual exclusivity with video, adaptive sampling priority).
