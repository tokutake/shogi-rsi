//! 評価関数。手番側から見た評価値（センチポーン相当）を返す。
//!
//! 駒割り + 玉との距離に基づく配置評価（守り駒は自玉の近く、攻め駒は敵玉の近くを評価）
//! + 玉の位置評価。

use crate::position::*;

/// 盤上の駒の価値（駒種インデックス）
pub const PIECE_VALUE: [i32; 15] = [
    0,    // empty
    90,   // 歩
    315,  // 香
    405,  // 桂
    495,  // 銀
    540,  // 金
    855,  // 角
    990,  // 飛
    15000, // 玉
    540,  // と
    540,  // 成香
    540,  // 成桂
    540,  // 成銀
    945,  // 馬
    1395, // 龍
];

/// 持ち駒の価値（打てる分だけ盤上より少し高い）
pub const HAND_VALUE: [i32; 8] = [0, 100, 350, 450, 550, 600, 950, 1100];

// 距離(チェビシェフ) 0..8 ごとのボーナス
const DEF_GOLD: [i32; 9] = [0, 55, 35, 12, 0, -5, -10, -15, -20];
const DEF_SILVER: [i32; 9] = [0, 45, 35, 15, 0, -5, -8, -10, -12];
const DEF_HORSE: [i32; 9] = [0, 40, 25, 10, 0, 0, 0, 0, 0];
const ATK_MINOR: [i32; 9] = [0, 45, 32, 18, 8, 2, 0, 0, 0];
const ATK_MAJOR: [i32; 9] = [0, 35, 30, 22, 14, 8, 3, 0, 0];
const ATK_PAWN: [i32; 9] = [0, 15, 10, 4, 0, 0, 0, 0, 0];

/// 玉の段（自陣から見た段: 0 = 最下段）ごとのボーナス
const KING_RANK: [i32; 9] = [30, 20, 0, -30, -60, -80, -90, -100, -100];

#[inline]
fn cheb(a: usize, b: usize) -> usize {
    let dr = (a / 9) as i32 - (b / 9) as i32;
    let dc = (a % 9) as i32 - (b % 9) as i32;
    dr.abs().max(dc.abs()) as usize
}

pub fn evaluate(pos: &Position) -> i32 {
    let mut score = [0i32; 2];
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
            score[c] += KING_RANK[rel];
            continue;
        }
        score[c] += PIECE_VALUE[pt as usize];
        let d_own = cheb(sq, ksq[c]);
        let d_opp = cheb(sq, ksq[c ^ 1]);
        score[c] += match pt {
            GOLD | PPAWN | PLANCE | PKNIGHT | PSILVER => DEF_GOLD[d_own] + ATK_MINOR[d_opp],
            SILVER => DEF_SILVER[d_own] + ATK_MINOR[d_opp],
            KNIGHT | LANCE => ATK_MINOR[d_opp] / 2,
            HORSE => DEF_HORSE[d_own] + ATK_MAJOR[d_opp],
            BISHOP | ROOK | DRAGON => ATK_MAJOR[d_opp],
            PAWN => ATK_PAWN[d_opp],
            _ => 0,
        };
    }
    for c in 0..2 {
        for pt in 1..8 {
            score[c] += HAND_VALUE[pt] * pos.hand[c][pt] as i32;
        }
    }
    let us = pos.side;
    score[us] - score[us ^ 1]
}
