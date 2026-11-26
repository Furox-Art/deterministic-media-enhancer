# RC3 validation record

This is a limited local release-candidate record, not a claim of universal quality or cross-machine reproducibility.

## Algorithms and local experiments

- Native integer BM3D-family variant: 1,000 generated 32x32 luma cases, aggregate SSE 63,086,713 to 14,248,507; 994 improved, six unchanged, none worse.
- Local super-resolution: 1,000 generated four-frame, 32x32 source sequences with mixed half-pixel shifts and a low-contrast occlusion proxy; only the center frame is evaluated against the analytic high-resolution reference. Aggregate SSE 1,947,811 to 668,187; 958 improved, 42 worsened, none unchanged. 2,271,517 fused sample positions. Combined synthetic benchmark wall time 28.36 s, peak RSS 2,220 KiB in this environment (observational).
- Conservative blind PSF on 128x128 checkerboard with known 5-tap horizontal blur: encoded grayscale MSE 560 to 384.94, selected kernel 2. Flat/ambiguous inputs abstain; separate step-edge regressions test axis motion, binomial blur and unsupported combinations.
- Diagonal 5-tap blur on the same checkerboard: motion-deblur abstained (processed_frames=0). Encoded MSE 1120 to 1116.37 is conversion difference, not restoration improvement. Known-PSF inverse unit test improves SSE; that does not prove unknown complex-motion recovery.
- Public-domain SciPy ascent image (local distribution copyright identifies ascent.dat as public domain): controlled grayscale sigma=8 degradation, Lanczos resize MSE 63.589752 / PSNR 30.0969 dB / SSIM 0.809150; native BM3D 33.821960 / 32.8388 / 0.944972; patch 28.776794 / 33.5404 / 0.944283. BM3D not superior in every metric. Visual inspection found noise reduction and softened texture. No media fixtures or private files are included in the repository.
- In the mixed local-motion analytical test, local registration with patch guard and three complementary neighbors outperforms global fusion. Unit tests check center sample preservation, all three phase contributions, low-contrast occlusion proxy, cut rejection and repeatability.

## Precision and CUDA

- RGBA16 1x decoded samples including alpha exact, 2x native-depth resize accepted. PQ and HLG tagged 10- and 16-bit synthetic videos tested at 1x and 2x; 1x exact decoded sample hashes and output pixel format/color tags verified. HDR10 mastering/content-light side data deliberately rejected; this is not HDR10 metadata-preserving restoration or tone mapping.
- NVIDIA GeForce RTX 4060 Laptop GPU, CUDA integer cubic kernel; one real encoded six-frame test with all three complementary phases contributed [41724, 41768, 27633] samples, CPU and two GPU outputs identical SHA-256: 417276c1e79a1652c9eb928f13ec12bdb2ab166d0c57e8b5e66ddf18dd606360. GPU first-frame CPU parity enforced per job; registration/fusion/encoding still CPU. No end-to-end performance advantage established.
- The earlier RC2 portable test compared two affinities on one physical CPU (19 cases × two repeats × two affinities). Different physical computers are not available in the current environment. Do not claim independent-machine validation.

## Remaining constraints

Full arbitrary PSF estimation, spatially varying motion/rolling shutter restoration, broad temporal/color quality validation, HDR metadata propagation, high-bit-depth denoising and further GPU kernels remain open. No independent thousand-image/thousand-video real corpus has been tested; the synthetic sweep is not a substitute. The reproducibility script records fixture/source/toolchain and per-case hashes; independently verify physical hosts and source provenance.

## Re-run

```sh
bash scripts/build.sh
bash scripts/build-gpu.sh # optional, only on supported CUDA hardware
build/skill/dme --synthetic-benchmark 1000
python3 tests/quality.py build/skill/dme --reference /authorized/reference.png
```

See the portable fixture instructions in the README. This release candidate was built with an existing local toolchain; no VM or cloud compute was provisioned.

## Final RC3 checks

13 Rust unit tests and 12 integration suites passed; optional CUDA integration also passed. The installed CPU and CUDA executables matched their respective built binaries by SHA-256. RC3 portable comparison on logical CPU affinities 0 and 7: 22 cases × two repeats × two affinities = 88 renders; no case differences, matching fixtures, source, golden kernel and recorded toolchain. Both affinities are on one physical host. Paired corpus smoke test: three cases completed, two improved, one unchanged, none failed; one independent public-domain photograph and two synthetic controls. These are not thousands of real-media samples.
