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
    for w,h,sample,quality,progressive,extra,label in cases:
        pixels=bytes((x*31+y*41+c*73)&255 for y in range(h) for x in range(w) for c in range(3))
        ppm=f'P6\n{w} {h}\n255\n'.encode()+pixels
        command=['-quality',str(quality),*(['-grayscale'] if sample=='gray' else ['-rgb'] if sample=='rgb' else ['-sample',sample]),*(['-progressive'] if progressive else []),*extra]
        data=subprocess.run([encoder,*command],input=ppm,capture_output=True,check=True).stdout
        fixtures.append(dict(name=label or f'{w}x{h}-{sample}-{quality}-{int(progressive)}.jpg',width=w,height=h,sampling=sample,quality=quality,progressive=progressive,command=command,input_sha256=hashlib.sha256(ppm).hexdigest(),sha256=hashlib.sha256(data).hexdigest(),hex=data.hex()))
    report=dict(scope='Deterministic owned sample patterns encoded independently by pinned native libjpeg-turbo; candidate is never used to produce expected pixels.',revision=JPEG,generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),encoder_sha256=hashlib.sha256(encoder.read_bytes()).hexdigest(),fixtures=fixtures)
    output=Path('tests/fixtures/jpeg-generated.json')
    temporary=output.with_suffix('.json.tmp')
    temporary.write_text(json.dumps(report,indent=2)+'\n')
    temporary.replace(output)

if __name__=='__main__':main()
