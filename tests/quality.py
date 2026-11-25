#!/usr/bin/env python3
"""Controlled grayscale reference comparison; timings are observational, not deterministic."""
import argparse
import json
import math
from pathlib import Path
import random
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument("engine", type=Path)
parser.add_argument("--reference", type=Path, help="Optional authorized local reference photograph")
parser.add_argument("--artifacts", type=Path, help="Existing or new directory for QA outputs; no overwrite")
args = parser.parse_args()
engine = args.engine.resolve()
W = H = 256

def ff(*argv):
    return subprocess.check_output(["ffmpeg","-nostdin","-v","error","-cpuflags","0","-threads","1",
                                    "-filter_threads","1",*map(str,argv)], timeout=120)

def decode(path):
    return ff("-i",path,"-vf",f"scale={W}:{H}:flags=lanczos,format=gray",
              "-frames:v","1","-threads","1","-f","rawvideo","-")

def png(path, pixels):
    assert not path.exists()
    subprocess.run(["ffmpeg","-nostdin","-v","error","-n","-f","rawvideo","-pixel_format","gray",
                    "-video_size",f"{W}x{H}","-i","pipe:0","-frames:v","1","-threads","1",str(path)],
                   input=pixels,check=True,timeout=120)

def metrics(a,b):
    assert len(a)==len(b)==W*H
    sse=sum((x-y)**2 for x,y in zip(a,b))
    mse=sse/len(a)
    # Mean SSIM over nonoverlapping 8x8 windows, population covariance, L=255.
    c1,c2=(0.01*255)**2,(0.03*255)**2
    scores=[]
    for y in range(0,H,8):
        for x in range(0,W,8):
            pairs=[(a[j*W+i],b[j*W+i]) for j in range(y,y+8) for i in range(x,x+8)]
            ma=sum(u for u,v in pairs)/64
            mb=sum(v for u,v in pairs)/64
            va=sum((u-ma)**2 for u,v in pairs)/64
            vb=sum((v-mb)**2 for u,v in pairs)/64
            cov=sum((u-ma)*(v-mb) for u,v in pairs)/64
            scores.append((2*ma*mb+c1)*(2*cov+c2)/((ma*ma+mb*mb+c1)*(va+vb+c2)))
    return {"sse":sse,"mse":round(mse,6),"psnr_db":round(10*math.log10(65025/mse),4) if mse else None,
            "ssim_8x8":round(sum(scores)/len(scores),6),"exact_match":not sse}

def benchmark(root):
    if args.reference:
        ref=decode(args.reference.resolve(strict=True))
    else:
        ref=bytes(max(0,min(255,round(125+36*math.sin(x/17)+29*math.cos(y/24)
                          +28*((x//64+y//64)%2)+10*math.sin((x+y)/4))))
                  for y in range(H) for x in range(W))
    rng=random.Random(8675309)
    degraded=bytes(max(0,min(255,x+round(rng.gauss(0,8)))) for x in ref)
    png(root/"reference.png",ref)
    png(root/"degraded.png",degraded)
    # Use the same color conversion, decode path, and output format for every preset.
    rows={}
    for preset in ("resize","clean","denoise","patch","bm3d"):
        out=root/(preset+".png")
        start=time.perf_counter()
        subprocess.run([str(engine),str(root/"degraded.png"),str(out),"1",preset,"keep"],
                       check=True,capture_output=True,timeout=120)
        row=metrics(ref,decode(out))
        row["seconds"]=round(time.perf_counter()-start,4)
        rows[preset]=row
    clean_controls={}
    for preset in ("patch","bm3d"):
        out=root/("clean-"+preset+".png")
        subprocess.run([str(engine),str(root/"reference.png"),str(out),"1",preset,"keep"],
                       check=True,capture_output=True,timeout=120)
        clean_controls[preset]=metrics(ref,decode(out))
    if args.reference is None:
        assert rows["patch"]["sse"]<rows["resize"]["sse"],rows
        assert rows["patch"]["ssim_8x8"]>rows["resize"]["ssim_8x8"],rows
    print(json.dumps({"reference":"local photograph" if args.reference else "synthetic pattern",
                      "dimensions":[W,H],"noise_sigma_fixture":8,"degraded":metrics(ref,degraded),
                      "presets":rows,"clean_controls":clean_controls,"scope":"grayscale controlled noise; not a general quality guarantee"},
                     sort_keys=True,allow_nan=False))

if args.artifacts:
    args.artifacts.mkdir(parents=True,exist_ok=True)
    benchmark(args.artifacts.resolve())
else:
    with tempfile.TemporaryDirectory(prefix="dme-quality-") as directory:
        benchmark(Path(directory))
