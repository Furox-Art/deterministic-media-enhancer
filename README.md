# Deterministic Media Enhancer (DME)

An AI agent skill for local, offline image and video enhancement without trained models, cloud services, telemetry or usage fees. No desktop application or GUI. Start with [SKILL.md](SKILL.md).

## v1.0.0-rc1 — experimental transform denoising and format hardening

The Rust CLI uses FFmpeg CPU filters and has no third-party Rust crate dependencies. Supported scales: 1x, 2x, 4x, 8x. Image output: PNG. Video output: FFV1/FLAC MKV or H.264/AAC MP4. Existing outputs are never intentionally overwritten. The CLI is the underlying engine for the agent skill; the supported agent entry point is scripts/agent.py.

| Preset | Operation |
| --- | --- |
| auto (default) | Sample-based defect analysis, candidate comparison and regression guards |
| clean | Spatial denoising, scaling, unsharp |
| detail | Scaling and unsharp |
| resize | Lanczos scaling only |
| restore | Deblocking, fixed-offset debanding, denoising, scaling, unsharp |
| denoise | Non-local means denoising and scaling |
| regional | Threshold-based local smoothing, stronger in flat regions |
| deblur | Damped approximate inverse of a fixed binomial blur kernel plus wider high-boost stage |
| wavelet | Three-level wavelet denoising |
| patch | Experimental single-patch transform denoising (FFmpeg BM3D basic, group=1) |
| bm3d | Experimental grouped basic estimate (group=4), not full two-pass BM3D |
| temporal | Five-frame adaptive temporal denoising (video only) |
| stabilize | Block-motion stabilization (video only) |
| multiframe | Half-pixel global registration and conservative 2x sample fusion (video only) |

## Build on Linux

Dependencies: Rust 1.85+ with Cargo (including the platform linker; tested with Rust 1.99.0), Python 3, FFmpeg and FFprobe (with libx264). A C++ compiler, CMake, Qt, Python package downloads and Rust crate downloads are not required. Build with bash scripts/build.sh; it checks existing local dependencies, builds offline with the lockfile, runs unit and integration tests, and installs the verified native engine at build/skill/dme.

```sh
bash scripts/build.sh
python3 scripts/agent.py doctor
python3 scripts/agent.py analyze --input input.jpg --scale 2
python3 scripts/agent.py enhance --input input.jpg --output output.png --scale 2
python3 scripts/agent.py enhance --input input.mp4 --output output.mkv --scale 2 --preset restore
python3 scripts/agent.py enhance --input input.mp4 --output output60.mkv --preset temporal --fps 60
python3 scripts/agent.py enhance --input input.mp4 --output stable.mkv --preset stabilize
python3 scripts/agent.py enhance --input input.mp4 --output multi.mkv --scale 2 --preset multiframe

```

## Agent interface

See SKILL.md and references/commands.md. Use python3 scripts/agent.py doctor, analyze, enhance or verify-repeat. The wrapper emits one JSON object, reports nonzero failure status, computes SHA-256 of source and output, and supports exact two-run hash comparison. No automatic software installation, network access or output overwrite. The wrapper defaults to 1x and original FPS; the native CLI retains its previous 2x default for compatibility.

## Batch jobs

Use `python3 scripts/agent.py batch --manifest jobs.json --dry-run` to validate an explicit ordered JSON job list without rendering, then omit `--dry-run` to execute. See references/commands.md for the manifest format. Invalid plans (including duplicate canonical targets and existing outputs) abort before any render. Runtime errors are isolated per job; remaining jobs continue, and any failure yields a nonzero exit. Successful outputs remain. The wrapper stages each output privately on the destination filesystem and publishes with a no-replace hard link only after source/engine hashes and required reports pass. Source/engine hash checks detect persistent changes, not adversarial changes restored between checks. No recursive discovery, arbitrary shell commands or automatic output renaming. No universal quality or cross-machine reproducibility guarantee.

## Agent verification and execution limits

Structured reports reject duplicate JSON keys, non-finite numbers (including exponent overflow), malformed objects, conflicting required guards and invalid field types. Every image preset now checks decoded output dimensions against the decoded source dimensions times the selected scale; automatic non-baseline image outputs must also carry a consistent luma/fallback report. Video jobs require exactly one accepted timeline report. The wrapper records SHA-256 for the engine and the FFmpeg/FFprobe executables and checks their hashes again before publication; repeat verification compares these identities as well as source and output hashes. This does not fingerprint dynamically loaded libraries, the operating system or hardware.

The optional global argument `--timeout-seconds` (default 3600, range 1–86400) limits each local subprocess, not the entire batch. Captured stdout and stderr together are limited to 8 MiB per subprocess. On timeout, output overflow or keyboard interruption, the wrapper kills the subprocess group and reaps the parent. Temporary staging is cleaned and unverified outputs are not published. Normal child processes are covered; this is not an operating-system security sandbox. Source files must be trusted and explicitly authorized; symlink races, adversarial source changes restored between checks and indirect local demuxer access are not fully isolated.

## Release-candidate restoration and input validation

The explicit `patch` and `bm3d` presets process 8-bit luma using FFmpeg's BM3D filter, with a minimum input size of 32x32. The patch estimator does not collaborate across similar blocks; the grouped estimator is only the basic pass, not a complete two-stage BM3D implementation. Neither is part of automatic selection. Both can remove real texture and do not preserve HDR/alpha. Local tests found severe distortion with group=8 even at sigma=0, so that configuration is excluded. Use only after inspecting a preview or output; neither preset is universally superior to NLMeans. The native engine restricts every external FFmpeg/FFprobe media input to the demuxer family matching its file extension. This rejects disguised concat/HLS playlists and mismatched containers; it is not a sandbox or a guarantee against all decoder vulnerabilities or indirect local reads. FFmpeg diagnostic lines are separated from strict JSON reports.

Run `python3 tests/quality.py build/skill/dme` for a deterministic synthetic grayscale noise fixture; optionally supply `--reference /path/to/authorized.jpg --artifacts /path/to/new-qa-directory`. The comparison reports MSE, PSNR and mean nonoverlapping 8x8 SSIM against the same clean reference, with clean-input controls for the new filters. Wall-clock timings are observational. It does not measure general perceptual quality, temporal quality or color fidelity. Doctor checks available filters, encoders, muxers, demuxers and the expected engine version.

## Adaptive analysis

Analyzes a native-resolution central crop (up to 512x512 pixels) in 8-bit grayscale. For video, samples every 15th decoded frame, at most 8 frames from the first 10 seconds. Integer-based metrics estimate high-frequency residuals in locally flat regions, aligned 8-pixel discontinuities, edge gradients and curvature. Median metrics determine eligible candidates. The optimizer (src/analysis.rs) evaluates a bounded, fixed-order set of candidate filters on up to four native-resolution 384x384 central crops, then compares them against the unfiltered samples. Integer scoring rewards measured defect reduction; guards limit mean absolute deviation, edge loss/gain, clipping, and increases in noise or block artifacts. A candidate must pass every sampled frame and beat the baseline score by more than 15 points. Ties preserve the earlier candidate. Otherwise, only Lanczos scaling is applied. JSON exposes candidate scores, acceptance decisions, and the selected chain. Candidate filters operate before scaling. The automatic mode also tests a regional filter and an approximate deblur candidate when eligible. The final image is decoded and, when a non-baseline automatic chain wins, compared against a separately encoded Lanczos-only baseline over overlapping 256-pixel tiles at full output resolution. Any tile exceeding guard thresholds triggers whole-image baseline fallback. This checks 8-bit luma, not color, alpha, temporal or perceptual quality. These are heuristic proxies: fine textures, nonaligned blocks, color artifacts and late scene changes may be missed or misclassified. No guarantee of quality improvement or parameter optimality.

## Output integrity and limits

Output is encoded in a private temporary directory beside the destination and published via an atomic hard link without overwriting an existing file. Failed encodes are not published. Output filesystems must support hard links. Still images are fully decoded for integrity with a 64 Mi-pixel limit; automatic non-baseline outputs receive a full-image luma comparison and may fall back to Lanczos. Manual presets are not overridden. Videos are decoded in full to detect decoder errors. Source and output video timelines are checked for dimensions, frame count, duration and frame cadence before publication. This does not certify visual quality, motion consistency or audio fidelity. Verification adds processing time. The new regional, deblur and wavelet filters explicitly convert to 8-bit YUV 4:4:4 and are not suited to alpha or HDR preservation. Deblur assumes a fixed small blur kernel; it is not blind or general motion deconvolution. The regional preset is locally threshold-adaptive, not a user-defined ROI editor.

## Video processing

The optional fifth positional argument after input/output/scale/preset selects `keep` (default), `30`, `60` or `120` FPS. Motion-compensated interpolation uses FFmpeg MCI with scene-change detection, adaptive overlapping block compensation, and cloned end padding trimmed to the expected frame count. Interpolation runs at source resolution before spatial filtering and scaling; synthesized frames are estimates. FPS conversion currently accepts only near-zero-start constant-rate video with at least three frames and dimensions of at least 32x32; down-conversion is not supported. Timestamp validation is based on microsecond integer arithmetic with a 2 ms cadence tolerance; unusual timestamps, geometry changes and missing timing information may be rejected. Full-frame FFprobe metadata is capped at 64 MiB; exceptionally long videos can exceed that limit. The temporal and stabilization presets use 8-bit YUV 4:4:4. Stabilization can create mirrored edge content and interpolation can introduce motion artifacts. The timeline guard is structural, not a perceptual or A/V synchronization guarantee.

## Multi-frame detail fusion

The experimental `multiframe` preset is implemented in Rust using integer arithmetic for registration, interpolation and blending. A three-frame sliding window aligns neighboring frames using global translations searched on an integer and half-pixel grid, with bidirectional checks. Only shifts providing a new sampling phase and sufficiently low matching error are eligible. The 2x output starts from a fixed-point cubic interpolation; compatible neighboring samples contribute to missing pixel phases with a 2:1 weight per observation relative to the baseline. A per-channel residual threshold rejects incompatible samples. Original center-frame RGB samples at even/even coordinates remain untouched before output encoding. If no neighbor qualifies, cubic interpolation is used. This is not a general optical-flow or iterative super-resolution solver and cannot guarantee better quality than Lanczos or recover all lost detail.

Currently limited to near-zero-start CFR videos, 2x spatial scale, original FPS, at least three frames, 32x32 minimum and 2073600 source pixels maximum. Input is decoded to 8-bit RGB without automatic rotation; output uses the existing video encoders and timeline guard. HDR, alpha, rotation metadata, nonrigid motion and large camera motion are not supported by the fusion model. The input is streamed through pipes and memory is bounded by the three source frames and one upscaled output frame. JSON reports accepted neighbors and fused pixels. The test suite uses Python 3 to generate synthetic fixtures.

## Determinism

CPU-only, restricted FFmpeg CPU flags, single-thread processing, bitexact flags, fixed presets, source metadata removed, file/pipe protocol whitelist. Reproducibility tests compare whole-file bytes and analysis reports across repeated runs in the same environment. Tests also check expected decisions on synthetic patterns. Cross-version/hardware bit identity is not guaranteed. Upscaling is not recovery of lost detail. PNG/FFV1 outputs are losslessly encoded after pixel conversion and filters, not necessarily source-preserving. MP4 is lossy. Alpha processing, HDR/color management, orientation, VFR, and uncommon media require further validation. Audio is re-encoded; subtitles and metadata are not preserved.

## Planned features

- User-defined ROI and scene-adaptive restoration, chroma/alpha and temporal quality validation.
- Blind/motion deconvolution, validated two-pass BM3D and more advanced local motion and iterative multi-frame reconstruction.
- Agent-driven progress, cancellation and expanded validation.
- Advanced scene-aware motion restoration, color management and higher bit-depth validation.
- Reproducible toolchain and cross-platform builds; GPU only after determinism validation.

No network access is needed by the application itself for local inputs. Installing dependencies and using GitHub may require internet access.

## License

[MIT](LICENSE).
