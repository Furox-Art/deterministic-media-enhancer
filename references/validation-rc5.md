# RC5 validation

Controlled grayscale reference sweep on the same development sets used in RC4 (not an independent holdout): BM3D soft improved 101/102 CLIC patches and 1998/2000 EuroSAT patches. Full strength improved 1905/2102 combined; half strength improved 2099/2102. The half-strength blend reduces texture loss but can retain more noise. This is not a guarantee of improvement on arbitrary inputs.

BT.709 limited-range conversion is explicit for supported SDR inputs. On six real-video excerpts, local fusion MSE decreased relative to RC4 but remained worse than Lanczos on all six. Local fusion remains opt-in. HDR inputs are rejected by this path.

The previous packaged advanced regression failed because the fixture's default audio codec shifted the first video PTS to 3 ms, outside the supported 2 ms boundary. The fixture now explicitly encodes FLAC; the engine's guard is unchanged.

Distribution: source-only npm and Python artifacts, no runtime package dependencies, no install hooks, explicit offline build. See scripts/package.py and scripts/test_packages.py. Linux/WSL only. Independent physical-machine validation and broad real-video restoration quality remain open.

The original RC5 visual comparison was performed on a difficult CLIC example; the softer blend preserved more detail but retained some noise.
