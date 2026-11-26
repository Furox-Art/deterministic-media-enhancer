# RC4 validation record

Local WSL, RTX 4060, FFmpeg 6.1.1. No trained model, inference API, or new VM. Installed and release CPU binaries matched SHA-256 e6577248177f4861324f33df2ea85c03312ed6428c2c7e23f345acfae661c447. CUDA helper hash e2754ab7de02cba8a676ed807e1471fabae89b4c5074b2c50ee767233415eaae.

## Restoration and real references

The directional estimator identified seven controlled PSFs (IDs 5, 7, 8, 9, 10, 11, 12), including oblique and diagonal motion, with reduced SSE. This is a bounded uniform-motion model, not arbitrary blind or spatially varying deconvolution.

| Real reference collection | Count | Noise SSE before | Noise SSE after | Improved | Worse | Clean mean MSE |
|---|---:|---:|---:|---:|---:|---:|
| CLIC professional + mobile validation | 102 | 114629782 | 81248383 | 90 | 12 | 3.758558 |
| EuroSAT RGB, 200 per class | 2000 | 572062776 | 268378615 | 1815 | 185 | 3.169525 |

Controlled integer noise ±14; grayscale square references resized to 128×128 (CLIC) and 64×64 (EuroSAT); no training. Source scene independence is not established. No decode failures; all per-case metrics repeated on final rc4. Visual inspection included best and worst examples; texture loss in the worst example is real. Clean controls show that denoising is not universally safe.

On the 102 CLIC references with controlled five-tap motion, the estimator selected one correct model (ID 6), improved one reference and abstained on 101; none worsened. Total SSE 396740335 → 395778069. On clean references it selected zero. This is conservative but has very low recall on natural images.

Sources (downloaded locally for evaluation, not committed):
- CLIC: https://archive.compression.cc/2021/tasks/index.html ; professional-valid.zip SHA-256 00be38092ade2ab5163b65f7f6523d46c7bca147007f559adbb02c0afc236b7d ; mobile-valid.zip ff0280382eeb32fdceb28c81a7f47ac9ad86d933ebe901680845635d04e47a10.
- EuroSAT: https://github.com/phelber/EuroSAT ; https://zenodo.org/records/7711810 ; EuroSAT_RGB.zip SHA-256 b4f5b234ecb7d7ff9c6cddb046543b4717c53fd6e9815be6c0e80cc614f51b90.
- Reference runner: scripts/reference_benchmark.py; per-case JSON stored locally in build/qa-rc4/clic-rc4.json and eurosat-rc4.json. Pillow 12.3.0 (QA only).

## Real video, negative result

Two CC0 source videos from https://github.com/shotstack/test-media/tree/b790a2b72325feecd189291c52663ed8123b1839 : orientation/h1080_w1920_f30_a16-9_r0.mp4 (SHA-256 5f4455bec917bce657ccc6a8ead87e806a6264f9272042f1dbcbe6d7bc75195c) and captioning/scott-ko.mp4 (265d97f2e38d25fbd1876d944461da631389851962b6353367e54f880b5bcb65). Three eight-frame clips each at seconds 1, 4, 7; 24 fps, area-downsampled 320×180 reference then 160×90 input. Baseline Lanczos vs local multiframe MSE:

| Clip | Lanczos | Local |
|---|---:|---:|
| 1 | 143.44250 | 228.84895 |
| 2 | 149.63327 | 234.25528 |
| 3 | 141.45370 | 222.41422 |
| 4 | 102.38653 | 204.04613 |
| 5 | 101.97569 | 204.98905 |
| 6 | 106.79141 | 213.90132 |

All six completed, all six worse than Lanczos. The local point-sampling reconstruction model is mismatched to this area-downsampled input. The method remains explicit, not auto. Visual QA compared reference, Lanczos and local. Two source videos are not a broad video corpus; the CLIC video archive request returned HTTP 403.

## CUDA

Registration (luma SAD search, forward/reverse consistency), cubic interpolation, patch rejection and blending execute on CUDA for the local multiframe preset. Decoding, encoding and other presets remain CPU. CPU vs two GPU renders of six-frame 96×80 fixture matched SHA-256 417276c1e79a1652c9eb928f13ec12bdb2ab166d0c57e8b5e66ddf18dd606360; phase counts [0,41724,41768,27633]. Odd 97×83 seven-frame chromatic/occlusion/cut fixture matched d6f241d0db1a53ae0dde243b6b111196259b9133f97b384f18173db52de66f8e with [0,42537,35885,28085].

Real orientation clip: three CPU + three CUDA renders identical SHA-256 6ba64b163cc98deb9a24b20297f73ca7ecd2582f190d92e8a64b44730dc3f1a0. End-to-end CPU seconds 3.145, 2.144, 1.946; CUDA 1.605, 1.593, 1.728. Median 2.144 vs 1.605 s (about 1.34×), for this workload and machine only. No general GPU performance or quality claim.

## HDR and reproduction

Static HDR10 lossless HEVC MKV, BT.2020/PQ 10-bit, mastering display and content light metadata checked frame-by-frame. Tested 4:2:0 and 4:4:4 at 1× and 2×, exact decoded samples vs source or independently resized reference, two-run repeat. Unsupported dynamic metadata rejected; unregistered vendor SEI not preserved. Not HDR10+, Dolby Vision, full HDR processing or display validation. The preview is SDR.

15 Rust unit tests, 12 integration suites and optional CUDA integration passed. Two CPU affinities (0 and 7), 24 cases each repeated twice (96 renders), all manifests match. Same physical host and toolchain, not independent hardware proof. Only one authorized online computer was available.

Re-run:
```sh
bash scripts/build.sh
bash scripts/build-gpu.sh
python3 scripts/repro.py --help
python3 scripts/reference_benchmark.py --help
```

The four parallel specialist requests remained pending without delivered results; the implementation and measurements above are main-agent work, not independently agent-audited.
