#!/usr/bin/env python3
"""Build allowlisted offline source packages; never publish or install dependencies."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path, required=True)
    args = p.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    py, npm = out / "python-src", out / "npm-src"
    shutil.copytree(ROOT / "packaging/python", py, ignore=shutil.ignore_patterns("__pycache__"))
    shutil.copytree(ROOT / "packaging/npm", npm, ignore=shutil.ignore_patterns("__pycache__"))
    for dest in (py, npm):
        for name in ("LICENSE", "README.md"):
            shutil.copy2(ROOT / name, dest / name)
        skill = dest / "dme_package/skill"
        skill.mkdir(parents=True)
        for name in ("LICENSE", "SKILL.md", "Cargo.toml", "Cargo.lock"):
            shutil.copy2(ROOT / name, skill / name)
        for folder, patterns in {"src":("*.rs",), "scripts":("*.py","*.sh"), "gpu":("*.cu",),
                                 "references":("*.md",), "tests":("*.py","*.sh")}.items():
            (skill/folder).mkdir()
            for pattern in patterns:
                for f in sorted((ROOT/folder).glob(pattern)):
                    if f.is_file() and not f.is_symlink() and f.name not in ("package.py","test_packages.py"):
                        shutil.copy2(f, skill/folder/f.name)
    shutil.copy2(py / "dme_package/launcher.py", npm / "dme_package/launcher.py")
    dist = out / "dist"
    dist.mkdir()
    subprocess.run([sys.executable, "-m", "build", "--no-isolation", "--outdir", str(dist), str(py)], check=True)
    subprocess.run(["npm", "pack", "--ignore-scripts", "--pack-destination", str(dist)], cwd=npm, check=True)
    artifacts = {f.name: hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(dist.iterdir()) if f.is_file()}
    if len(artifacts) != 3:
        raise ValueError("Expected one wheel, one source tarball, and one npm tarball")
    (out/"sha256.json").write_text(json.dumps(artifacts, indent=2)+"\n")
    print(json.dumps(artifacts, indent=2))
if __name__ == "__main__":
    main()
