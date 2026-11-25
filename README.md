# Deterministic Media Enhancer (DME)

Local, offline image and video enhancement without trained models, cloud services, telemetry or usage fees.

## v0.4.0 — regional restoration and output validation

C++17 CLI uses FFmpeg CPU filters. Supported scales: 1x, 2x, 4x, 8x. Image output: PNG. Video output: FFV1/FLAC MKV or H.264/AAC MP4. Existing outputs are never intentionally overwritten. Qt 6 GUI source is optional; the CLI is the tested application.

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

## Build on Linux

Dependencies: CMake >=3.20, C++17 compiler, FFmpeg and FFprobe (with libx264). Qt 6 Widgets development package is optional.

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j
ctest --test-dir build --output-on-failure
./build/dme --analyze input.jpg 2
./build/dme input.jpg output.png 2 auto
./build/dme input.mp4 output.mkv 2 restore
./build/dme-gui # if Qt6 Widgets was found
```

## Adaptive analysis

Analyzes a native-resolution central crop (up to 512x512 pixels) in 8-bit grayscale. For video, samples every 15th decoded frame, at most 8 frames from the first 10 seconds. Integer-based metrics estimate high-frequency residuals in locally flat regions, aligned 8-pixel discontinuities, edge gradients and curvature. Median metrics determine eligible candidates. The optimizer (src/optimizer.hpp) evaluates a bounded, fixed-order set of candidate filters on up to four native-resolution 384x384 central crops, then compares them against the unfiltered samples. Integer scoring rewards measured defect reduction; guards limit mean absolute deviation, edge loss/gain, clipping, and increases in noise or block artifacts. A candidate must pass every sampled frame and beat the baseline score by more than 15 points. Ties preserve the earlier candidate. Otherwise, only Lanczos scaling is applied. JSON exposes candidate scores, acceptance decisions, and the selected chain. Candidate filters operate before scaling. The automatic mode also tests a regional filter and an approximate deblur candidate when eligible. The final image is decoded and, when a non-baseline automatic chain wins, compared against a separately encoded Lanczos-only baseline over overlapping 256-pixel tiles at full output resolution. Any tile exceeding guard thresholds triggers whole-image baseline fallback. This checks 8-bit luma, not color, alpha, temporal or perceptual quality. These are heuristic proxies: fine textures, nonaligned blocks, color artifacts and late scene changes may be missed or misclassified. No guarantee of quality improvement or parameter optimality.

## Output integrity and limits

Output is encoded in a private temporary directory beside the destination and published via an atomic hard link without overwriting an existing file. Failed encodes are not published. Output filesystems must support hard links. Still images are fully decoded for integrity with a 64 Mi-pixel limit; automatic non-baseline outputs receive a full-image luma comparison and may fall back to Lanczos. Manual presets are not overridden. Videos are decoded in full to detect decoder errors, not to certify visual quality, motion consistency or audio fidelity. Verification adds processing time. The new regional, deblur and wavelet filters explicitly convert to 8-bit YUV 4:4:4 and are not suited to alpha or HDR preservation. Deblur assumes a fixed small blur kernel; it is not blind or general motion deconvolution. The regional preset is locally threshold-adaptive, not a user-defined ROI editor.

## Determinism

CPU-only, restricted FFmpeg CPU flags, single-thread processing, bitexact flags, fixed presets, source metadata removed, file/pipe protocol whitelist. Reproducibility tests compare whole-file bytes and analysis reports across repeated runs in the same environment. Tests also check expected decisions on synthetic patterns. Cross-version/hardware bit identity is not guaranteed. Upscaling is not recovery of lost detail. PNG/FFV1 outputs are losslessly encoded after pixel conversion and filters, not necessarily source-preserving. MP4 is lossy. Alpha processing, HDR/color management, orientation, VFR, and uncommon media require further validation. Audio is re-encoded; subtitles and metadata are not preserved.

## Planned features

- User-defined ROI and scene-adaptive restoration, chroma/alpha and temporal quality validation.
- Blind/motion deconvolution, BM3D and multi-frame restoration.
- Preview, batch jobs, progress, cancellation and advanced controls.
- Frame interpolation, stabilization, color management and higher bit-depth validation.
- Reproducible toolchain and cross-platform builds; GPU only after determinism validation.
- License selection before public distribution.

No network access is needed by the application itself for local inputs. Installing dependencies and using GitHub may require internet access.
