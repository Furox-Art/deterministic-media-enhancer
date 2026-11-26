#!/usr/bin/env python3
"""Install and test the exact release archives in separate offline environments."""
import argparse
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

def run(args,env,cwd):
    return subprocess.run(list(map(str,args)),env=env,cwd=cwd,check=True)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("dist",type=Path)
    p.add_argument("--npm-only",action="store_true")
    a=p.parse_args()
    dist=a.dist.resolve(strict=True)
    for mode in (("npm",) if a.npm_only else ("pip","npm")):
        with tempfile.TemporaryDirectory(prefix="dme-package-test-") as tmp:
            t=Path(tmp)
            env=dict(os.environ,XDG_CACHE_HOME=str(t/"cache"),CARGO_NET_OFFLINE="true")
            if mode=="npm":
                archive,=dist.glob("*.tgz")
                run(["npm","install","--prefix",t/"npm","--offline","--ignore-scripts","--no-audit","--no-fund",archive],env,t)
                cli=t/"npm/node_modules/.bin/dme"
            else:
                archive,=dist.glob("*.whl")
                run([sys.executable,"-m","venv",t/"venv"],env,t)
                env["PATH"]=str(t/"venv/bin")+os.pathsep+env["PATH"]
                run([t/"venv/bin/python","-m","pip","install","--no-index","--no-deps",archive],env,t)
                cli=t/"venv/bin/dme"
            run([cli,"--version"],env,t)
            run([cli,"skill-path"],env,t)
            run([cli,"build"],env,t)
            run([cli,"doctor"],env,t)
            raw=bytes((i*17)%256 for i in range(64*64*3))
            source=t/"source.bmp"
            source.write_bytes(struct.pack("<2sIHHI",b"BM",54+len(raw),0,0,54)+struct.pack("<IiiHHIIiiII",40,64,64,1,24,0,len(raw),0,0,0,0)+raw)
            run([cli,"enhance","--input",source,"--output",t/"restored.png","--preset","bm3d-soft"],env,t)
            run([cli,"verify-repeat","--input",source,"--preset","bm3d-soft"],env,t)
            assert (t/"restored.png").is_file()
            print(json.dumps({"package":mode,"status":"ok"}),flush=True)
if __name__=="__main__":main()
