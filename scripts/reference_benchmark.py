#!/usr/bin/env python3
"""Offline ZIP-based real-reference kernel benchmark. Pillow is QA-only."""
import argparse, hashlib, io, json, pathlib, subprocess, sys, tempfile, zipfile
from collections import defaultdict
from PIL import Image, ImageOps, __version__ as pillow_version

def sha(b):return hashlib.sha256(b).hexdigest()
def hashfile(p):
    h=hashlib.sha256()
    with p.open("rb") as f:
        for b in iter(lambda:f.read(1048576),b""):h.update(b)
    return h.hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--engine",type=pathlib.Path,required=True)
    p.add_argument("--archive",type=pathlib.Path,nargs="+",required=True)
    p.add_argument("--output",type=pathlib.Path,required=True)
    p.add_argument("--size",type=int,choices=(64,128,256),default=64)
    p.add_argument("--per-class",type=int,default=0,help="Select at most N members per parent directory")
    p.add_argument("--motion",action="store_true")
    a=p.parse_args()
    if a.output.exists() or a.output.is_symlink():raise ValueError("Output exists")
    if not 0<=a.per_class<=10000:raise ValueError("Invalid per-class limit")
    if len({p.name for p in a.archive})!=len(a.archive):raise ValueError("Archive basenames must be unique")
    refs=[];seen=set();errors=[];archives=[]
    with tempfile.TemporaryDirectory(prefix="dme-reference-") as tmp:
        raw=pathlib.Path(tmp)/"references.raw"
        with raw.open("xb") as f:
            for path in a.archive:
                digest=hashfile(path);archives.append({"file":path.name,"sha256":digest})
                with zipfile.ZipFile(path) as z:
                    groups=defaultdict(list)
                    for member in z.infolist():
                        name=pathlib.PurePosixPath(member.filename)
                        if not member.is_dir() and name.suffix.lower() in (".png",".jpg",".jpeg",".bmp",".tif",".tiff") and not any(part.startswith(".") or part=="__MACOSX" for part in name.parts):
                            groups[str(name.parent)].append(member)
                    for group in sorted(groups):
                        members=sorted(groups[group],key=lambda m:(sha(m.filename.encode()),m.filename))
                        for m in members[:a.per_class or len(members)]:
                            try:
                                if m.file_size>32*1024*1024:raise ValueError("Member too large")
                                data=z.read(m)
                                with Image.open(io.BytesIO(data)) as im:
                                    im=ImageOps.exif_transpose(im).convert("L")
                                    im=ImageOps.fit(im,(a.size,a.size),method=Image.Resampling.BOX)
                                    pixels=im.tobytes()
                                h=sha(pixels)
                                if h in seen:continue
                                seen.add(h);f.write(pixels)
                                refs.append({"archive":path.name,"member":m.filename,"source_sha256":sha(data),"reference_sha256":h,"group":group})
                            except (OSError,ValueError) as e:errors.append({"member":m.filename,"error":str(e)})
                            if len(refs)>10000:raise ValueError("Too many references")
        if not refs:raise ValueError("No reference images")
        cmd=[str(a.engine.resolve()),"--reference-benchmark",str(raw),str(a.size),str(len(refs)),"motion" if a.motion else "noise"]
        proc=subprocess.run(cmd,capture_output=True,text=True,timeout=600,check=True)
        rows=[json.loads(x) for x in proc.stdout.splitlines()]
        if len(rows)!=len(refs) or any(x["index"]!=i for i,x in enumerate(rows)):raise ValueError("Kernel result count/order mismatch")
        for ref,row in zip(refs,rows):ref.update(row)
        def summary(prefix):
            x=[r[prefix+"_baseline_sse"] for r in refs];y=[r[prefix+"_candidate_sse"] for r in refs]
            return {"before":sum(x),"after":sum(y),"improved":sum(b<a for a,b in zip(x,y)),
                    "equal":sum(a==b for a,b in zip(x,y)),"worse":sum(b>a for a,b in zip(x,y))}
        result={"scope":"distinct decoded grayscale reference patches; source-scene independence not established; no training",
                "size":a.size,"pillow":pillow_version,"archives":archives,"engine_sha256":hashfile(a.engine),
                "references":len(refs),"noise":summary("noise"),"clean_mean_mse":sum(r["clean_sse"] for r in refs)/(len(refs)*a.size*a.size),
                "decode_failures":errors,"cases":refs}
        x=[r["noise_baseline_sse"] for r in refs];y=[r["soft_candidate_sse"] for r in refs]
        result["soft"]={"before":sum(x),"after":sum(y),"improved":sum(b<a for a,b in zip(x,y)),"equal":sum(a==b for a,b in zip(x,y)),"worse":sum(b>a for a,b in zip(x,y)),"clean_mean_mse":sum(r["soft_clean_sse"] for r in refs)/(len(refs)*a.size*a.size)}
        if a.motion:
            result["motion"]=summary("motion")
            result["motion"].update(selected=sum(r["motion_selected_id"]!=0 for r in refs),clean_selected=sum(r["motion_clean_id"]!=0 for r in refs))
        with a.output.open("x",encoding="utf-8") as f:json.dump(result,f,sort_keys=True,allow_nan=False)
        print(json.dumps({k:v for k,v in result.items() if k!="cases"},sort_keys=True))
if __name__=="__main__":main()
