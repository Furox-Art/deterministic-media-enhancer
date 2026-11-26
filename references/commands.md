# Agent command reference

All commands are local and offline after dependencies are installed. Linux is the tested platform. The native engine uses Rust and its standard library; Cargo builds offline without external crates.

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
patch: experimental single-patch transform denoising using the BM3D basic estimator (group=1).
bm3d: native two-stage BM3D-family integer Walsh-Hadamard variant, not the canonical reference implementation. Both require at least 32x32 pixels, convert to 8-bit YUV 4:4:4, operate on luma and may remove real texture. Native bm3d supports at most 2,073,600 source pixels; video must be near-zero-start CFR with at least three frames. JSON reports frames, processed_frames and sigma_sum. Neither is included in auto.
temporal: temporal averaging (video only).
stabilize: block-motion stabilization (video only).
multiframe: experimental 2x video fusion; requires original FPS, near-zero-start CFR, at least 3 frames, at most 2073600 input pixels, dimensions >=32x32.

Scale: 1, 2, 4, 8. FPS: keep, 30, 60, 120 (video only, up-conversion for supported CFR sources). Output: PNG for images; MKV or MP4 for video. Inputs: PNG/JPEG/BMP/TIFF/WebP or MP4/MKV/MOV/AVI/WebM. A supported extension does not guarantee that every variant is decodable. Every external media input is restricted to the demuxer family matching its extension; this is not an OS sandbox. Doctor checks the expected engine version and FFmpeg capabilities.

## Results and limits

JSON includes status, operation, engine/tool versions where available, reports and SHA-256 for successful enhance jobs. The wrapper is deterministic in its orchestration, not a universal cross-platform bit-exactness guarantee. The native engine uses conservative sampled analysis and image luma guards; video guards check decoded integrity, timing and geometry. Visual quality and audio/video sync are not guaranteed. Color conversion, HDR, alpha and metadata can change; subtitles are not preserved. Audio is re-encoded. MP4 is lossy. Still-image output QA is limited to 64 Mi-pixels. Read README.md for detailed constraints. For forensic or archival work retain the original and do not treat restored detail as evidence of lost information.

## Verification and execution limits

Global option: `--timeout-seconds 3600` before the subcommand (1–86400). Each subprocess is individually limited and its combined captured stdout/stderr must not exceed 8 MiB. The wrapper terminates its process group on timeout or overflow. Required image geometry/video timeline reports must be unique and accepted; duplicate keys, non-finite numbers and inconsistent luma/fallback reports are rejected. Results include `engine_sha256` and `tool_sha256` for FFmpeg/FFprobe. These executable hashes do not establish full machine or library identity.

## Advanced presets

`blind` and `motion-deblur`: bounded 8-bit inverse restoration; explicit only, minimum 32x32, maximum 2,073,600 source pixels, CFR video with near-zero start. Abstention possible. `multiframe-local` and `multiframe-gpu`: 2x, keep FPS, same source limits as multiframe; local five-frame fusion. GPU helper must be installed with scripts/build-gpu.sh; local registration, cubic, patch rejection and blending use CUDA; codecs and other presets remain CPU. `precision`: 16-bit stills and supported planar 10/12/16-bit MKV output, keep FPS, native-depth scaling and color tag checks. 1x checks exact decoded samples; rejects unsupported side data. `hdr10`: 10-bit BT.2020/PQ video with constant mastering display and content-light metadata on every frame; lossless libx265 MKV, keep FPS, exact native-depth scaled sample and static metadata checks. Rejects other HDR side data except unregistered H.26x vendor SEI (not preserved). Do not use legacy restoration presets to promise HDR/alpha preservation.

## Cross-hardware comparison

Use `python3 scripts/repro.py prepare --fixtures NEW_DIRECTORY` once. Copy the three fixture files unchanged and build identical source on each authorized machine. On each, run `python3 scripts/repro.py record --fixtures DIRECTORY --output NEW_MANIFEST.json`, then compare the two reports with `python3 scripts/repro.py compare A.json B.json`. Only generated fixture media are processed. The 26 CPU cases each run twice. The integer kernel also has a fixed SHA-256 reference. The comparator distinguishes source/fixture incompatibility, file/decoded/report differences and recorded toolchain/environment differences. Same-machine tests do not prove cross-hardware determinism; independently confirm the physical devices and keep claims limited to the tested cases.

## Batch manifest

```json
{"jobs":[{"input":"photo.jpg","output":"photo-restored.png","scale":2},{"input":"clip.mp4","output":"clip-restored.mkv","preset":"temporal","fps":"keep"}]}
```

Run `python3 scripts/agent.py batch --manifest jobs.json --dry-run`, then without dry-run. Relative paths resolve against the manifest directory; absolute paths are allowed. The manifest is limited to 1 MiB and 1–100 jobs, in listed order. Each requires input/output; scale defaults to 1, preset to auto, fps to keep. FPS must be a string. All output parents must exist; duplicate canonical targets, unknown keys, existing outputs and invalid options reject the entire plan. Failed renders are isolated, later jobs run, successful files remain; status error and exit 1 indicate at least one failure. Dry-run creates no output and does not certify decodability. Results include ordered jobs, totals and per-job hashes or errors. The wrapper stages output until verification is complete and uses atomic no-replace publication. Hardlink-capable destination filesystem required.

## Paired reference corpus

Run `python3 scripts/corpus.py --manifest pairs.json --output NEW_REPORT.json` on authorized local inputs. Manifest example: `{"cases":[{"input":"noisy.png","reference":"clean.png","preset":"bm3d","scale":1}]}`. Relative paths resolve to the manifest directory; 1–10,000 explicitly listed pairs. It evaluates baseline resize and candidate against the same full grayscale reference, reports SSE/MSE/PSNR, hashes and failures, and rejects inconsistent geometry/frame count or decoded streams over 64 MiB per file. Caller must establish aligned frames and dataset provenance. This is not HDR/alpha/audio quality assessment or perceptual superiority proof.

## Real-reference ZIP kernel sweep

Use `python3 scripts/reference_benchmark.py --engine build/skill/dme --archive AUTHORIZED.zip --size 128 --motion --output NEW.json`. Optional Pillow is only used for QA preprocessing, not by the skill. `--per-class 200` selects at most 200 deterministic hash-ranked files per parent directory; duplicate grayscale references are excluded. The runner measures controlled added integer noise, clean controls and optionally known five-tap motion blur against the same reference. It records archive/source/reference/engine hashes and per-case errors. Small grayscale patches are not full-resolution codec, color or perceptual evaluations. Source scenes may overlap.

## Installed packages

Use `dme build` once (or `dme build --gpu` if CUDA is already available), then `dme doctor`. The installed `dme` command forwards the same JSON options as scripts/agent.py. `dme skill-path` locates SKILL.md. The `bm3d-soft` preset uses a deterministic 50% blend of the native filtered luma with the decoded source luma. It is explicit, not auto.
