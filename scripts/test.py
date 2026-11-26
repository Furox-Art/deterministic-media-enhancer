#!/usr/bin/env python3
"""Run the preserved media integration suite against a native engine."""
import concurrent.futures
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
engine = Path(sys.argv[1]).resolve()
agent = root / "scripts/agent.py"
shell_tests = ("reproducibility", "adaptive", "optimizer_integration",
               "restoration", "video", "multiframe")
python_tests = ("agent", "batch", "hardening", "formats", "advanced")
jobs = [(name, ["bash", root / "tests" / (name + ".sh"), engine]) for name in shell_tests]
jobs += [(name, [sys.executable, root / "tests" / (name + ".py"), agent, engine])
         for name in python_tests]
jobs += [("quality", [sys.executable, root / "tests/quality.py", engine])]

def test(job):
    name, command = job
    try:
        r = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=300)
        return name, r.returncode, r.stdout + r.stderr
    except subprocess.TimeoutExpired:
        return name, 1, "Test timed out"

failed = 0
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
    for name, code, log in pool.map(test, jobs):
        print(f"{'PASS' if code == 0 else 'FAIL'} {name}", flush=True)
        if code:
            failed += 1
            print(log, flush=True)
        elif name == "quality":
            print(log.strip(), flush=True)
print(f"{len(jobs)-failed}/{len(jobs)} integration suites passed", flush=True)
sys.exit(bool(failed))
