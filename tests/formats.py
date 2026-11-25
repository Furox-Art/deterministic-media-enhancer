import json
from pathlib import Path
import subprocess
import sys
import tempfile

agent, engine=map(Path,sys.argv[1:3])
def ff(*args):
    subprocess.run(["ffmpeg","-nostdin","-v","error","-n",*map(str,args)],check=True,timeout=30)
def invoke(*args,ok=True):
    r=subprocess.run([sys.executable,str(agent),"--engine",str(engine),*map(str,args)],
                     capture_output=True,text=True,timeout=30)
    assert not r.stderr,r.stderr
    value=json.loads(r.stdout)
    assert (r.returncode==0)==ok,value
    assert (value["status"]=="ok")==ok,value
    return value

with tempfile.TemporaryDirectory(prefix="dme-formats-") as tmp:
    root=Path(tmp)
    for ext in ("png","jpg","jpeg","bmp","tif","tiff","webp"):
        path=root/("image."+ext)
        ff("-f","lavfi","-i","testsrc2=size=64x48:rate=1","-frames:v","1","-threads","1",path)
        invoke("analyze","--input",path)
        invoke("enhance","--input",path,"--output",root/(ext+"-out.png"),"--preset","resize")
    for ext,codec in (("mkv","ffv1"),("mp4","libx264"),("mov","libx264"),("avi","mpeg4"),("webm","libvpx-vp9")):
        path=root/("video."+ext)
        ff("-f","lavfi","-i","testsrc2=size=64x48:rate=24","-t","0.25","-threads","1","-c:v",codec,path)
        invoke("enhance","--input",path,"--output",root/(ext+"-out.mkv"),"--preset","resize")
    # Valid local sentinel referenced by a disguised playlist. No user data is accessed.
    for ext,body in [("png","ffconcat version 1.0\nfile 'image.png'\n"),
                     ("mkv","ffconcat version 1.0\nfile 'video.mkv'\n"),
                     ("mp4","#EXTM3U\n#EXT-X-VERSION:3\n#EXTINF:1,\nvideo.mp4\n#EXT-X-ENDLIST\n")]:
        source=root/("spoof."+ext);source.write_text(body)
        output=root/("spoof-out."+("png" if ext=="png" else "mkv"))
        a=invoke("analyze","--input",source,ok=False)
        b=invoke("enhance","--input",source,"--output",output,"--preset","resize",ok=False)
        assert not output.exists()
        # A disguised HLS file may be probed as MOV by extension and fail before
        # the whitelist is consulted. Either rejection is valid; preserve the diagnostic.
        for value in (a,b):
            diagnostics="\n".join(value.get("diagnostics",[])).lower()
            assert ("whitelist" in diagnostics or
                    (ext=="mp4" and "moov atom not found" in diagnostics)),value
    wrong=root/"wrong.png";wrong.write_bytes((root/"video.mkv").read_bytes())
    invoke("enhance","--input",wrong,"--output",root/"wrong-out.png","--preset","resize",ok=False)
    assert not (root/"wrong-out.png").exists()
    tiny=root/"tiny.png"
    ff("-f","lavfi","-i","color=s=16x16","-frames:v","1","-threads","1",tiny)
    invoke("enhance","--input",tiny,"--output",root/"tiny-out.png","--preset","bm3d",ok=False)
    assert not (root/"tiny-out.png").exists()
print("Supported formats, per-extension demuxer restriction and disguised playlist rejection passed.")
