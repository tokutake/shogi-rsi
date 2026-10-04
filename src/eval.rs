//! 評価関数。手番側から見た評価値（センチポーン相当）を返す。

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

pub fn evaluate(pos: &Position) -> i32 {
    let mut score = [0i32; 2];
    for sq in 0..81 {
        let p = pos.board[sq];
        if p == EMPTY {
            continue;
        }
        let pt = ptype(p);
        if pt == KING {
            continue;
        }
        score[color_of(p)] += PIECE_VALUE[pt as usize];
    }
    for c in 0..2 {
        for pt in 1..8 {
            score[c] += HAND_VALUE[pt] * pos.hand[c][pt] as i32;
        }
    }
    let us = pos.side;
    score[us] - score[us ^ 1]
}
