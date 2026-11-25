#!/usr/bin/env python3
"""Local JSON interface to the deterministic media engine. Standard library only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_ENGINE = ROOT / "build" / "skill" / "dme"
PRESETS = ("auto", "clean", "detail", "resize", "restore", "denoise",
           "regional", "deblur", "wavelet", "temporal", "stabilize", "multiframe")
IMAGES = {".png", ".jpg", ".jpeg", ".bmp", ".tif", ".tiff", ".webp"}
VIDEOS = {".mp4", ".mkv", ".mov", ".avi", ".webm"}
VIDEO_ONLY = {"temporal", "stabilize", "multiframe"}
ENV = dict(os.environ, LC_ALL="C", LANG="C", TZ="UTC")


class JobError(Exception):
    def __init__(self, message, reports=None, diagnostics=None):
        super().__init__(message)
        self.reports = reports or []
        self.diagnostics = diagnostics or []


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise JobError(message)


def run(argv):
    return subprocess.run([str(a) for a in argv], capture_output=True,
                          text=True, encoding="utf-8", errors="replace", env=ENV,
                          stdin=subprocess.DEVNULL, check=False)


def first_line(argv):
    result = run(argv)
    if result.returncode != 0:
        raise JobError("Version check failed: " + str(argv[0]))
    return result.stdout.splitlines()[0] if result.stdout else ""


def fingerprint(path):
    sha = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            sha.update(chunk)
    return sha.hexdigest()


def split_reports(raw):
    reports, diagnostics = [], []
    for line in raw.splitlines():
        try:
            value = json.loads(line)
            if isinstance(value, dict):
                reports.append(value)
                continue
        except ValueError:
            pass
        if line.strip():
            diagnostics.append(line)
    return reports, diagnostics


def doctor(engine):
    names = ("ffmpeg", "ffprobe", "cmake", "c++")
    tools = {name: shutil.which(name) for name in names}
    runtime_missing = [n for n in ("ffmpeg", "ffprobe") if not tools[n]]
    build_missing = [n for n in ("cmake", "c++") if not tools[n]]
    filters, encoders = [], []
    versions = {}
    if not runtime_missing:
        versions = {n: first_line([tools[n], "-version"]) for n in ("ffmpeg", "ffprobe")}
        required = ("scale", "hqdn3d", "nlmeans", "deblock", "deband", "unsharp",
                    "bilateral", "smartblur", "convolution", "vaguedenoiser",
                    "atadenoise", "deshake", "minterpolate", "tpad", "trim")
        f, e = run([tools["ffmpeg"], "-hide_banner", "-filters"]), run([tools["ffmpeg"], "-hide_banner", "-encoders"])
        if f.returncode or e.returncode:
            raise JobError("FFmpeg capability check failed")
        available_f = {x.split()[1] for x in f.stdout.splitlines() if len(x.split()) >= 2}
        available_e = {x.split()[1] for x in e.stdout.splitlines() if len(x.split()) >= 2}
        filters = [x for x in required if x not in available_f]
        encoders = [x for x in ("png", "pgm", "ffv1", "flac", "libx264", "aac", "rawvideo") if x not in available_e]
    ready = engine.is_file() and os.access(engine, os.X_OK)
    if ready:
        versions["engine"] = first_line([engine, "--version"])
    ok = ready and not (runtime_missing or filters or encoders)
    return {"status": "ok" if ok else "error", "operation": "doctor",
            "engine_ready": ready, "engine": str(engine),
            "tools": tools, "versions": versions, "missing_runtime": runtime_missing,
            "missing_build": build_missing, "missing_filters": filters, "missing_encoders": encoders}


def media_input(raw):
    p = Path(raw).expanduser().resolve(strict=True)
    if not p.is_file() or p.suffix.lower() not in IMAGES | VIDEOS:
        raise JobError("Input must be a supported local regular file")
    return p


def check_options(source, scale, preset, fps):
    if source.suffix.lower() in IMAGES and (preset in VIDEO_ONLY or fps != "keep"):
        raise JobError("Video-only operation requested for an image")
    if preset == "multiframe" and (scale != 2 or fps != "keep"):
        raise JobError("Multiframe requires 2x and keep FPS")


def check_output(source, raw):
    p = Path(raw).expanduser().absolute()
    if p.exists() or p.is_symlink():
        raise JobError("Output already exists; refusing overwrite")
    if not p.parent.is_dir():
        raise JobError("Output parent must already exist")
    allowed = {".png"} if source.suffix.lower() in IMAGES else {".mkv", ".mp4"}
    if p.suffix.lower() not in allowed:
        raise JobError("Unsupported output extension for media type")
    return p


def process_job(engine, source, target, scale, preset, fps):
    check_options(source, scale, preset, fps)
    target = check_output(source, target)
    initial_hash = fingerprint(source)
    result = run([engine, source, target, scale, preset, fps])
    reports, diagnostics = split_reports(result.stderr)
    if result.returncode != 0:
        raise JobError("Engine failed with exit code " + str(result.returncode), reports, diagnostics)
    if not target.is_file() or target.stat().st_size == 0:
        raise JobError("Engine did not publish a nonempty output", reports, diagnostics)
    if fingerprint(source) != initial_hash:
        raise JobError("Source changed during processing; output is not verified", reports, diagnostics)
    if source.suffix.lower() in VIDEOS and not any(
            x.get("video_guard") == "timeline" and x.get("accepted") is True for x in reports):
        raise JobError("Required video timeline report missing", reports, diagnostics)
    return {"status": "ok", "operation": "enhance", "input": str(source), "output": str(target),
            "input_sha256": initial_hash, "sha256": fingerprint(target), "bytes": target.stat().st_size,
            "options": {"scale": scale, "preset": preset, "fps": fps},
            "engine_version": first_line([engine, "--version"]),
            "ffmpeg_version": first_line(["ffmpeg", "-version"]),
            "reports": reports, "diagnostics": diagnostics,
            "fallback": any("fallback" in r for r in reports)}


def options(parser):
    parser.add_argument("--input", required=True)
    parser.add_argument("--scale", type=int, choices=(1, 2, 4, 8), default=1)
    parser.add_argument("--preset", choices=PRESETS, default="auto")
    parser.add_argument("--fps", choices=("keep", "30", "60", "120"), default="keep")


def main():
    parser = Parser(description=__doc__)
    parser.add_argument("--engine", type=Path, default=DEFAULT_ENGINE)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("doctor")
    analyze = sub.add_parser("analyze")
    analyze.add_argument("--input", required=True)
    analyze.add_argument("--scale", type=int, choices=(1, 2, 4, 8), default=1)
    enhance = sub.add_parser("enhance")
    options(enhance)
    enhance.add_argument("--output", required=True)
    repeat = sub.add_parser("verify-repeat")
    options(repeat)
    repeat.add_argument("--format", choices=("png", "mkv", "mp4"))
    args = parser.parse_args()
    engine = args.engine.expanduser().resolve()
    if args.command == "doctor":
        return doctor(engine)
    if not engine.is_file() or not os.access(engine, os.X_OK):
        raise JobError("Local engine missing; run scripts/build.sh")
    if not shutil.which("ffmpeg") or not shutil.which("ffprobe"):
        raise JobError("FFmpeg/FFprobe missing")
    source = media_input(args.input)
    if args.command == "analyze":
        result = run([engine, "--analyze", source, args.scale])
        reports, diagnostics = split_reports(result.stderr)
        if result.returncode:
            raise JobError("Analysis failed", reports, diagnostics)
        analysis = json.loads(result.stdout)
        if not isinstance(analysis, dict) or "selected" not in analysis:
            raise JobError("Invalid engine analysis response")
        return {"status": "ok", "operation": "analyze", "input": str(source),
                "analysis": analysis, "diagnostics": diagnostics}
    if args.command == "enhance":
        return process_job(engine, source, args.output, args.scale, args.preset, args.fps)
    ext = args.format or ("png" if source.suffix.lower() in IMAGES else "mkv")
    with tempfile.TemporaryDirectory(prefix="dme-repeat-") as temp:
        a = process_job(engine, source, Path(temp) / ("a." + ext), args.scale, args.preset, args.fps)
        b = process_job(engine, source, Path(temp) / ("b." + ext), args.scale, args.preset, args.fps)
        match = a["input_sha256"] == b["input_sha256"] and a["sha256"] == b["sha256"]
        return {"status": "ok" if match else "error", "operation": "verify-repeat",
                "match": match, "sha256_first": a["sha256"], "sha256_second": b["sha256"],
                "input_sha256": a["input_sha256"], "engine_version": a["engine_version"],
                "ffmpeg_version": a["ffmpeg_version"], "options": a["options"],
                "scope": "same local execution environment; temporary outputs removed"}


if __name__ == "__main__":
    try:
        response = main()
    except JobError as exc:
        response = {"status": "error", "error": str(exc), "reports": exc.reports, "diagnostics": exc.diagnostics}
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        response = {"status": "error", "error": str(exc)}
    print(json.dumps(response, ensure_ascii=True, sort_keys=True, separators=(",", ":")))
    sys.exit(0 if response.get("status") == "ok" else 1)
