#!/usr/bin/env python3
"""Generate owned JPEG2000 samples with a pinned test-only OpenJPEG encoder."""
import argparse,hashlib,itertools,json,math,os,subprocess
from pathlib import Path
REVISION='6c4a29b00211eb0430fa0e5e890f1ce5c80f409f'
def main():
 p=argparse.ArgumentParser();p.add_argument('--install',required=True);p.add_argument('--source',required=True);p.add_argument('--work',default='.build/jpeg2000-generated');a=p.parse_args()
 source=Path(a.source).resolve();revision=subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()
 if revision!=REVISION:raise SystemExit('wrong native OpenJPEG revision')
 install=Path(a.install).resolve();encoder=install/'bin/opj_compress';library=install/'lib/libopenjp2.so';env=dict(os.environ,LD_LIBRARY_PATH=str(install/'lib'))
 work=Path(a.work).resolve();work.mkdir(parents=True,exist_ok=True);fixtures=[];encoder_failures=[]
 def add(name,w,h,components=3,depth=8,signed=False,sampling=(1,1),mct=0,extra=()):
  raw=bytearray();scales=[(1,1)]+[sampling]*(components-1)
  for c,(dx,dy) in enumerate(scales):
   for y in range((h+dy-1)//dy):
    for x in range((w+dx-1)//dx):
     value=(x*73+y*151+x*y*17+c*113)%(1<<depth)
     if signed:value-=1<<(depth-1)
     raw.extend(value.to_bytes(1 if depth<=8 else 2,'big',signed=signed))
  inp=work/(name+'.raw');out=work/(name+'.j2k');inp.write_bytes(raw)
  spec=f'{w},{h},{components},{depth},{"s" if signed else "u"}@'+':'.join(f'{dx}x{dy}' for dx,dy in scales)
  n=min(4,int(math.log2(min(w//max(x for x,y in scales),h//max(y for x,y in scales))))+1)
  command=[str(encoder),'-i',str(inp),'-o',str(out),'-F',spec,'-n',str(n),'-mct',str(mct),*extra]
  run=subprocess.run(command,capture_output=True,env=env)
  if run.returncode:
   encoder_failures.append(dict(name=name,command=command,input_sha256=hashlib.sha256(raw).hexdigest(),returncode=run.returncode,stdout=run.stdout.decode(),stderr=run.stderr.decode()));return
  data=out.read_bytes();fixtures.append(dict(name=name,width=w,height=h,components=components,depth=depth,signed=signed,sampling=scales,command=command,input_sha256=hashlib.sha256(raw).hexdigest(),sha256=hashlib.sha256(data).hexdigest(),hex=data.hex()))
 for w,h in [(1,1),(7,5),(16,16),(17,19),(65,31)]:
  for components,depth,signed,lossy in itertools.product([1,3],[8,10,12,16],[False,True],[False,True]):
   add(f'{w}x{h}-{components}-{depth}-{int(signed)}-{int(lossy)}',w,h,components,depth,signed,extra=['-I','-r','4'] if lossy else [])
 for sampling,depth,lossy in itertools.product([(2,1),(2,2)],[8,10,12,16],[False,True]):
  add(f'sampling-{sampling[0]}x{sampling[1]}-{depth}-{lossy}',32,32,depth=depth,sampling=sampling,extra=['-I'] if lossy else [])
 for order,tile,mct,mode in itertools.product(['LRCP','RLCP','RPCL','PCRL','CPRL'],[False,True],[0,1],[0,1,2,4,8,16,32,63]):
  add(f'coding-{order}-{tile}-{mct}-{mode}',32,32,mct=mct,extra=['-p',order,'-M',str(mode),'-b','8,8','-r','8,4,1']+(['-t','16,16'] if tile else []))
 for w,h in [(1,9),(9,1),(3,3),(5,7),(31,33)]:
  for depth,lossy in itertools.product([1,2,4,7,8,12,16],[False,True]):
   add(f'edge-{w}x{h}-{depth}-{lossy}',w,h,depth=depth,extra=['-I'] if lossy else [])
 for depth,tile,mct in itertools.product([8,10,12,16],[False,True],[0,1]):
  add(f'lossy-mct-{depth}-{tile}-{mct}',32,32,depth=depth,mct=mct,extra=['-I','-r','3']+(['-t','16,16'] if tile else []))
 for sampling in [(2,1),(2,2),(1,2),(4,1)]:
  add(f'odd-sampling-{sampling}',17,19,sampling=sampling)
 result=dict(generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),openjpeg_revision=revision,encoder_sha256=hashlib.sha256(encoder.read_bytes()).hexdigest(),library_sha256=hashlib.sha256(library.read_bytes()).hexdigest(),fixtures=fixtures,encoder_failures=encoder_failures)
 target=Path('tests/fixtures/jpeg2000-generated.json');temporary=target.with_suffix('.tmp');temporary.write_text(json.dumps(result,indent=2)+'\n');temporary.replace(target)
 print(json.dumps({'fixtures':len(fixtures),'native_revision':revision}))
if __name__=='__main__':main()
