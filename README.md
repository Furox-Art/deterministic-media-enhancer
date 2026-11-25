# Deterministic Media Enhancer (DME)

Local, offline image and video enhancement without trained models, cloud services, telemetry or usage fees.

## v0.1.0 — initial prototype

C++17 CLI uses FFmpeg CPU filters: Lanczos scaling (1x/2x/4x/8x), spatial denoising and controlled unsharp masking. Presets: clean, detail, resize. PNG image output, FFV1/FLAC MKV or H.264/AAC MP4 video output. No overwriting existing output. Qt 6 desktop GUI source included; optional build.

## Build on Linux

Dependencies: CMake >=3.20, C++17 compiler, FFmpeg and FFprobe (with libx264). Qt 6 Widgets development package is optional.

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j
ctest --test-dir build --output-on-failure
./build/dme input.jpg output.png 2 clean
./build/dme input.mp4 output.mkv 2 detail
./build/dme-gui # if Qt6 Widgets was found
```

## Determinism

CPU-only, restricted FFmpeg CPU flags, single-thread processing, bitexact flags, fixed presets, source metadata removed. Reproducibility tests compare whole-file bytes across repeated runs in the same environment. Cross-version/hardware bit identity is not guaranteed. Upscaling is not recovery of lost detail. PNG/FFV1 outputs are losslessly encoded after pixel conversion and filters, not necessarily source-preserving. MP4 is lossy. Alpha processing, HDR/color management, orientation, VFR, and uncommon media require further validation. Audio is re-encoded; subtitles and metadata are not preserved. This release is not a complete production application.

## Planned features

- Adaptive defect detection and deterministic parameter search.
- Deblocking, debanding, deblurring, BM3D and multi-frame restoration.
- Preview, batch jobs, progress, cancellation and advanced controls.
- Frame interpolation, stabilization, color management and higher bit-depth validation.
- Reproducible toolchain and cross-platform builds; GPU only after determinism validation.
- License selection before public distribution.

No network access is used by the application itself for local inputs. Installing dependencies and using GitHub may require internet access.
