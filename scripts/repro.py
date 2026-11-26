#!/usr/bin/env python3
"""Portable, offline reproducibility evidence. Use the same fixture bundle on each host."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from agent import strict_json, JobError, validate_reports, split_reports

ROOT = Path(__file__).resolve().parent.parent
DEFAULT = ROOT / "build/skill/dme"
SCHEMA = "dme-repro-v1"
GOLDEN = "a7bf1450aade6ec752e2faee5d43c0ceee667fea0f4940327024884739141808"
W, H = 96, 80
CASES = [
    (f"image-{p}", "image.bmp", "png", 2, p, "keep")
    for p in ("auto","resize","clean","detail","restore","denoise","regional","deblur","wavelet","patch","bm3d")
] + [
    (f"video-{p}", "video.mkv", "mkv", 2, p, "keep")
    for p in ("auto","resize","temporal","stabilize","multiframe","bm3d")
] + [("video-fps60","video.mkv","mp4",1,"resize","60"),
     ("video-bm3d-fps30","video.mkv","mkv",1,"bm3d","30"),
     ("image-blind","image.bmp","png",1,"blind","keep"),
     ("image-motion-deblur","image.bmp","png",1,"motion-deblur","keep"),
     ("video-local","video.mkv","mkv",2,"multiframe-local","keep")]
ENV = dict(os.environ, LC_ALL="C", LANG="C", TZ="UTC")
HEX = re.compile(r"[a-f0-9]{64}\Z")

def sha(data):
    return hashlib.sha256(data).hexdigest()

def filehash(path):
    digest=hashlib.sha256()
    with Path(path).open("rb") as handle:
        for chunk in iter(lambda:handle.read(1024*1024),b""):digest.update(chunk)
    return digest.hexdigest()

def canonical(value):
    return json.dumps(value,sort_keys=True,separators=(",",":"),ensure_ascii=True,allow_nan=False).encode("ascii")

def command(*args):
    p = subprocess.run(list(map(str,args)), stdin=subprocess.DEVNULL, capture_output=True,
                       env=ENV, timeout=180, check=False)
    if p.returncode:
        raise RuntimeError(f"{Path(args[0]).name} failed: {p.stderr.decode(errors='replace')[:2000]}")
    return p

def ff(*args):
    return command("ffmpeg","-nostdin","-v","error","-xerror","-cpuflags","0","-threads","1",
                   "-fflags","+bitexact",*args).stdout

def pixels(frame):
    seed=9
    buf=bytearray()
    for y in range(H):
        for x in range(W):
            seed=(seed*1664525+1013904223)&0xffffffff
            # Integer-only texture and deterministic noise, shifted between frames.
            v=50+((2*x+frame)//8)*4+((2*y+frame)//16)*3+((seed>>24)%19)-9
            buf.extend((max(0,min(255,v+11)),max(0,min(255,v)),max(0,min(255,v-7))))
    return bytes(buf)

def prepare(folder):
    folder=folder.resolve()
    folder.mkdir(parents=True,exist_ok=False)
    row=W*3
    rgb=pixels(0)
    bgr=b"".join(bytes((rgb[i+2],rgb[i+1],rgb[i])) for y in range(H-1,-1,-1)
                 for i in range(y*row,(y+1)*row,3))
    # Width 96 makes each BMP row 4-byte aligned.
    bmp=struct.pack("<2sIHHI",b"BM",54+len(bgr),0,0,54)
    bmp+=struct.pack("<IiiHHIIiiII",40,W,H,1,24,0,len(bgr),0,0,0,0)+bgr
    (folder/"image.bmp").write_bytes(bmp)
    with tempfile.TemporaryDirectory(prefix="dme-fixture-") as temp:
        raw=Path(temp)/"frames.rgb"; pcm=Path(temp)/"sound.pcm"
        raw.write_bytes(b"".join(pixels(i) for i in range(6)))
        pcm.write_bytes(b"".join(struct.pack("<h",((i%120)-60)*120) for i in range(12000)))
        ff("-n","-f","rawvideo","-pixel_format","rgb24","-video_size",f"{W}x{H}",
           "-framerate","24","-i",raw,"-f","s16le","-ar","48000","-ac","1","-i",pcm,
           "-map","0:v:0","-map","1:a:0","-map_metadata","-1","-threads","1","-c:v","ffv1",
           "-pix_fmt","bgr0","-c:a","flac","-flags:v","+bitexact","-flags:a","+bitexact",
           "-fflags","+bitexact",folder/"video.mkv")
    return {name:filehash(folder/name) for name in ("image.bmp","video.mkv")}

def cpu():
    result="unknown"
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            key,sep,value=line.partition(":")
            if sep and key.strip() in ("model name","Hardware"):
                result=value.strip();break
    except OSError:
        pass
    return {"architecture":platform.machine(),"cpu_model":result,
            "os":platform.system(),"kernel":platform.release(),
            "affinity":sorted(os.sched_getaffinity(0)) if hasattr(os,"sched_getaffinity") else None}

def tool(name):
    path=shutil.which(name)
    if not path:raise RuntimeError("Missing "+name)
    libraries={}
    if shutil.which("ldd"):
        info=command("ldd",path).stdout.decode(errors="replace")
        for line in info.splitlines():
            words=line.strip().split()
            lib=words[2] if len(words)>2 and words[1]=="=>" else (words[0] if words else "")
            if lib.startswith("/") and Path(lib).is_file():
                libraries[Path(lib).name]=filehash(lib)
    return {"sha256":filehash(path),"version":command(path,"-version").stdout.decode(errors="replace").splitlines()[0],
            "linked_libraries":libraries}

def source_identity():
    paths=[ROOT/"Cargo.toml",ROOT/"Cargo.lock",ROOT/"scripts/repro.py",ROOT/"scripts/agent.py",*sorted((ROOT/"src").glob("*.rs"))]
    return sha(canonical({str(p.relative_to(ROOT)):filehash(p) for p in paths}))

def decoded(path,image):
    pix="rgba64be" if image else "yuv444p16le"
    video=ff("-i",path,"-map","0:v:0","-an","-filter_threads","1","-threads","1",
             "-pix_fmt",pix,"-fps_mode","passthrough","-f","rawvideo","pipe:1")
    audio=None if image else sha(ff("-i",path,"-map","0:a:0","-vn","-threads","1","-c:a","pcm_s32le","-f","s32le","pipe:1"))
    return {"video":sha(video),"audio":audio}

def record(engine,folder):
    engine=engine.resolve();folder=folder.resolve()
    fixtures={name:filehash(folder/name) for name in ("image.bmp","video.mkv")}
    identity=source_identity()
    engine_id={"sha256":filehash(engine),"version":command(engine,"--version").stdout.decode().strip()}
    tools={name:tool(name) for name in ("ffmpeg","ffprobe")}
    kern=command(engine,"--kernel-fixture").stdout
    if len(kern)!=4096 or sha(kern)!=GOLDEN or command(engine,"--kernel-fixture").stdout!=kern:
        raise RuntimeError("Integer kernel golden fixture mismatch")
    rows={}
    with tempfile.TemporaryDirectory(prefix="dme-repro-") as tmp:
        tmp=Path(tmp)
        for name,src,ext,scale,preset,fps in CASES:
            runs=[]
            for i in (0,1):
                out=tmp/f"{name}-{i}.{ext}"
                p=command(engine,folder/src,out,scale,preset,fps)
                reports,_=split_reports(p.stderr.decode(errors="replace"))
                validate_reports(reports,ext=="png",scale,fps,preset)
                if out.stat().st_size<=0:raise RuntimeError("Empty output for "+name)
                runs.append({"file":filehash(out),"bytes":out.stat().st_size,"decoded":decoded(out,ext=="png"),
                             "reports":sha(canonical(reports))})
            if runs[0]!=runs[1]:raise RuntimeError("Local repeat mismatch: "+name)
            rows[name]=runs[0]
            print("PASS "+name,file=sys.stderr,flush=True)
    if fixtures!={name:filehash(folder/name) for name in fixtures}:raise RuntimeError("Fixture changed")
    if identity!=source_identity() or engine_id["sha256"]!=filehash(engine) or tools!={name:tool(name) for name in tools}:
        raise RuntimeError("Source or toolchain changed during experiment")
    return {"schema":SCHEMA,"source_sha256":identity,"environment":cpu(),
            "engine":engine_id,"tools":tools,
            "fixtures":fixtures,"kernel_sha256":sha(kern),"repetitions":2,"cases":rows}

def digest(value):
    return isinstance(value,str) and bool(HEX.fullmatch(value))

def validate(m):
    if not isinstance(m,dict) or m.get("schema")!=SCHEMA:raise ValueError("Unknown manifest schema")
    if not digest(m.get("source_sha256")) or not digest(m.get("kernel_sha256")):raise ValueError("Invalid source/kernel identity")
    if type(m.get("repetitions")) is not int or m["repetitions"]!=2:raise ValueError("Missing two-run evidence")
    if not isinstance(m.get("environment"),dict):raise ValueError("Missing environment")
    for key in ("architecture","cpu_model","os","kernel"):
        if not isinstance(m["environment"].get(key),str):raise ValueError("Invalid environment")
    if not isinstance(m.get("tools"),dict) or set(m["tools"])!={"ffmpeg","ffprobe"}:
        raise ValueError("Missing tool identities")
    for item in (m.get("engine"),*(m["tools"].get(n) for n in ("ffmpeg","ffprobe"))):
        if not isinstance(item,dict) or not digest(item.get("sha256")) or not isinstance(item.get("version"),str):
            raise ValueError("Invalid tool identity")
    for item in m["tools"].values():
        libs=item.get("linked_libraries")
        if not isinstance(libs,dict) or not all(digest(v) for v in libs.values()):
            raise ValueError("Invalid linked library identities")
    f=m.get("fixtures")
    if not isinstance(f,dict) or set(f)!={"image.bmp","video.mkv"} or not all(map(digest,f.values())):
        raise ValueError("Invalid fixture identities")
    c=m.get("cases")
    if not isinstance(c,dict) or set(c)!={x[0] for x in CASES}:raise ValueError("Incomplete case set")
    for name,_,ext,_,_,_ in CASES:
        row=c[name]
        if (not isinstance(row,dict) or not digest(row.get("file")) or not digest(row.get("reports"))
                or type(row.get("bytes")) is not int or row["bytes"]<=0):
            raise ValueError("Invalid case digest")
        dec=row.get("decoded")
        if not isinstance(dec,dict) or not digest(dec.get("video")) or (dec.get("audio") is not None if ext=="png" else not digest(dec.get("audio"))):
            raise ValueError("Invalid decoded digest")

def compare(a,b):
    validate(a);validate(b)
    source_equal=a["source_sha256"]==b["source_sha256"]
    fixtures_equal=a["fixtures"]==b["fixtures"]
    comparable=source_equal and fixtures_equal
    differences=[name for name in sorted(a["cases"]) if a["cases"][name]!=b["cases"][name]]
    kernel_match=a["kernel_sha256"]==b["kernel_sha256"]==GOLDEN
    cpu_a,cpu_b=a["environment"],b["environment"]
    distinct_cpu=(cpu_a["architecture"]!=cpu_b["architecture"] or
                  (cpu_a["cpu_model"]!="unknown" and cpu_b["cpu_model"]!="unknown" and cpu_a["cpu_model"]!=cpu_b["cpu_model"]))
    match=comparable and kernel_match and not differences
    return {"status":"ok" if match else "error","match":match,"comparable":comparable,
            "source_equal":source_equal,"fixtures_equal":fixtures_equal,"kernel_match":kernel_match,
            "different_cpu_metadata":distinct_cpu,
            "same_executable_hashes":a["engine"]["sha256"]==b["engine"]["sha256"] and all(a["tools"][n]["sha256"]==b["tools"][n]["sha256"] for n in a["tools"]),
            "same_recorded_toolchain":a["engine"]==b["engine"] and a["tools"]==b["tools"],
            "same_environment_metadata":cpu_a==cpu_b,"different_cases":differences,
            "case_count":len(a["cases"]),
            "scope":"Only compared fixtures and reported environments; not universal or independently authenticated hardware proof"}

def main():
    p=argparse.ArgumentParser(description=__doc__)
    sub=p.add_subparsers(dest="mode",required=True)
    prep=sub.add_parser("prepare");prep.add_argument("--fixtures",type=Path,required=True)
    rec=sub.add_parser("record");rec.add_argument("--engine",type=Path,default=DEFAULT)
    rec.add_argument("--fixtures",type=Path,required=True);rec.add_argument("--output",type=Path,required=True)
    comp=sub.add_parser("compare");comp.add_argument("first",type=Path);comp.add_argument("second",type=Path)
    a=p.parse_args()
    if a.mode=="prepare":return {"status":"ok","fixtures":prepare(a.fixtures)}
    if a.mode=="record":
        if a.output.exists() or a.output.is_symlink():raise ValueError("Manifest output exists")
        m=record(a.engine,a.fixtures);validate(m)
        with a.output.open("xb") as handle:handle.write(canonical(m)+b"\n")
        return {"status":"ok","manifest":str(a.output),"cases":len(m["cases"]),"kernel_sha256":m["kernel_sha256"]}
    def load(path):
        with path.open("rb") as handle:
            data=handle.read(1024*1024+1)
        if len(data)>1024*1024:raise ValueError("Manifest exceeds 1 MiB")
        return strict_json(data.decode("utf-8"))
    return compare(load(a.first),load(a.second))

if __name__=="__main__":
    try:r=main()
    except (OSError,ValueError,RuntimeError,JobError,subprocess.SubprocessError) as e:r={"status":"error","error":str(e)}
    print(json.dumps(r,sort_keys=True,allow_nan=False))
    sys.exit(0 if r["status"]=="ok" else 1)
