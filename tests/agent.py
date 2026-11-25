import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

agent, engine = map(Path, sys.argv[1:3])
def invoke(*args, ok=True):
    r = subprocess.run([sys.executable, str(agent), "--engine", str(engine), *map(str,args)],
                       capture_output=True, text=True, cwd="/tmp")
    assert not r.stderr, r.stderr
    value = json.loads(r.stdout)
    assert (r.returncode == 0) == ok, value
    assert (value["status"] == "ok") == ok, value
    return value

with tempfile.TemporaryDirectory(prefix="dme agent test ") as temp:
    root=Path(temp)
    src=root/"input ; $ data.png"
    subprocess.run(["ffmpeg","-v","error","-f","lavfi","-i","testsrc2=size=64x48:rate=1",
                    "-frames:v","1","-threads","1",str(src)],check=True)
    original=hashlib.sha256(src.read_bytes()).hexdigest()
    target=root/"output ; $ data.png"
    assert invoke("doctor")["engine_ready"]
    assert "selected" in invoke("analyze","--input",src)["analysis"]
    result=invoke("enhance","--input",src,"--output",target,"--scale",2)
    assert result["sha256"]==hashlib.sha256(target.read_bytes()).hexdigest()
    assert result["input_sha256"]==original
    assert hashlib.sha256(src.read_bytes()).hexdigest()==original
    invoke("enhance","--input",src,"--output",target,ok=False)
    assert result["sha256"]==hashlib.sha256(target.read_bytes()).hexdigest()
    assert invoke("verify-repeat","--input",src,"--scale",2)["match"]
    invoke("enhance","--input",src,"--output",root/"bad.png","--fps","60",ok=False)
    invoke("enhance","--input",src,"--output",root/"bad.png","--scale",3,ok=False)
    invoke("analyze","--input",root/"missing.png",ok=False)
    invoke("enhance","--input",src,"--output",root/"bad.mkv",ok=False)
    bad=root/"invalid.png";bad.write_text("invalid")
    invoke("enhance","--input",bad,"--output",root/"bad.png",ok=False)
    assert not (root/"bad.png").exists()
    video=root/"source.mkv"
    subprocess.run(["ffmpeg","-v","error","-f","lavfi","-i","testsrc2=size=64x48:rate=24",
                    "-t","0.5","-c:v","ffv1","-threads","1",str(video)],check=True)
    v=invoke("enhance","--input",video,"--output",root/"out.mkv","--preset","resize","--fps","60")
    assert any(r.get("video_guard")=="timeline" and r.get("accepted") for r in v["reports"])
    assert invoke("verify-repeat","--input",video,"--preset","resize")["match"]
print("Agent JSON, local paths, hashes, repeatability, video reports and negative cases passed.")
