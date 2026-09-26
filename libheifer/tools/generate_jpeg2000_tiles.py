#!/usr/bin/env python3
"""Generate owned odd-sized JPEG2000 tiles using the pinned native encoder."""
import argparse,hashlib,itertools,json,os,subprocess
from pathlib import Path
from generate_jpeg2000_fixtures import REVISION

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--install',required=True);p.add_argument('--source',required=True);p.add_argument('--work',default='.build/jpeg2000-tiles-generated');a=p.parse_args()
 source=Path(a.source).resolve();revision=subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()
 if revision!=REVISION:raise SystemExit('wrong native OpenJPEG revision')
 install=Path(a.install).resolve();encoder=install/'bin/opj_compress';library=install/'lib/libopenjp2.so';env=dict(os.environ,LD_LIBRARY_PATH=str(install/'lib'))
 work=Path(a.work).resolve();work.mkdir(parents=True,exist_ok=True);fixtures=[]
 for (w,h,tw,th),lossy,mct in itertools.product([(17,19,9,11),(5,7,3,4)],[0,1],[0,1]):
  name=f'{w}x{h}-{lossy}-{mct}';raw=bytes((x*73+y*151+x*y*17+c*113)%256 for c in range(3) for y in range(h) for x in range(w))
  inp=work/(name+'.raw');out=work/(name+'.j2k');inp.write_bytes(raw)
  command=[str(encoder),'-i',str(inp),'-o',str(out),'-F',f'{w},{h},3,8,u@1x1:1x1:1x1','-n','2','-t',f'{tw},{th}','-mct',str(mct)]+(['-I','-r','3'] if lossy else [])
  subprocess.run(command,check=True,capture_output=True,env=env);data=out.read_bytes()
  fixtures.append(dict(name=name,width=w,height=h,command=command,input_sha256=hashlib.sha256(raw).hexdigest(),sha256=hashlib.sha256(data).hexdigest(),hex=data.hex()))
 result=dict(generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),openjpeg_revision=revision,encoder_sha256=hashlib.sha256(encoder.read_bytes()).hexdigest(),library_sha256=hashlib.sha256(library.read_bytes()).hexdigest(),fixtures=fixtures)
 target=Path('tests/fixtures/jpeg2000-tiles-generated.json');target.write_text(json.dumps(result,indent=2)+'\n');print(len(fixtures))
if __name__=='__main__':main()
