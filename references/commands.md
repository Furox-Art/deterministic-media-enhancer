# Agent command reference

All commands are local and offline after dependencies are installed. Linux is the tested platform.

| Command | Purpose |
| --- | --- |
| doctor | Check engine and local dependencies |
| analyze | Report heuristic metrics and deterministic candidate selection |
| enhance | Restore/upscale, validate and hash a new file |
| verify-repeat | Compare two complete outputs without keeping them |
| batch | Preflight and sequentially process explicit jobs |

Run python3 scripts/agent.py COMMAND --help for arguments. Optional --engine before COMMAND chooses an explicit local engine executable. The default is build/skill/dme relative to the skill directory, independent of the current working directory.

## Presets

auto: conservative analysis, candidate evaluation and fallback.
clean: spatial denoising + sharpening.
detail: controlled sharpening.
resize: Lanczos only.
restore: deblocking, debanding, denoising and sharpening.
denoise: non-local means.
regional: local threshold-based smoothing.
deblur: approximate fixed-kernel inverse filtering.
wavelet: three-level wavelet denoising.
temporal: temporal averaging (video only).
stabilize: block-motion stabilization (video only).
multiframe: experimental 2x video fusion; requires original FPS, near-zero-start CFR, at least 3 frames, at most 2073600 input pixels, dimensions >=32x32.

Scale: 1, 2, 4, 8. FPS: keep, 30, 60, 120 (video only, up-conversion for supported CFR sources). Output: PNG for images; MKV or MP4 for video. Inputs: PNG/JPEG/BMP/TIFF/WebP or MP4/MKV/MOV/AVI/WebM. A supported extension does not guarantee that every variant is decodable.

## Results and limits

JSON includes status, operation, engine/tool versions where available, reports and SHA-256 for successful enhance jobs. The wrapper is deterministic in its orchestration, not a universal cross-platform bit-exactness guarantee. The native engine uses conservative sampled analysis and image luma guards; video guards check decoded integrity, timing and geometry. Visual quality and audio/video sync are not guaranteed. Color conversion, HDR, alpha and metadata can change; subtitles are not preserved. Audio is re-encoded. MP4 is lossy. Still-image output QA is limited to 64 Mi-pixels. Read README.md for detailed constraints. For forensic or archival work retain the original and do not treat restored detail as evidence of lost information.

## Verification and execution limits

Global option: `--timeout-seconds 3600` before the subcommand (1–86400). Each subprocess is individually limited and its combined captured stdout/stderr must not exceed 8 MiB. The wrapper terminates its process group on timeout or overflow. Required image geometry/video timeline reports must be unique and accepted; duplicate keys, non-finite numbers and inconsistent luma/fallback reports are rejected. Results include `engine_sha256` and `tool_sha256` for FFmpeg/FFprobe. These executable hashes do not establish full machine or library identity.

## Batch manifest

```json
{"jobs":[{"input":"photo.jpg","output":"photo-restored.png","scale":2},{"input":"clip.mp4","output":"clip-restored.mkv","preset":"temporal","fps":"keep"}]}
```

Run `python3 scripts/agent.py batch --manifest jobs.json --dry-run`, then without dry-run. Relative paths resolve against the manifest directory; absolute paths are allowed. The manifest is limited to 1 MiB and 1–100 jobs, in listed order. Each requires input/output; scale defaults to 1, preset to auto, fps to keep. FPS must be a string. All output parents must exist; duplicate canonical targets, unknown keys, existing outputs and invalid options reject the entire plan. Failed renders are isolated, later jobs run, successful files remain; status error and exit 1 indicate at least one failure. Dry-run creates no output and does not certify decodability. Results include ordered jobs, totals and per-job hashes or errors. The wrapper stages output until verification is complete and uses atomic no-replace publication. Hardlink-capable destination filesystem required.
