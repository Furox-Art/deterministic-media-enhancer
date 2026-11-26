"""Explicit offline build and invocation of the packaged Rust media skill."""
import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import stat
import subprocess
import sys
import tempfile

SKILL = Path(__file__).resolve().parent / "skill"
VERSION = "1.0.0-rc5"

def fingerprint(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1024*1024), b""):
            h.update(block)
    return h.hexdigest()

def identity():
    h = hashlib.sha256()
    h.update(bytes.fromhex(fingerprint(__file__)))
    for f in sorted(SKILL.rglob("*")):
        if f.is_file() and "__pycache__" not in f.parts:
            h.update(f.relative_to(SKILL).as_posix().encode() + b"\0")
            h.update(bytes.fromhex(fingerprint(f)))
    return h.hexdigest()

def private_dir(path):
    path.mkdir(mode=0o700, exist_ok=True)
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError("Cache must be an owned private directory: " + str(path))
    return path

def cache():
    root = Path(os.environ.get("XDG_CACHE_HOME") or (Path.home() / ".cache"))
    if not root.is_absolute():
        raise ValueError("XDG_CACHE_HOME must be absolute")
    root.mkdir(parents=True, exist_ok=True)
    base = private_dir(root / "deterministic-media-enhancer")
    return private_dir(private_dir(base / platform.machine()) / identity())

@contextmanager
def locked(dest, exclusive):
    fd = os.open(dest / ".lock", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "r+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX if exclusive else fcntl.LOCK_SH)
        yield

def tools(names):
    result = {}
    for name in names:
        path = shutil.which(name)
        if not path:
            raise ValueError("Missing local dependency: " + name)
        result[name] = fingerprint(path)
    return result

def build(dest, gpu=False):
    runtime = tools(("ffmpeg", "ffprobe"))
    toolchain = tools(("cargo", "rustc", "bash") + (("nvcc",) if gpu else ()))
    with tempfile.TemporaryDirectory(prefix="build-", dir=dest) as tmp:
        root = Path(tmp) / "skill"
        shutil.copytree(SKILL, root, ignore=shutil.ignore_patterns("__pycache__"))
        env = dict(os.environ, CARGO_NET_OFFLINE="true", LC_ALL="C", TZ="UTC")
        subprocess.run(["bash", str(root / "scripts/build.sh")], cwd=root, env=env, check=True)
        if gpu:
            subprocess.run(["bash", str(root / "scripts/build-gpu.sh")], cwd=root, env=env, check=True)
        if runtime != tools(runtime) or toolchain != tools(toolchain):
            raise ValueError("Build tools changed during build")
        names = ["dme"] + (["dme-cuda"] if gpu else [])
        stage = root / "build/skill"
        manifest = {"source": identity(), "runtime": runtime, "build_tools": toolchain,
                    "binaries": {n: fingerprint(stage / n) for n in names}}
        metadata = Path(tmp) / "manifest.json"
        metadata.write_text(json.dumps(manifest, sort_keys=True))
        # Exclusive lock and manifest-last publication; an interrupted build fails closed.
        (dest / "manifest.json").unlink(missing_ok=True)
        (dest / "dme-cuda").unlink(missing_ok=True)
        for n in names:
            os.replace(stage / n, dest / n)
        os.replace(metadata, dest / "manifest.json")
    return 0

def verified_engine(dest):
    meta = dest / "manifest.json"
    if not meta.is_file() or meta.is_symlink():
        raise ValueError("Run dme build first")
    m = json.loads(meta.read_text())
    if m["source"] != identity() or m["runtime"] != tools(("ffmpeg", "ffprobe")):
        raise ValueError("Source or FFmpeg executables changed; run dme build")
    for name in ("dme", "dme-cuda"):
        f = dest / name
        if name not in m["binaries"]:
            if f.exists() or f.is_symlink():
                raise ValueError("Unexpected cached binary")
        elif f.is_symlink() or not f.is_file() or fingerprint(f) != m["binaries"][name]:
            raise ValueError("Cached binary failed integrity check; run dme build")
    if "dme" not in m["binaries"]:
        raise ValueError("Missing engine")
    return dest / "dme"

def main():
    args = sys.argv[1:]
    try:
        if platform.system() != "Linux":
            raise ValueError("Only Linux (including WSL) is currently supported")
        if not SKILL.is_dir():
            raise ValueError("Packaged skill source missing")
        if args == ["--version"]:
            print("DME package " + VERSION)
            return 0
        if args == ["skill-path"]:
            print(SKILL)
            return 0
        if not args or args == ["--help"]:
            print("dme build [--gpu] | skill-path | doctor | analyze | enhance | verify-repeat | batch\n"
                  "Run dme build once to compile/test the offline engine.")
            return 0
        dest = cache()
        if args[0] == "build":
            p = argparse.ArgumentParser(prog="dme build")
            p.add_argument("--gpu", action="store_true")
            gpu = p.parse_args(args[1:]).gpu
            with locked(dest, True):
                return build(dest, gpu)
        with locked(dest, False):
            engine = verified_engine(dest)
            return subprocess.call([sys.executable, str(SKILL / "scripts/agent.py"),
                                    "--engine", str(engine), *args])
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as exc:
        print(json.dumps({"status": "error", "error": str(exc)}))
        return 1
    except KeyboardInterrupt:
        print(json.dumps({"status": "error", "error": "Interrupted"}))
        return 130

if __name__ == "__main__":
    sys.exit(main())
