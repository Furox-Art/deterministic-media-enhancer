"""Precision, local fusion, CUDA parity (optional), deconvolution regression."""
import json, math, pathlib, struct, subprocess, sys, tempfile
agent, engine=map(lambda p:pathlib.Path(p).resolve(),sys.argv[1:3])
gpu="--gpu" in sys.argv[3:]
def call(*args):
    p=subprocess.run([str(x) for x in args],capture_output=True,text=True,timeout=180)
    if p.returncode:raise RuntimeError((args,p.stdout,p.stderr))
    return p
def invoke(source,out,preset,scale=1,ok=True):
    p=subprocess.run([sys.executable,str(agent),"--engine",str(engine),"enhance","--input",str(source),
                      "--output",str(out),"--preset",preset,"--scale",str(scale)],capture_output=True,text=True,timeout=180)
    result=json.loads(p.stdout)
    assert (p.returncode==0)==ok and (result["status"]=="ok")==ok,(result,p.stderr)
    return result
def ff(*args):return call("ffmpeg","-v","error","-n",*args)
with tempfile.TemporaryDirectory(prefix="dme-advanced-") as tmp:
    r=pathlib.Path(tmp)
    raw=r/"rgba.raw"
    raw.write_bytes(b"".join(struct.pack(">HHHH",(i*131)%65536,(i*733)%65536,(i*1711)%65536,(i*211)%65536) for i in range(64*48)))
    png=r/"rgba.png"
    ff("-f","rawvideo","-pixel_format","rgba64be","-video_size","64x48","-i",raw,"-frames:v","1","-threads","1",png)
    a=invoke(png,r/"rgba-exact.png","precision")
    assert next(x for x in a["reports"] if "precision" in x)["sample_exact"] is True
    invoke(png,r/"rgba-up.png","precision",2)
    for transfer in ("smpte2084","arib-std-b67"):
        for depth in (10,16):
            source=r/f"{transfer}-{depth}.mkv"
            ff("-f","lavfi","-i",f"testsrc2=s=64x48:r=24,format=yuv444p{depth}le","-t","0.125",
               "-c:v","ffv1","-level","3","-color_range","tv","-colorspace","bt2020nc","-color_trc",transfer,
               "-color_primaries","bt2020","-threads","1",source)
            for scale in (1,2):
                out=r/f"{transfer}-{depth}-{scale}.mkv"
                report=next(x for x in invoke(source,out,"precision",scale)["reports"] if "precision" in x)
                assert report["pixel_format"]==f"yuv444p{depth}le" and report["sample_exact"]==(scale==1)
            invoke(source,r/"invalid.mp4","precision",ok=False)
            assert not (r/"invalid.mp4").exists()
    for i,(dx,dy) in enumerate([(0,0),(1,0),(0,1),(1,1),(0,0),(1,0)]):
        raw=bytearray()
        for y in range(80):
            for x in range(96):
                xx,yy=2*x+dx,2*y+dy
                v=round(128+43*math.sin(xx*.24)+30*math.cos(yy*.19)+17*math.sin((xx+yy)*.45))
                raw.extend((v,v,v))
        (r/f"frame{i:02d}.ppm").write_bytes(b"P6\n96 80\n255\n"+raw)
    source=r/"video.mkv"
    ff("-framerate","24","-i",r/"frame%02d.ppm","-f","lavfi","-i","sine=frequency=440","-t","0.25",
       "-threads","1","-c:v","ffv1","-pix_fmt","bgr0","-c:a","flac",source)
    a=invoke(source,r/"local-a.mkv","multiframe-local",2)
    b=invoke(source,r/"local-b.mkv","multiframe-local",2)
    assert a["sha256"]==b["sha256"]
    phase=next(x for x in a["reports"] if "local_fusion" in x)["phases"]
    assert phase[0]==0 and all(x>0 for x in phase[1:]),phase
    if gpu:
        for i in range(2):
            g=invoke(source,r/f"gpu-{i}.mkv","multiframe-gpu",2)
            assert g["sha256"]==a["sha256"]
        print("CUDA and CPU video SHA256 equal:",a["sha256"],"phases",phase)
    # Both deconvolution modes must emit validated reports and deterministic outputs.
    for preset in ("blind","motion-deblur"):
        a=invoke(source,r/f"{preset}-a.mkv",preset)
        b=invoke(source,r/f"{preset}-b.mkv",preset)
        assert a["sha256"]==b["sha256"]
        assert any(x.get("deconvolution")==preset for x in a["reports"])
    print("Precision RGBA16, PQ/HLG 10/16-bit, local phase coverage and deconvolution passed; CUDA tested:",gpu)
