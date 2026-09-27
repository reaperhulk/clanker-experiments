#!/usr/bin/env python3
"""Independent decoded samples from mixed per-tile JPEG2000 transforms."""
import hashlib,itertools,json,sys
from pathlib import Path
import test_decode
from item_fixtures import item_file,ispe
from test_context import box

def entries(path):
 result={}
 for entry in json.loads(Path(path).read_text())['fixtures']:
  data=bytes.fromhex(entry['hex']);assert hashlib.sha256(data).hexdigest()==entry['sha256'];result[entry['name']]=data
 return result

def split(data):
 at=2;markers=[]
 while data[at:at+2]!=b'\xff\x90':
  end=at+2+int.from_bytes(data[at+2:at+4],'big');markers.append((data[at:at+2],data[at:end]));at=end
 main=data[:at];tiles=[]
 while data[at:at+2]==b'\xff\x90':
  end=at+int.from_bytes(data[at+6:at+10],'big');assert end>at;tiles.append(data[at:end]);at=end
 assert data[at:]==b'\xff\xd9'
 return main,markers,tiles

def mix(streams,choices,kinds):
 parsed=[split(d) for d in streams];out=bytearray(parsed[0][0])
 for index,choice in enumerate(choices):
  _,markers,tiles=parsed[choice];tile=bytearray(tiles[index]);extra=b''.join(data for kind,data in markers if kind in kinds)
  tile[12:12]=extra;tile[6:10]=len(tile).to_bytes(4,'big');out+=tile
 return bytes(out)+b'\xff\xd9'

def corpus():
 base=entries('tests/fixtures/jpeg2000-generated.json');odd=entries('tests/fixtures/jpeg2000-tiles-generated.json');cases=[]
 for depth in [0,8,10,12,16]:
  streams=[base[f'coding-LRCP-True-{m}-0'] if depth==0 else base[f'lossy-mct-{depth}-True-{m}'] for m in [0,1]]
  for mask in range(16):cases.append((f'mct-{depth}-{mask}',32,32,mix(streams,[(mask>>i)&1 for i in range(4)],{b'\xff\x52'})))
 for w,h in [(32,32),(17,19),(5,7)]:
  streams=[base[f'lossy-mct-8-True-{m}'] if loss else base[f'coding-LRCP-True-{m}-0'] for loss,m in itertools.product([0,1],[0,1])] if w==32 else [odd[f'{w}x{h}-{loss}-{m}'] for loss,m in itertools.product([0,1],[0,1])]
  for mask in range(256):cases.append((f'transform-{w}-{mask}',w,h,mix(streams,[(mask>>(i*2))&3 for i in range(4)],{b'\xff\x52',b'\xff\x5c',b'\xff\x5d'})))
 original=base['17x19-3-8-0-0'];end=4+int.from_bytes(original[4:6],'big')
 for count in [1,2,3,4,5,6,16,257]:
  data=original[:4]+(38+3*count).to_bytes(2,'big')+original[6:40]+count.to_bytes(2,'big')+bytes([7,1,1])*count+original[end:]
  cases.append((f'components-{count}',17,19,data))
 return cases

def main():
 if '--work' not in sys.argv:sys.argv+=['--work','.build/jpeg2000-tiles']
 if '--output' not in sys.argv:sys.argv+=['--output','.build/jpeg2000-tiles-report.json']
 if '--modes' not in sys.argv:sys.argv+=['--modes','0,3,11,27']
 directory=Path(sys.argv[sys.argv.index('--work')+1]).resolve()/'fixtures';directory.mkdir(parents=True,exist_ok=True);test_decode.FIXTURES=[]
 for name,w,h,data in corpus():
  path=directory/(name+'.heif');path.write_bytes(item_file([dict(id=1,kind=b'j2k1',data=data,props=[ispe(w,h),box(b'j2kH',b'')])]))
  test_decode.FIXTURES.append(str(path))
 test_decode.__doc__=__doc__;test_decode.main()
if __name__=='__main__':main()
