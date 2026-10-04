//! 評価関数。手番側から見た評価値（センチポーン相当）を返す。
//!
//! 駒割り + 玉との距離に基づく配置評価（守り駒は自玉の近く、攻め駒は敵玉の近くを評価）
//! + 玉の位置評価。パラメータは params.rs（自動調整の対象）。
//! 評価は特徴量の線形和なので、eval_terms() が特徴量を列挙し、evaluate() と tune の両方が使う。

use crate::params::P;
use crate::position::*;

pub const IDX_PIECE: usize = 0;
pub const IDX_HAND: usize = 15;
pub const IDX_DEF_GOLD: usize = 23;
pub const IDX_DEF_SILVER: usize = 32;
pub const IDX_DEF_HORSE: usize = 41;
pub const IDX_ATK_MINOR: usize = 50;
pub const IDX_ATK_MAJOR: usize = 59;
pub const IDX_ATK_PAWN: usize = 68;
pub const IDX_KING_RANK: usize = 77;
pub const IDX_ATK_KL: usize = 86;
pub const NUM_PARAMS: usize = 95;

/// 指し手の並べ替え等で使う駒の価値
pub const PIECE_VALUE: [i32; 15] = [
    0, 90, 315, 405, 495, 540, 855, 990, 15000, 540, 540, 540, 540, 945, 1395,
];

#[inline]
fn cheb(a: usize, b: usize) -> usize {
    let dr = (a / 9) as i32 - (b / 9) as i32;
    let dc = (a % 9) as i32 - (b % 9) as i32;
    dr.abs().max(dc.abs()) as usize
}

/// 評価の特徴量を列挙する: f(color, param_index, count)
#[inline(always)]
pub fn eval_terms<F: FnMut(Color, usize, i32)>(pos: &Position, mut f: F) {
    let ksq = pos.king_sq;
    for sq in 0..81 {
        let p = pos.board[sq];
        if p == EMPTY {
            continue;
        }
        let pt = ptype(p);
        let c = color_of(p);
        if pt == KING {
            let r = sq / 9;
            let rel = if c == BLACK { 8 - r } else { r };
            f(c, IDX_KING_RANK + rel, 1);
            continue;
        }
        f(c, IDX_PIECE + pt as usize, 1);
        let d_own = cheb(sq, ksq[c]);
        let d_opp = cheb(sq, ksq[c ^ 1]);
        match pt {
            GOLD | PPAWN | PLANCE | PKNIGHT | PSILVER => {
                f(c, IDX_DEF_GOLD + d_own, 1);
                f(c, IDX_ATK_MINOR + d_opp, 1);
            }
            SILVER => {
                f(c, IDX_DEF_SILVER + d_own, 1);
                f(c, IDX_ATK_MINOR + d_opp, 1);
            }
            KNIGHT | LANCE => f(c, IDX_ATK_KL + d_opp, 1),
            HORSE => {
                f(c, IDX_DEF_HORSE + d_own, 1);
                f(c, IDX_ATK_MAJOR + d_opp, 1);
            }
            BISHOP | ROOK | DRAGON => f(c, IDX_ATK_MAJOR + d_opp, 1),
            PAWN => f(c, IDX_ATK_PAWN + d_opp, 1),
            _ => {}
        }
    }
    for c in 0..2 {
        for pt in 1..8 {
            let n = pos.hand[c][pt] as i32;
            if n > 0 {
                f(c, IDX_HAND + pt, n);
            }
        }
    }
}

pub fn evaluate(pos: &Position) -> i32 {
    let mut score = [0i32; 2];
    eval_terms(pos, |c, idx, n| score[c] += P[idx] * n);
    let us = pos.side;
    score[us] - score[us ^ 1]
}
