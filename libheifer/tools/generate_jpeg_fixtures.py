#!/usr/bin/env python3
"""Generate owned JPEG sample patterns with the pinned native test-only cjpeg."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

JPEG = '7723f50f3f66b9da74376e6d8badb6162464212c'

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', default='.build/libjpeg-turbo-source')
    parser.add_argument('--encoder', default='.build/libjpeg-turbo-install/bin/cjpeg')
    args = parser.parse_args()
    if subprocess.check_output(['git','-C',args.source,'rev-parse','HEAD'],text=True).strip() != JPEG:
        raise SystemExit('Wrong native JPEG fixture encoder revision')
    encoder = Path(args.encoder).resolve()
    fixtures = []
    cases = []
    for w,h in [(1,1),(7,5),(16,16),(17,19),(65,31)]:
        for sample in ['1x1','2x1','2x2','gray']:
            for quality in [25,75,100]:
                for progressive in [False,True]:
                    cases.append((w,h,sample,quality,progressive))
    for w,h in [(1,9),(2,9),(3,9),(4,9),(5,9),(9,1),(9,2),(9,3),(9,4),(9,5),(31,33)]:
        for sample in ['1x1','2x1','2x2','1x2','4x1']:
            cases.append((w,h,sample,75,False))
    cases=[(*case,[],None) for case in cases]
    for progressive in [False,True]:
        for sample in ['1x1','2x1','2x2','gray','rgb']:
            cases.append((65,31,sample,75,progressive,['-restart','1B'],f'restart-{sample}-{int(progressive)}.jpg'))
    for predictor in range(1,8):
        for point in [0,1,3,7]:
            for sample in ['gray','rgb']:
                cases.append((17,19,sample,75,False,['-lossless',f'{predictor},{point}'],f'lossless-{sample}-{predictor}-{point}.jpg'))
    for precision in [2,3,4,5,6,7,9,12,16]:
        for predictor in [1,7]:
            for point in [0,precision-1]:
                cases.append((17,19,'gray',75,False,['-precision',str(precision),'-lossless',f'{predictor},{point}'],f'precision-{precision}-{predictor}-{point}.jpg'))
    for progressive in [False,True]:
        cases.append((17,19,'gray',75,progressive,['-precision','12'],f'dct-12-{int(progressive)}.jpg'))
    # Arithmetic entropy coding (SOF9/SOF10).
    for w,h in [(1,1),(16,16),(17,19),(65,31)]:
        for sample in ['1x1','2x1','2x2','gray','rgb']:
            for quality in [25,75,100]:
                for progressive in [False,True]:
                    cases.append((w,h,sample,quality,progressive,['-arithmetic'],f'arith-{w}x{h}-{sample}-{quality}-{int(progressive)}.jpg'))
    for progressive in [False,True]:
        for sample in ['1x1','2x2','gray']:
            for restart in ['1B','2']:
                cases.append((65,31,sample,75,progressive,['-arithmetic','-restart',restart],f'arith-restart-{restart}-{sample}-{int(progressive)}.jpg'))
    for w,h,sample,quality,progressive,extra,label in cases:
        pixels=bytes((x*31+y*41+c*73)&255 for y in range(h) for x in range(w) for c in range(3))
        ppm=f'P6\n{w} {h}\n255\n'.encode()+pixels
        command=['-quality',str(quality),*(['-grayscale'] if sample=='gray' else ['-rgb'] if sample=='rgb' else ['-sample',sample]),*(['-progressive'] if progressive else []),*extra]
        data=subprocess.run([encoder,*command],input=ppm,capture_output=True,check=True).stdout
        fixtures.append(dict(name=label or f'{w}x{h}-{sample}-{quality}-{int(progressive)}.jpg',width=w,height=h,sampling=sample,quality=quality,progressive=progressive,command=command,input_sha256=hashlib.sha256(ppm).hexdigest(),sha256=hashlib.sha256(data).hexdigest(),hex=data.hex()))
    # DAC conditioning segments spliced before every scan header of arithmetic
    # streams (after the encoder's own DAC): the defaults restated, other valid
    # conditioning (which decodes the same entropy data differently) and
    # invalid segments.
    bases=[f for f in fixtures if f['name'] in ('arith-17x19-2x2-75-0.jpg','arith-17x19-2x2-75-1.jpg')]
    dacs=[('default',bytes([0,0x10,16,5])),('dc-l1u2',bytes([0,0x21])),('dc-l0u0',bytes([0,0x00])),('dc-l5u9',bytes([0,0x95,1,0x95])),
          ('ac-k1',bytes([16,1])),('ac-k63',bytes([16,63,17,63])),('ac-k0',bytes([16,0])),('ac-k200',bytes([16,200])),
          ('both',bytes([0,0x32,16,2,1,0x10,17,9])),('bad-l-gt-u',bytes([0,0x12])),('bad-index',bytes([32,1])),('bad-length',bytes([0,0x10,16]))]
    for base in bases:
        data=bytes.fromhex(base['hex'])
        for label,body in dacs:
            out=data.replace(b'\xff\xda',b'\xff\xcc'+(len(body)+2).to_bytes(2,'big')+body+b'\xff\xda')
            fixtures.append(dict(base,name=f"dac-{label}-{int(base['progressive'])}.jpg",command=base['command']+['dac',body.hex()],sha256=hashlib.sha256(out).hexdigest(),hex=out.hex()))
    report=dict(scope='Deterministic owned sample patterns encoded independently by pinned native libjpeg-turbo; candidate is never used to produce expected pixels.',revision=JPEG,generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),encoder_sha256=hashlib.sha256(encoder.read_bytes()).hexdigest(),fixtures=fixtures)
    output=Path('tests/fixtures/jpeg-generated.json')
    temporary=output.with_suffix('.json.tmp')
    temporary.write_text(json.dumps(report,indent=2)+'\n')
    temporary.replace(output)

if __name__=='__main__':main()
