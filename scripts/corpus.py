#!/usr/bin/env python3
"""Paired local reference benchmark. No downloads, no automatic file discovery."""
import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import agent

FORMATS={".png":"png_pipe",".jpg":"jpeg_pipe",".jpeg":"jpeg_pipe",".bmp":"bmp_pipe",
         ".tif":"tiff_pipe",".tiff":"tiff_pipe",".webp":"webp_pipe",
         ".mkv":"matroska,webm",".webm":"matroska,webm",
         ".mp4":"mov,mp4,m4a,3gp,3g2,mj2",".mov":"mov,mp4,m4a,3gp,3g2,mj2",".avi":"avi"}
LIMIT=64*1024*1024

def path_of(value,base):
    if not isinstance(value,str) or not value:raise ValueError("Invalid media path")
    p=(base/Path(value)).resolve(strict=True)
    if not p.is_file() or p.suffix.lower() not in FORMATS:raise ValueError("Unsupported media file")
    return p

def input_args(path):
    return ["-protocol_whitelist","file,pipe","-format_whitelist",FORMATS[path.suffix.lower()],"-i",str(path)]

def decode(path,dest):
    # Write a bounded raw stream, not a potentially unbounded captured buffer.
    cmd=["ffmpeg","-nostdin","-v","error","-xerror","-cpuflags","0","-threads","1",*input_args(path),
         "-map","0:v:0","-an","-filter_threads","1","-threads","1","-pix_fmt","gray",
         "-fps_mode","passthrough","-f","rawvideo","-fs",str(LIMIT+1),"-n",str(dest)]
    q=subprocess.run(cmd,stdin=subprocess.DEVNULL,capture_output=True,timeout=180)
    if q.returncode or not dest.is_file() or not 0<dest.stat().st_size<=LIMIT:
        raise ValueError("Decode failed or exceeds 64 MiB sample budget")
    # FFprobe geometry/frame count; reject a truncated stream, not a partial score.
    q=subprocess.run(["ffprobe","-v","error","-protocol_whitelist","file,pipe","-format_whitelist",
        FORMATS[path.suffix.lower()],"-select_streams","v:0","-count_frames","-show_entries",
        "stream=width,height,nb_read_frames","-of","json",str(path)],capture_output=True,timeout=180)
    if q.returncode:raise ValueError("Frame count failed")
    a=agent.strict_json(q.stdout.decode())["streams"]
    if len(a)!=1:raise ValueError("Missing stream")
    w,h,n=int(a[0]["width"]),int(a[0]["height"]),int(a[0]["nb_read_frames"])
    if min(w,h,n)<=0 or w*h*n!=dest.stat().st_size:raise ValueError("Geometry or full sample count mismatch")
    return w,h,n

def score(a,b):
    sse=n=0
    with a.open("rb") as x,b.open("rb") as y:
        while True:
            u=x.read(262144);v=y.read(262144)
            if len(u)!=len(v):raise ValueError("Reference length differs")
            if not u:break
            n+=len(u);sse+=sum((i-j)**2 for i,j in zip(u,v))
    if not n:raise ValueError("Empty sample")
    mse=sse/n
    return {"samples":n,"sse":sse,"mse":mse,"psnr_db":10*math.log10(65025/mse) if mse else None}

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--manifest",type=Path,required=True)
    p.add_argument("--output",type=Path,required=True)
    p.add_argument("--engine",type=Path,default=agent.DEFAULT_ENGINE)
    a=p.parse_args()
    manifest=a.manifest.resolve(strict=True)
    with manifest.open("rb") as f:raw=f.read(8*1024*1024+1)
    if len(raw)>8*1024*1024:raise ValueError("Manifest too large")
    m=agent.strict_json(raw.decode())
    if not isinstance(m,dict) or set(m)!={"cases"} or not isinstance(m["cases"],list) or not 1<=len(m["cases"])<=10000:
        raise ValueError("Expected 1..10000 cases")
    rows=[]
    if a.output.exists() or a.output.is_symlink():raise ValueError("Output exists")
    for i,case in enumerate(m["cases"],1):
        row={"index":i}
        try:
            if not isinstance(case,dict) or set(case)-{"input","reference","preset","scale","label"} or not {"input","reference","preset"}<=set(case):raise ValueError("Invalid case")
            src=path_of(case["input"],manifest.parent);ref=path_of(case["reference"],manifest.parent)
            preset=case["preset"];scale=case.get("scale",1)
            if preset not in agent.PRESETS or type(scale) is not int or scale not in (1,2,4,8):raise ValueError("Invalid preset/scale")
            if (src.suffix.lower() in agent.IMAGES)!=(ref.suffix.lower() in agent.IMAGES):raise ValueError("Media types differ")
            suffix=".png" if src.suffix.lower() in agent.IMAGES else ".mkv"
            row.update(label=str(case.get("label",i)),preset=preset,scale=scale,input_sha256=agent.fingerprint(src),reference_sha256=agent.fingerprint(ref))
            with tempfile.TemporaryDirectory(prefix="dme-corpus-") as t:
                t=Path(t);refraw=t/"ref.raw";geom=decode(ref,refraw)
                for mode,tag in (("resize","baseline"),(preset,"candidate")):
                    target=t/(tag+suffix)
                    report=agent.process_job(a.engine.resolve(strict=True),src,target,scale,mode,"keep")
                    decoded=t/(tag+".raw")
                    if decode(target,decoded)!=geom:raise ValueError("Reference geometry/frame count differs")
                    row[tag]=score(refraw,decoded)
                    row[tag]["output_sha256"]=report["sha256"]
                row["status"]="ok"
        except (OSError,ValueError,RuntimeError,agent.JobError,subprocess.SubprocessError) as e:
            row.update(status="error",error=str(e))
        rows.append(row);print(f"{i}/{len(m['cases'])}: {row['status']}",file=sys.stderr,flush=True)
    good=[r for r in rows if r["status"]=="ok"]
    improved=sum(r["candidate"]["sse"]<r["baseline"]["sse"] for r in good)
    equal=sum(r["candidate"]["sse"]==r["baseline"]["sse"] for r in good)
    result={"scope":"8-bit grayscale, paired frame-index reference; caller must ensure temporal alignment; not HDR/alpha/perceptual QA",
            "total":len(rows),"completed":len(good),"improved":improved,"unchanged":equal,"worse":len(good)-improved-equal,
            "failed":len(rows)-len(good),"cases":rows}
    with a.output.open("x",encoding="utf-8") as f:json.dump(result,f,sort_keys=True,allow_nan=False)
    print(json.dumps({k:v for k,v in result.items() if k!="cases"}))
    return 0 if len(good)==len(rows) else 1
if __name__=="__main__":
    try:sys.exit(main())
    except (OSError,ValueError,RuntimeError,agent.JobError,subprocess.SubprocessError) as e:print(json.dumps({"status":"error","error":str(e)}));sys.exit(1)
