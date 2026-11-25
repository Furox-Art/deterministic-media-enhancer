# Agent command reference

All commands are local and offline after dependencies are installed. Linux is the tested platform.

| Command | Purpose |
| --- | --- |
| doctor | Check engine and local dependencies |
| analyze | Report heuristic metrics and deterministic candidate selection |
| enhance | Restore/upscale, validate and hash a new file |
| verify-repeat | Compare two complete outputs without keeping them |

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
