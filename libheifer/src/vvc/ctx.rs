// SPDX-License-Identifier: LGPL-3.0-or-later
#![allow(dead_code)]
//! CABAC context initialization values (H.266 tables 51-126), in the order of
//! vvdec's `Contexts.cpp`: rows are init types B, P, I and window sizes.
// Generated from vvdec 3.2.0 source (BSD-3-Clause-Clear); see licenses/.

pub const SPLIT_FLAG: usize = 0;
pub const SPLIT_QT_FLAG: usize = 9;
pub const SPLIT_HV_FLAG: usize = 15;
pub const SPLIT12_FLAG: usize = 20;
pub const MODE_CONS_FLAG: usize = 24;
pub const SKIP_FLAG: usize = 26;
pub const MERGE_FLAG: usize = 29;
pub const REGULAR_MERGE_FLAG: usize = 30;
pub const MERGE_IDX: usize = 32;
pub const MMVD_FLAG: usize = 33;
pub const MMVD_MERGE_IDX: usize = 34;
pub const MMVD_STEP_MVP_IDX: usize = 35;
pub const PRED_MODE: usize = 36;
pub const MULTI_REF_LINE_IDX: usize = 38;
pub const I_PRED_MODE0: usize = 40;
pub const I_PRED_MODE1: usize = 41;
pub const INTRA_LUMA_PLANAR_FLAG: usize = 42;
pub const CCLM_MODE_FLAG: usize = 44;
pub const CCLM_MODE_IDX: usize = 45;
pub const MIP_FLAG: usize = 46;
pub const DELTAQP: usize = 50;
pub const INTER_DIR: usize = 52;
pub const REF_PIC: usize = 58;
pub const SUBBLOCK_MERGE_FLAG: usize = 60;
pub const AFFINE_FLAG: usize = 63;
pub const AFFINE_TYPE: usize = 66;
pub const AFF_MERGE_IDX: usize = 67;
pub const BCW_IDX: usize = 68;
pub const MVD: usize = 69;
pub const BDPCM_MODE: usize = 71;
pub const QT_ROOT_CBF: usize = 75;
pub const ACT_FLAG: usize = 76;
pub const QT_CBF0: usize = 77;
pub const QT_CBF1: usize = 81;
pub const QT_CBF2: usize = 83;
pub const SIG_COEFF_GROUP0: usize = 86;
pub const SIG_COEFF_GROUP1: usize = 88;
pub const SIG_FLAG0: usize = 90;
pub const SIG_FLAG1: usize = 102;
pub const SIG_FLAG2: usize = 110;
pub const SIG_FLAG3: usize = 122;
pub const SIG_FLAG4: usize = 130;
pub const SIG_FLAG5: usize = 142;
pub const PAR_FLAG0: usize = 150;
pub const PAR_FLAG1: usize = 171;
pub const GTX_FLAG0: usize = 182;
pub const GTX_FLAG1: usize = 203;
pub const GTX_FLAG2: usize = 214;
pub const GTX_FLAG3: usize = 235;
pub const LASTX0: usize = 246;
pub const LASTX1: usize = 266;
pub const LASTY0: usize = 269;
pub const LASTY1: usize = 289;
pub const MVP_IDX: usize = 292;
pub const SMVD_FLAG: usize = 293;
pub const SAO_MERGE_FLAG: usize = 294;
pub const SAO_TYPE_IDX: usize = 295;
pub const LFNST_IDX: usize = 296;
pub const RDPCM_FLAG: usize = 299;
pub const RDPCM_DIR: usize = 301;
pub const MTS_INDEX: usize = 303;
pub const ISP_MODE: usize = 309;
pub const SBT_FLAG: usize = 311;
pub const SBT_QUAD_FLAG: usize = 313;
pub const SBT_HOR_FLAG: usize = 314;
pub const SBT_POS_FLAG: usize = 317;
pub const CHROMA_QP_ADJ_FLAG: usize = 318;
pub const CHROMA_QP_ADJ_IDC: usize = 319;
pub const IMV_FLAG: usize = 320;
pub const CTB_ALF_FLAG: usize = 325;
pub const CTB_ALF_ALTERNATIVE: usize = 334;
pub const ALF_USE_TEMPORAL_FILT: usize = 336;
pub const CC_ALF_FILTER_CONTROL_FLAG: usize = 337;
pub const CIIP_FLAG: usize = 343;
pub const IBC_FLAG: usize = 344;
pub const JOINT_CB_CR_FLAG: usize = 347;
pub const TS_SIG_COEFF_GROUP: usize = 350;
pub const TS_SIG_FLAG: usize = 353;
pub const TS_PAR_FLAG: usize = 356;
pub const TS_GTX_FLAG: usize = 357;
pub const TS_LRG1_FLAG: usize = 362;
pub const TS_RESIDUAL_SIGN: usize = 366;
pub const NUM_CTX: usize = 372;

pub static INIT: [[u8; NUM_CTX]; 4] = [
    [
        18, 27, 15, 18, 28, 45, 26, 7, 23, 26, 36, 38, 18, 34, 21, 43, 42, 37, 42, 44, 28, 29, 28,
        29, 25, 20, 57, 60, 46, 6, 46, 15, 18, 25, 43, 59, 40, 35, 25, 59, 44, 25, 13, 6, 26, 27,
        56, 57, 50, 26, 35, 35, 14, 13, 5, 4, 3, 40, 5, 35, 25, 58, 45, 19, 13, 6, 35, 4, 5, 51,
        36, 19, 21, 0, 28, 12, 46, 15, 6, 5, 14, 25, 37, 9, 36, 45, 25, 45, 25, 14, 17, 41, 49, 36,
        1, 49, 50, 37, 48, 51, 58, 45, 9, 49, 50, 36, 48, 59, 59, 38, 26, 45, 53, 46, 49, 54, 61,
        39, 35, 39, 39, 39, 34, 45, 38, 31, 58, 39, 39, 39, 19, 54, 39, 39, 50, 39, 39, 39, 0, 39,
        39, 39, 34, 38, 54, 39, 41, 39, 39, 39, 33, 40, 25, 41, 26, 42, 25, 33, 26, 34, 27, 25, 41,
        42, 42, 35, 33, 27, 35, 42, 43, 33, 25, 26, 34, 19, 27, 33, 42, 43, 35, 43, 25, 0, 0, 17,
        25, 26, 0, 9, 25, 33, 19, 0, 25, 33, 26, 20, 25, 33, 27, 35, 22, 25, 1, 25, 33, 26, 12, 25,
        33, 27, 28, 37, 0, 0, 33, 34, 35, 21, 25, 34, 35, 28, 29, 40, 42, 43, 29, 30, 49, 36, 37,
        45, 38, 0, 40, 34, 43, 36, 37, 57, 52, 45, 38, 46, 6, 6, 12, 14, 6, 4, 14, 7, 6, 4, 29, 7,
        6, 6, 12, 28, 7, 13, 13, 35, 19, 5, 4, 5, 5, 20, 13, 13, 19, 21, 6, 12, 12, 14, 14, 5, 4,
        12, 13, 7, 13, 12, 41, 11, 5, 27, 34, 28, 2, 2, 52, 37, 27, 35, 35, 35, 35, 45, 25, 27, 0,
        25, 17, 33, 43, 41, 57, 42, 35, 51, 27, 28, 35, 35, 59, 26, 50, 60, 38, 33, 52, 46, 25, 61,
        54, 25, 61, 54, 11, 26, 46, 25, 35, 38, 25, 28, 38, 57, 0, 43, 45, 42, 43, 52, 18, 35, 45,
        25, 50, 37, 11, 35, 3, 4, 4, 5, 19, 11, 4, 6, 35, 25, 46, 28, 33, 38,
    ],
    [
        11, 35, 53, 12, 6, 30, 13, 15, 31, 20, 14, 23, 18, 19, 6, 43, 35, 37, 34, 52, 43, 37, 21,
        22, 25, 12, 57, 59, 45, 21, 38, 7, 20, 26, 43, 60, 40, 35, 25, 58, 36, 25, 12, 20, 34, 27,
        41, 57, 58, 26, 35, 35, 7, 6, 5, 12, 4, 40, 20, 35, 48, 57, 44, 12, 13, 14, 35, 5, 4, 44,
        43, 40, 36, 0, 13, 5, 46, 23, 5, 20, 7, 25, 28, 25, 29, 45, 25, 30, 25, 45, 17, 41, 42, 29,
        25, 49, 43, 37, 33, 58, 51, 30, 17, 34, 35, 21, 41, 59, 60, 38, 19, 38, 38, 46, 34, 54, 54,
        39, 6, 39, 39, 39, 35, 45, 53, 54, 44, 39, 39, 39, 19, 39, 54, 39, 19, 39, 39, 39, 56, 39,
        39, 39, 34, 38, 62, 39, 26, 39, 39, 39, 18, 17, 33, 18, 26, 42, 25, 33, 26, 42, 27, 25, 34,
        42, 42, 35, 26, 27, 42, 20, 20, 25, 25, 26, 11, 19, 27, 33, 42, 35, 35, 43, 17, 0, 1, 17,
        25, 18, 0, 9, 25, 33, 34, 9, 25, 18, 26, 20, 25, 18, 19, 27, 29, 17, 9, 25, 10, 18, 4, 17,
        33, 19, 20, 29, 0, 17, 26, 19, 35, 21, 25, 34, 20, 28, 29, 33, 27, 28, 29, 22, 34, 28, 44,
        37, 38, 0, 25, 19, 20, 13, 14, 57, 44, 30, 30, 23, 6, 13, 12, 6, 6, 12, 14, 14, 13, 12, 29,
        7, 6, 13, 36, 28, 14, 13, 5, 26, 12, 4, 18, 5, 5, 12, 6, 6, 4, 6, 14, 5, 12, 14, 7, 13, 5,
        13, 21, 14, 20, 12, 34, 11, 4, 18, 34, 28, 60, 5, 37, 45, 27, 35, 35, 35, 35, 45, 40, 27,
        0, 25, 9, 33, 36, 56, 57, 42, 20, 43, 12, 28, 35, 35, 59, 48, 58, 60, 60, 13, 23, 46, 4,
        61, 54, 19, 46, 54, 20, 12, 46, 18, 21, 38, 18, 21, 38, 57, 0, 57, 44, 27, 36, 45, 18, 12,
        29, 40, 35, 44, 3, 35, 2, 10, 3, 3, 18, 11, 4, 28, 5, 10, 53, 43, 25, 46,
    ],
    [
        19, 28, 38, 27, 29, 38, 20, 30, 31, 27, 6, 15, 25, 19, 37, 43, 42, 29, 27, 44, 36, 45, 36,
        45, 35, 35, 0, 26, 28, 26, 35, 35, 34, 35, 35, 35, 35, 35, 25, 60, 45, 34, 13, 28, 59, 27,
        33, 49, 50, 25, 35, 35, 35, 35, 35, 35, 35, 35, 35, 35, 35, 35, 35, 35, 35, 35, 35, 35, 35,
        14, 45, 19, 35, 1, 27, 6, 52, 15, 12, 5, 7, 12, 21, 33, 28, 36, 18, 31, 25, 15, 25, 19, 28,
        14, 25, 20, 29, 30, 19, 37, 30, 38, 25, 27, 28, 37, 34, 53, 53, 46, 11, 38, 46, 54, 27, 39,
        39, 39, 44, 39, 39, 39, 19, 46, 38, 39, 52, 39, 39, 39, 18, 39, 39, 39, 27, 39, 39, 39, 0,
        39, 39, 39, 11, 39, 39, 39, 19, 39, 39, 39, 33, 25, 18, 26, 34, 27, 25, 26, 19, 42, 35, 33,
        19, 27, 35, 35, 34, 42, 20, 43, 20, 33, 25, 26, 42, 19, 27, 26, 50, 35, 20, 43, 25, 1, 40,
        25, 33, 11, 17, 25, 25, 18, 4, 17, 33, 26, 19, 13, 33, 19, 20, 28, 22, 40, 9, 25, 18, 26,
        35, 25, 26, 35, 28, 37, 25, 25, 11, 27, 20, 21, 33, 12, 28, 21, 22, 34, 28, 29, 29, 30, 36,
        29, 45, 30, 23, 40, 33, 27, 28, 21, 37, 36, 37, 45, 38, 46, 13, 5, 4, 21, 14, 4, 6, 14, 21,
        11, 14, 7, 14, 5, 11, 21, 30, 22, 13, 42, 12, 4, 3, 13, 5, 4, 6, 13, 11, 14, 6, 5, 3, 14,
        22, 6, 4, 3, 6, 22, 29, 20, 34, 12, 4, 3, 42, 35, 60, 13, 28, 52, 42, 35, 35, 35, 35, 29,
        0, 28, 0, 25, 9, 33, 43, 35, 35, 35, 35, 35, 35, 35, 35, 35, 35, 34, 35, 35, 35, 62, 39,
        39, 54, 39, 39, 31, 39, 39, 11, 11, 46, 18, 30, 31, 18, 30, 31, 35, 17, 42, 36, 12, 21, 35,
        18, 20, 38, 25, 28, 38, 11, 35, 10, 3, 3, 3, 11, 5, 5, 14, 12, 17, 46, 28, 25, 46,
    ],
    [
        12, 13, 8, 8, 13, 12, 5, 9, 9, 0, 8, 8, 12, 12, 8, 9, 8, 9, 8, 5, 12, 13, 12, 13, 1, 0, 5,
        4, 8, 4, 5, 5, 4, 4, 10, 0, 5, 1, 5, 8, 6, 5, 1, 5, 4, 9, 9, 10, 9, 6, 8, 8, 0, 0, 1, 4, 4,
        0, 0, 4, 4, 4, 4, 4, 0, 0, 4, 0, 1, 9, 5, 1, 4, 1, 0, 4, 1, 5, 1, 8, 9, 5, 0, 2, 1, 0, 8,
        5, 5, 8, 12, 9, 9, 10, 9, 9, 9, 10, 8, 8, 8, 10, 12, 12, 9, 13, 4, 5, 8, 9, 9, 13, 8, 8, 8,
        8, 8, 5, 8, 0, 0, 0, 8, 12, 12, 8, 4, 0, 0, 0, 8, 8, 8, 8, 8, 0, 4, 4, 0, 0, 0, 0, 8, 8, 8,
        8, 4, 0, 0, 0, 8, 9, 12, 13, 13, 13, 10, 13, 13, 13, 13, 13, 13, 13, 13, 13, 10, 13, 13,
        13, 13, 8, 12, 12, 12, 13, 13, 13, 13, 13, 13, 13, 1, 5, 9, 9, 9, 6, 5, 9, 10, 10, 9, 9, 9,
        9, 9, 9, 6, 8, 9, 9, 10, 1, 5, 8, 8, 9, 6, 6, 9, 8, 8, 9, 9, 5, 10, 13, 13, 10, 9, 10, 13,
        13, 13, 9, 10, 10, 10, 13, 8, 9, 10, 10, 13, 8, 8, 9, 12, 12, 10, 5, 9, 9, 9, 13, 8, 5, 4,
        5, 4, 4, 5, 4, 1, 0, 4, 1, 0, 0, 0, 0, 1, 0, 0, 0, 5, 4, 4, 8, 5, 8, 5, 5, 4, 5, 5, 4, 0,
        5, 4, 1, 0, 0, 1, 4, 0, 0, 0, 6, 5, 5, 12, 5, 0, 4, 9, 9, 10, 8, 8, 8, 8, 8, 0, 9, 0, 1, 1,
        9, 2, 1, 5, 10, 8, 4, 1, 13, 8, 8, 0, 5, 0, 0, 4, 0, 0, 0, 4, 0, 0, 1, 0, 0, 0, 0, 0, 4, 1,
        4, 4, 1, 4, 1, 1, 5, 8, 1, 1, 0, 5, 8, 8, 13, 13, 8, 6, 8, 1, 1, 1, 1, 4, 2, 1, 6, 1, 4, 4,
        5, 8, 8,
    ],
];
