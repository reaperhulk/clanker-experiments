// Progressive smoothing adapted to Rust from libjpeg-turbo 3.1.1 jdcoefct.c.
// This file was part of the Independent JPEG Group's software:
// Copyright (C) 1994-1997, Thomas G. Lane.
// libjpeg-turbo Modifications:
// Copyright 2009 Pierre Ossman <ossman@cendio.se> for Cendio AB
// Copyright (C) 2010, 2015-2016, 2019-2020, 2022-2024, D. R. Commander.
// Copyright (C) 2015, 2020, Google, Inc.
// Rust adaptation Copyright (C) 2026 libheifer contributors.
// For conditions of distribution and use, see LICENSE-IJG.
use alloc::vec::Vec;
use crate::parser::Component;
const POS: [usize;10]=[0,1,8,16,9,2,3,10,17,24];

pub(crate) fn eligible(bits: &[i8;64], q: &[u16;64]) -> bool {
    bits[0]>=0 && POS.iter().all(|&p|q[p]!=0)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn smooth(input:&[i16], component:&Component, q:&[u16;64], bits:&[i8;64], previous:&[i8;64], last_good:usize, mcu_rows:usize) -> Vec<i16> {
    let mut output=input.to_vec();
    let width=usize::from(component.size.width).div_ceil(8);
    let height=usize::from(component.size.height).div_ceil(8);
    let padded_height=usize::from(component.block_size.height);
    let stride=usize::from(component.block_size.width);
    let vertical=usize::from(component.vertical_sampling_factor);
    for y in 0..height {
        let mcu=y/vertical;
        let rows=if mcu+1==mcu_rows && height%vertical!=0 {height%vertical} else {vertical};
        let pseudo_y=mcu*rows+y%vertical;
        let pseudo_height=rows*mcu_rows;
        let up=if pseudo_y>0 {y.saturating_sub(1)} else {y};
        let up2=if pseudo_y>1 {y.saturating_sub(2)} else {up};
        let down=if pseudo_y+1<pseudo_height {(y+1).min(padded_height-1)} else {y};
        let down2=if pseudo_y+2<pseudo_height {(y+2).min(padded_height-1)} else {down};
        let bits=if mcu>last_good {previous} else {bits};
        let change_dc=bits[1..10].iter().all(|&b|b==-1);
        for x in 0..width {
            let mut d=[0i64;26];
            for (ry,sy) in [up2,up,y,down,down2].into_iter().enumerate() {
                for rx in 0..5 {
                    let sx=(x+rx).saturating_sub(2).min(width-1);
                    d[ry*5+rx+1]=i64::from(input[(sy*stride+sx)*64]);
                }
            }
            let nums=if change_dc {[
                -2*d[1]-6*d[2]-8*d[3]-6*d[4]-2*d[5]-6*d[6]+6*d[7]+42*d[8]+6*d[9]-6*d[10]-8*d[11]+42*d[12]+152*d[13]+42*d[14]-8*d[15]-6*d[16]+6*d[17]+42*d[18]+6*d[19]-6*d[20]-2*d[21]-6*d[22]-8*d[23]-6*d[24]-2*d[25],
                -d[1]-d[2]+d[4]+d[5]-3*d[6]+13*d[7]-13*d[9]+3*d[10]-3*d[11]+38*d[12]-38*d[14]+3*d[15]-3*d[16]+13*d[17]-13*d[19]+3*d[20]-d[21]-d[22]+d[24]+d[25],
                -d[1]-3*d[2]-3*d[3]-3*d[4]-d[5]-d[6]+13*d[7]+38*d[8]+13*d[9]-d[10]+d[16]-13*d[17]-38*d[18]-13*d[19]+d[20]+d[21]+3*d[22]+3*d[23]+3*d[24]+d[25],
                d[3]+2*d[7]+7*d[8]+2*d[9]-5*d[12]-14*d[13]-5*d[14]+2*d[17]+7*d[18]+2*d[19]+d[23],
                -d[1]+d[5]+9*d[7]-9*d[9]-9*d[17]+9*d[19]+d[21]-d[25],
                2*d[7]-5*d[8]+2*d[9]+d[11]+7*d[12]-14*d[13]+7*d[14]+d[15]+2*d[17]-5*d[18]+2*d[19],
                d[7]-d[9]+2*d[12]-2*d[14]+d[17]-d[19],
                d[7]-3*d[8]+d[9]-d[17]+3*d[18]-d[19],
                d[7]-d[9]-3*d[12]+3*d[14]+d[17]-d[19],
                d[7]+2*d[8]+d[9]-d[17]-2*d[18]-d[19],
            ]} else {[
                0,-7*d[11]+50*d[12]-50*d[14]+7*d[15],
                -7*d[3]+50*d[8]-50*d[18]+7*d[23],
                -d[3]+13*d[8]-24*d[13]+13*d[18]-d[23],
                d[10]+d[16]-10*d[17]+10*d[19]-d[2]-d[20]+d[22]-d[24]+d[4]-d[6]+10*d[7]-10*d[9],
                -d[11]+13*d[12]-24*d[13]+13*d[14]-d[15],0,0,0,0,
            ]};
            for k in 0..if change_dc {10} else {6} {
                let p=POS[k];let at=(y*stride+x)*64+p;
                if k==0 && !change_dc || k!=0 && (bits[k]==0 || output[at]!=0) {continue;}
                let num=i64::from(q[0])*nums[k];let quant=i64::from(q[p]);
                let mut pred=((quant<<7)+num.abs())/(quant<<8);
                if k!=0 && bits[k]>0 {pred=pred.min((1<<bits[k])-1);}
                output[at]=(pred*num.signum()) as i16;
            }
        }
    }
    output
}
