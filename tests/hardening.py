import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time

agent, engine = map(Path, sys.argv[1:3])

def invoke(*args, executable=engine, ok=True, timeout=3600):
    proc = subprocess.run([sys.executable, str(agent), "--engine", str(executable),
                           "--timeout-seconds", str(timeout), *map(str,args)],
                          capture_output=True, text=True, timeout=30)
    assert not proc.stderr, proc.stderr
    value = json.loads(proc.stdout, parse_constant=lambda x: (_ for _ in ()).throw(ValueError(x)))
    assert (proc.returncode == 0) == ok, value
    assert (value["status"] == "ok") == ok, value
    return value

with tempfile.TemporaryDirectory(prefix="dme hardening ") as tmp:
    root = Path(tmp)
    src = root/"image.png"
    video = root/"video.mkv"
    for dest, tail in [(src,["-frames:v","1"]), (video,["-t","0.5","-c:v","ffv1"])]:
        subprocess.run(["ffmpeg","-v","error","-f","lavfi","-i","testsrc2=size=64x48:rate=24",
                        "-threads","1",*tail,str(dest)],check=True)
    result = invoke("enhance","--input",src,"--output",root/"real.png","--scale",4,"--preset","resize")
    guard = [r for r in result["reports"] if "image_guard" in r][0]
    assert (guard["input_width"],guard["input_height"],guard["output_width"],guard["output_height"]) == (64,48,256,192)
    assert all(result["tool_sha256"][name] == hashlib.sha256(Path(__import__("shutil").which(name)).read_bytes()).hexdigest()
               for name in ("ffmpeg","ffprobe"))
    report = '{"video_guard":"timeline","accepted":true,"input_frames":12,"output_frames":12,"input_duration_us":500000,"output_duration_us":500000,"target_fps":0}'
    geometry = '{"image_guard":"geometry","accepted":true,"input_width":64,"input_height":48,"output_width":64,"output_height":48}'
    fake = root/"fake-engine"
    def make_fake(body):
        fake.write_text("#!/usr/bin/env python3\nimport pathlib,sys,subprocess,time\n"
                        "if sys.argv[1]=='--version': print('fake 1'); sys.exit(0)\n"+body)
        fake.chmod(0o700)
    def reject_report(source, suffix, lines, error):
        make_fake("pathlib.Path(sys.argv[2]).write_bytes(b'fake')\n"
                  +"sys.stderr.write("+repr(lines+"\n")+")\n")
        output=root/("reject."+suffix)
        response=invoke("enhance","--input",source,"--output",output,"--preset","resize",executable=fake,ok=False)
        assert error in response["error"], response
        assert not output.exists()
    reject_report(video,"mkv",report.replace('"accepted":true','"accepted":false,"accepted":true'),"Duplicate JSON key")
    reject_report(video,"mkv",report+"\n"+report.replace("true","false"),"duplicate")
    reject_report(video,"mkv",report.replace("500000","NaN",1),"Non-finite")
    reject_report(video,"mkv",report.replace("500000","1e999",1),"Non-finite")
    reject_report(video,"mkv",report.replace('"input_frames":12','"input_frames":true'),"Invalid video timeline")
    reject_report(video,"mkv",'{"other":true}',"Missing")
    reject_report(src,"png",geometry.replace('"output_width":64','"output_width":65'),"Invalid image geometry")
    reject_report(src,"png",geometry+"\n"+geometry,"duplicate")
    reject_report(src,"png",'[1,2]',"must be an object")
    # FFmpeg diagnostics are distinct from JSON arrays; old engine is not ready.
    make_fake("sys.stderr.write('[decoder @ 0x123abc] Format not on whitelist\\n')\nsys.exit(2)\n")
    response=invoke("enhance","--input",src,"--output",root/"log.png","--preset","resize",executable=fake,ok=False)
    assert "Format not on whitelist" in response["diagnostics"][0],response
    assert not (root/"log.png").exists()
    stale=invoke("doctor",executable=fake,ok=False)
    assert stale["engine_ready"] and not stale["engine_version_match"],stale
    manifest=root/"jobs.json"
    manifest.write_text('{"jobs":[{"input":"image.png","output":"never.png","scale":NaN}]}')
    assert "Non-finite" in invoke("batch","--manifest",manifest,ok=False)["error"]
    marker=root/"child-survived"
    child_code="import pathlib,time;time.sleep(3);pathlib.Path("+repr(str(marker))+").write_text('bad')"
    make_fake("subprocess.Popen([sys.executable,'-c',"+repr(child_code)+"])\ntime.sleep(20)\n")
    response=invoke("enhance","--input",src,"--output",root/"timeout.png","--preset","resize",executable=fake,ok=False,timeout=1)
    assert "timeout" in response["error"].lower(), response
    time.sleep(3.1)
    assert not marker.exists() and not (root/"timeout.png").exists()
    make_fake("sys.stderr.write('x'*(9*1024*1024))\n")
    response=invoke("enhance","--input",src,"--output",root/"limit.png","--preset","resize",executable=fake,ok=False)
    assert "output limit" in response["error"],response
    assert not (root/"limit.png").exists()
    # Concurrent publication: exactly one winner; never replace the winner.
    target=root/"race.png"
    cmd=[sys.executable,str(agent),"--engine",str(engine),"enhance","--input",str(src),"--output",str(target),"--preset","resize"]
    procs=[subprocess.Popen(cmd,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) for _ in range(2)]
    values=[]
    for p in procs:
        out,err=p.communicate(timeout=30)
        assert not err,err
        values.append(json.loads(out))
    winners=[v for v in values if v["status"]=="ok"]
    assert len(winners)==1 and hashlib.sha256(target.read_bytes()).hexdigest()==winners[0]["sha256"]
    assert not list(root.glob(".dme-agent-*"))
print("Strict JSON, contradictory guards, image geometry, tool hashes, process group timeout, bounded logs and publication race passed.")
