#!/usr/bin/env python3
"""Local JSON interface to the deterministic media engine. Standard library only."""
import argparse
import hashlib
import json
import math
import os
import selectors
import signal
import time
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
TIMEOUT_SECONDS = 3600
MAX_CAPTURE = 8 * 1024 * 1024


class JobError(Exception):
    def __init__(self, message, reports=None, diagnostics=None):
        super().__init__(message)
        self.reports = reports or []
        self.diagnostics = diagnostics or []


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise JobError(message)


def run(argv):
    args = [str(a) for a in argv]
    with subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          env=ENV, stdin=subprocess.DEVNULL, start_new_session=True) as proc:
        deadline = time.monotonic() + TIMEOUT_SECONDS
        buffers = {"stdout": bytearray(), "stderr": bytearray()}
        total = 0
        try:
            with selectors.DefaultSelector() as selector:
                selector.register(proc.stdout, selectors.EVENT_READ, "stdout")
                selector.register(proc.stderr, selectors.EVENT_READ, "stderr")
                while selector.get_map():
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise JobError("Local subprocess timeout")
                    for key, _ in selector.select(remaining):
                        chunk = os.read(key.fileobj.fileno(), 65536)
                        if not chunk:
                            selector.unregister(key.fileobj)
                            continue
                        total += len(chunk)
                        if total > MAX_CAPTURE:
                            raise JobError("Local subprocess output limit exceeded")
                        buffers[key.data].extend(chunk)
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise JobError("Local subprocess timeout")
            try:
                code = proc.wait(timeout=remaining)
            except subprocess.TimeoutExpired as exc:
                raise JobError("Local subprocess timeout") from exc
        except BaseException:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            proc.wait()
            raise
    return subprocess.CompletedProcess(args, code,
        buffers["stdout"].decode("utf-8", errors="strict"),
        buffers["stderr"].decode("utf-8", errors="replace"))


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


def reject_constant(value):
    raise JobError("Non-finite JSON value: " + value)


def finite_float(value):
    number = float(value)
    if not math.isfinite(number):
        raise JobError("Non-finite JSON number")
    return number


def strict_json(raw):
    return json.loads(raw, object_pairs_hook=reject_duplicate_keys,
                      parse_constant=reject_constant, parse_float=finite_float)


def split_reports(raw):
    reports, diagnostics = [], []
    for line in raw.splitlines():
        if line.lstrip().startswith(("{", "[")):
            try:
                value = strict_json(line)
            except ValueError as exc:
                raise JobError("Malformed structured engine report") from exc
            if not isinstance(value, dict):
                raise JobError("Structured engine report must be an object")
            reports.append(value)
        elif line.strip():
            diagnostics.append(line)
    return reports, diagnostics


def positive_int(value):
    return type(value) is int and value > 0


def validate_analysis(report):
    if not isinstance(report, dict) or not isinstance(report.get("selected"), str):
        raise JobError("Invalid engine analysis response")
    candidates = report.get("candidates")
    if not isinstance(candidates, list) or not candidates or not isinstance(report.get("filter"), str):
        raise JobError("Invalid analysis candidates")
    names = set()
    for item in candidates:
        if (not isinstance(item, dict) or not isinstance(item.get("name"), str)
                or not isinstance(item.get("chain"), str) or type(item.get("accepted")) is not bool
                or type(item.get("score")) is not int or item["name"] in names):
            raise JobError("Invalid or duplicate analysis candidate")
        names.add(item["name"])
    if not any(c["name"] == report["selected"] and c["accepted"] for c in candidates):
        raise JobError("Selected analysis candidate not accepted")


def validate_reports(reports, image, scale, fps, preset):
    kind, expected = ("image_guard", "geometry") if image else ("video_guard", "timeline")
    guards = [r for r in reports if kind in r]
    if len(guards) != 1 or guards[0].get(kind) != expected or guards[0].get("accepted") is not True:
        raise JobError("Missing, duplicate or rejected " + expected + " report", reports)
    guard = guards[0]
    if image:
        keys = ("input_width", "input_height", "output_width", "output_height")
        if (not all(positive_int(guard.get(k)) for k in keys)
                or guard["output_width"] != scale * guard["input_width"]
                or guard["output_height"] != scale * guard["input_height"]):
            raise JobError("Invalid image geometry report", reports)
    else:
        keys = ("input_frames", "output_frames", "input_duration_us", "output_duration_us")
        if (not all(positive_int(guard.get(k)) for k in keys)
                or type(guard.get("target_fps")) is not int
                or guard["target_fps"] != (0 if fps == "keep" else int(fps))
                or (fps == "keep" and guard["input_frames"] != guard["output_frames"])):
            raise JobError("Invalid video timeline report", reports)
    analyses = [r for r in reports if "selected" in r or "candidates" in r]
    if preset == "auto":
        if len(analyses) != 1:
            raise JobError("Missing or duplicate automatic analysis report", reports)
        validate_analysis(analyses[0])
    elif analyses:
        raise JobError("Unexpected automatic analysis report", reports)
    luma = [r for r in reports if "output_guard" in r]
    fallback = [r for r in reports if "fallback" in r]
    if len(luma) > 1 or len(fallback) > 1:
        raise JobError("Duplicate output guard or fallback report", reports)
    if image and preset == "auto":
        needs_luma = analyses[0]["selected"] != "baseline"
        if bool(luma) != needs_luma:
            raise JobError("Missing or unexpected automatic luma guard", reports)
    if luma:
        g = luma[0]
        if (not image or preset != "auto" or g.get("output_guard") != "full_image_luma"
                or type(g.get("accepted")) is not bool
                or not positive_int(g.get("tiles")) or type(g.get("rejected_tiles")) is not int
                or not 0 <= g["rejected_tiles"] <= g["tiles"]
                or g["accepted"] != (g["rejected_tiles"] == 0)
                or bool(fallback) == g["accepted"]):
            raise JobError("Contradictory luma or fallback report", reports)
    if fallback and (not luma or fallback[0].get("fallback") != "baseline"
                     or fallback[0].get("reason") != "full_image_guard"):
        raise JobError("Invalid fallback report", reports)


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
    try:
        p = Path(raw).expanduser().resolve(strict=True)
    except RuntimeError as exc:
        raise JobError("Input path resolution failed") from exc
    if not p.is_file() or p.suffix.lower() not in IMAGES | VIDEOS:
        raise JobError("Input must be a supported local regular file")
    return p


def check_options(source, scale, preset, fps):
    if source.suffix.lower() in IMAGES and (preset in VIDEO_ONLY or fps != "keep"):
        raise JobError("Video-only operation requested for an image")
    if preset == "multiframe" and (scale != 2 or fps != "keep"):
        raise JobError("Multiframe requires 2x and keep FPS")


def check_output(source, raw):
    requested = Path(raw).expanduser().absolute()
    try:
        p = requested.parent.resolve(strict=True) / requested.name
    except RuntimeError as exc:
        raise JobError("Output parent resolution failed") from exc
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
    engine_hash = fingerprint(engine)
    engine_version = first_line([engine, "--version"])
    ffmpeg_version = first_line(["ffmpeg", "-version"])
    tool_hashes = {name: fingerprint(shutil.which(name)) for name in ("ffmpeg", "ffprobe")}
    with tempfile.TemporaryDirectory(prefix=".dme-agent-", dir=target.parent) as temp:
        staged = Path(temp) / ("media" + target.suffix.lower())
        result = run([engine, source, staged, scale, preset, fps])
        reports, diagnostics = split_reports(result.stderr)
        if result.returncode != 0:
            raise JobError("Engine failed with exit code " + str(result.returncode), reports, diagnostics)
        if staged.is_symlink() or not staged.is_file() or staged.stat().st_size == 0:
            raise JobError("Engine did not publish a nonempty regular output", reports, diagnostics)
        digest, size = fingerprint(staged), staged.stat().st_size
        if (fingerprint(source) != initial_hash or fingerprint(engine) != engine_hash
                or any(fingerprint(shutil.which(name)) != expected for name, expected in tool_hashes.items())):
            raise JobError("Source or toolchain changed during processing; output not published", reports, diagnostics)
        validate_reports(reports, source.suffix.lower() in IMAGES, scale, fps, preset)
        os.link(staged, target, follow_symlinks=False)
    return {"status": "ok", "operation": "enhance", "input": str(source), "output": str(target),
            "input_sha256": initial_hash, "sha256": digest, "bytes": size,
            "options": {"scale": scale, "preset": preset, "fps": fps},
            "engine_version": engine_version, "engine_sha256": engine_hash,
            "ffmpeg_version": ffmpeg_version, "tool_sha256": tool_hashes,
            "reports": reports, "diagnostics": diagnostics,
            "fallback": any("fallback" in r for r in reports)}


def reject_duplicate_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise JobError("Duplicate JSON key: " + key)
        result[key] = value
    return result


def batch_plan(raw):
    manifest = Path(raw).expanduser().resolve(strict=True)
    if not manifest.is_file() or manifest.stat().st_size > 1024 * 1024:
        raise JobError("Manifest must be a local regular JSON file, at most 1 MiB")
    with manifest.open(encoding="utf-8") as handle:
        value = strict_json(handle.read(1024 * 1024 + 1))
    if not isinstance(value, dict) or set(value) != {"jobs"}:
        raise JobError("Manifest must contain only a jobs array")
    jobs = value["jobs"]
    if not isinstance(jobs, list) or not 1 <= len(jobs) <= 100:
        raise JobError("Manifest must contain 1 to 100 jobs")
    planned = []
    for i, item in enumerate(jobs, 1):
        try:
            if not isinstance(item, dict) or not {"input", "output"} <= item.keys() or item.keys() - {"input","output","scale","preset","fps"}:
                raise JobError("Invalid job fields")
            for key in ("input", "output"):
                if not isinstance(item[key], str) or not item[key] or chr(0) in item[key]:
                    raise JobError("Input and output must be nonempty path strings")
            scale, preset, fps = item.get("scale", 1), item.get("preset", "auto"), item.get("fps", "keep")
            if type(scale) is not int or scale not in (1,2,4,8) or preset not in PRESETS or fps not in ("keep","30","60","120"):
                raise JobError("Invalid scale, preset or FPS")
            source = media_input(manifest.parent / Path(item["input"]).expanduser())
            target = check_output(source, manifest.parent / Path(item["output"]).expanduser())
            check_options(source, scale, preset, fps)
            planned.append((source,target,scale,preset,fps))
        except (JobError, OSError, ValueError) as exc:
            raise JobError(f"Job {i} preflight: {exc}") from exc
    targets = [job[1] for job in planned]
    if len(set(targets)) != len(targets):
        raise JobError("Duplicate canonical output paths in manifest")
    return planned


def batch(engine, raw, dry_run=False):
    planned = batch_plan(raw)
    jobs = []
    for i, (source,target,scale,preset,fps) in enumerate(planned,1):
        record = {"index": i, "input": str(source), "output": str(target),
                  "options": {"scale": scale, "preset": preset, "fps": fps}}
        if dry_run:
            record["status"] = "planned"
        else:
            try:
                record.update(process_job(engine, source, target, scale, preset, fps))
            except (JobError, OSError, ValueError, subprocess.SubprocessError) as exc:
                record.update(status="error", error=str(exc))
                if isinstance(exc, JobError):
                    record.update(reports=exc.reports, diagnostics=exc.diagnostics)
        jobs.append(record)
    failed = sum(j["status"] == "error" for j in jobs)
    return {"status": "error" if failed else "ok", "operation": "batch",
            "dry_run": dry_run, "total": len(jobs), "failed": failed,
            "succeeded": sum(j["status"] == "ok" for j in jobs), "jobs": jobs}


def options(parser):
    parser.add_argument("--input", required=True)
    parser.add_argument("--scale", type=int, choices=(1, 2, 4, 8), default=1)
    parser.add_argument("--preset", choices=PRESETS, default="auto")
    parser.add_argument("--fps", choices=("keep", "30", "60", "120"), default="keep")


def main():
    parser = Parser(description=__doc__)
    parser.add_argument("--engine", type=Path, default=DEFAULT_ENGINE)
    parser.add_argument("--timeout-seconds", type=int, default=3600,
                        help="Per subprocess deadline (1 to 86400 seconds)")
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
    group = sub.add_parser("batch")
    group.add_argument("--manifest", required=True)
    group.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    global TIMEOUT_SECONDS
    if not 1 <= args.timeout_seconds <= 86400:
        raise JobError("Timeout must be between 1 and 86400 seconds")
    TIMEOUT_SECONDS = args.timeout_seconds
    engine = args.engine.expanduser().resolve()
    if args.command == "doctor":
        return doctor(engine)
    if not engine.is_file() or not os.access(engine, os.X_OK):
        raise JobError("Local engine missing; run scripts/build.sh")
    if not shutil.which("ffmpeg") or not shutil.which("ffprobe"):
        raise JobError("FFmpeg/FFprobe missing")
    if args.command == "batch":
        return batch(engine, args.manifest, args.dry_run)
    source = media_input(args.input)
    if args.command == "analyze":
        result = run([engine, "--analyze", source, args.scale])
        reports, diagnostics = split_reports(result.stderr)
        if result.returncode:
            raise JobError("Analysis failed", reports, diagnostics)
        analysis = strict_json(result.stdout)
        validate_analysis(analysis)
        return {"status": "ok", "operation": "analyze", "input": str(source),
                "analysis": analysis, "diagnostics": diagnostics}
    if args.command == "enhance":
        return process_job(engine, source, args.output, args.scale, args.preset, args.fps)
    ext = args.format or ("png" if source.suffix.lower() in IMAGES else "mkv")
    with tempfile.TemporaryDirectory(prefix="dme-repeat-") as temp:
        a = process_job(engine, source, Path(temp) / ("a." + ext), args.scale, args.preset, args.fps)
        b = process_job(engine, source, Path(temp) / ("b." + ext), args.scale, args.preset, args.fps)
        match = all(a[key] == b[key] for key in ("input_sha256", "sha256", "engine_sha256", "engine_version", "ffmpeg_version", "tool_sha256"))
        return {"status": "ok" if match else "error", "operation": "verify-repeat",
                "match": match, "sha256_first": a["sha256"], "sha256_second": b["sha256"],
                "input_sha256": a["input_sha256"], "engine_version": a["engine_version"],
                "ffmpeg_version": a["ffmpeg_version"], "engine_sha256": a["engine_sha256"], "tool_sha256": a["tool_sha256"], "options": a["options"],
                "scope": "same local execution environment; temporary outputs removed"}


if __name__ == "__main__":
    try:
        response = main()
    except KeyboardInterrupt:
        response = {"status": "error", "error": "Interrupted by operator"}
    except JobError as exc:
        response = {"status": "error", "error": str(exc), "reports": exc.reports, "diagnostics": exc.diagnostics}
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as exc:
        response = {"status": "error", "error": str(exc)}
    print(json.dumps(response, ensure_ascii=True, allow_nan=False, sort_keys=True, separators=(",", ":")))
    sys.exit(0 if response.get("status") == "ok" else 1)
