import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

agent, engine = map(Path, sys.argv[1:3])

def invoke(*args, ok=True, executable=engine):
    result = subprocess.run([sys.executable, str(agent), "--engine", str(executable), *map(str,args)],
                            cwd="/tmp", capture_output=True, text=True)
    assert not result.stderr, result.stderr
    value = json.loads(result.stdout)
    assert (result.returncode == 0) == ok, value
    assert (value["status"] == "ok") == ok, value
    return value

with tempfile.TemporaryDirectory(prefix="dme batch ") as tmp:
    root = Path(tmp)
    src = root/"source ; $.png"
    subprocess.run(["ffmpeg","-v","error","-f","lavfi","-i","testsrc2=size=64x48:rate=1",
                    "-frames:v","1","-threads","1",str(src)], check=True)
    original = hashlib.sha256(src.read_bytes()).hexdigest()
    video = root/"source.mkv"
    subprocess.run(["ffmpeg","-v","error","-f","lavfi","-i","testsrc2=size=64x48:rate=24",
                    "-t","0.5","-threads","1","-c:v","ffv1",str(video)], check=True)
    (root/"bad.png").write_text("invalid image")
    manifest = root/"jobs.json"
    def write(jobs):
        manifest.write_text(json.dumps({"jobs":jobs}),encoding="utf-8")
    def job(name, out):
        return {"input":name,"output":out,"preset":"resize","scale":2}
    jobs = [job(src.name,"first.png"), job("bad.png","failed.png"),job(video.name,"last.mkv")]
    write(jobs)
    plan = invoke("batch","--manifest",manifest,"--dry-run")
    assert [j["status"] for j in plan["jobs"]] == ["planned"]*3
    assert not (root/"first.png").exists()
    result = invoke("batch","--manifest",manifest,ok=False)
    assert [j["status"] for j in result["jobs"]] == ["ok","error","ok"]
    assert result["succeeded"] == 2 and result["failed"] == 1
    assert not (root/"failed.png").exists()
    assert hashlib.sha256(src.read_bytes()).hexdigest() == original
    for index in (0,2):
        rec = result["jobs"][index]
        assert rec["sha256"] == hashlib.sha256(Path(rec["output"]).read_bytes()).hexdigest()
        assert rec["engine_sha256"] == hashlib.sha256(engine.read_bytes()).hexdigest()
    write([job(src.name,"copy.png"),job(video.name,"copy.mkv")])
    repeat = invoke("batch","--manifest",manifest)
    assert repeat["jobs"][0]["sha256"] == result["jobs"][0]["sha256"]
    assert repeat["jobs"][1]["sha256"] == result["jobs"][2]["sha256"]
    alias=root/"alias";alias.symlink_to(root,target_is_directory=True)
    write([job(src.name,"collision.png"),job(src.name,"alias/collision.png")])
    invoke("batch","--manifest",manifest,ok=False)
    assert not (root/"collision.png").exists()
    write([job(src.name,"never.png"),job(src.name,"first.png")])
    invoke("batch","--manifest",manifest,ok=False)
    assert not (root/"never.png").exists()
    write([job(src.name,"never.png"),dict(job(src.name,"wrong.png"),scale=True)])
    invoke("batch","--manifest",manifest,ok=False)
    assert not (root/"never.png").exists()
    manifest.write_text('{"jobs":[],"jobs":[]}')
    invoke("batch","--manifest",manifest,ok=False)
    fake=root/"fake-engine"
    fake.write_text("#!/usr/bin/env python3\nimport pathlib,sys\nif sys.argv[1]=='--version': print('fake 1'); sys.exit(0)\npathlib.Path(sys.argv[2]).write_bytes(b'fake')\npathlib.Path(sys.argv[1]).write_bytes(b'changed')\n")
    fake.chmod(0o700)
    disposable=root/"disposable.png";disposable.write_bytes(src.read_bytes())
    invoke("enhance","--input",disposable,"--output",root/"unverified.png",ok=False,executable=fake)
    assert not (root/"unverified.png").exists()
    assert not list(root.glob(".dme-agent-*"))
print("Batch ordering, preflight, alias collisions, runtime isolation, hashes and staging passed.")
