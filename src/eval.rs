//! 評価関数。手番側から見た評価値（センチポーン相当）を返す。
//!
//! 駒割り + 玉との距離に基づく配置評価（守り駒は自玉の近く、攻め駒は敵玉の近くを評価）
//! + 玉の位置評価。パラメータは params.rs（自動調整の対象）。
//! 評価は特徴量の線形和なので、eval_terms() が特徴量を列挙し、evaluate() と tune の両方が使う。

use crate::params::P;
use crate::position::*;
use std::sync::OnceLock;

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
pub const IDX_PST: usize = 95; // [駒種 1..14][自陣から見た升 81]
pub const IDX_KP_OWN: usize = IDX_PST + 15 * 81; // [駒種 1..14][自玉との相対 17x17]
pub const IDX_KP_OPP: usize = IDX_KP_OWN + 15 * 289; // [駒種 1..14][敵玉との相対 17x17]
pub const IDX_TEMPO: usize = IDX_KP_OPP + 15 * 289; // 手番ボーナス
pub const NUM_PARAMS: usize = IDX_TEMPO + 1;

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

/// 1 つの駒（盤上）の特徴量を列挙する。king_sq は両玉の位置。
#[inline(always)]
pub fn piece_terms<F: FnMut(Color, usize, i32)>(sq: usize, p: u8, ksq: &[usize; 2], f: &mut F) {
    let pt = ptype(p);
    let c = color_of(p);
    // 自陣側から見た升（後手は 180 度回転）
    let rsq = if c == BLACK { sq } else { 80 - sq };
    f(c, IDX_PST + pt as usize * 81 + rsq, 1);
    if pt == KING {
        let r = sq / 9;
        let rel = if c == BLACK { 8 - r } else { r };
        f(c, IDX_KING_RANK + rel, 1);
        return;
    }
    f(c, IDX_PIECE + pt as usize, 1);
    {
        let (mut dr, mut dc) = ((sq / 9) as i32 - (ksq[c] / 9) as i32, (sq % 9) as i32 - (ksq[c] % 9) as i32);
        let (mut er, mut ec) = ((sq / 9) as i32 - (ksq[c ^ 1] / 9) as i32, (sq % 9) as i32 - (ksq[c ^ 1] % 9) as i32);
        if c == WHITE {
            dr = -dr;
            dc = -dc;
            er = -er;
            ec = -ec;
        }
        f(c, IDX_KP_OWN + pt as usize * 289 + ((dr + 8) * 17 + dc + 8) as usize, 1);
        f(c, IDX_KP_OPP + pt as usize * 289 + ((er + 8) * 17 + ec + 8) as usize, 1);
    }
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

/// 評価の特徴量を列挙する: f(color, param_index, count)
#[inline(always)]
pub fn eval_terms<F: FnMut(Color, usize, i32)>(pos: &Position, mut f: F) {
    for sq in 0..81 {
        let p = pos.board[sq];
        if p != EMPTY {
            piece_terms(sq, p, &pos.king_sq, &mut f);
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
    f(pos.side, IDX_TEMPO, 1);
}

/// 盤面全体から各手番の評価合計を計算する（Position が差分更新の初期値に使う）
/// 手番ボーナスを含まない各色の評価合計
pub fn compute_scores(pos: &Position) -> [i32; 2] {
    let mut score = [0i32; 2];
    eval_terms(pos, |c, idx, n| {
        if idx != IDX_TEMPO {
            score[c] += P[idx] * n;
        }
    });
    score
}

/// 駒 1 枚分の評価（その駒の色の側に加算される値）
#[inline]
pub fn piece_score(sq: usize, p: u8, ksq: &[usize; 2]) -> i32 {
    let mut s = 0;
    piece_terms(sq, p, ksq, &mut |_, idx, n| s += P[idx] * n);
    s
}

#[inline]
pub fn hand_score(pt: usize) -> i32 {
    P[IDX_HAND + pt]
}

#[inline]
pub fn evaluate(pos: &Position) -> i32 {
    let us = pos.side;
    let lin = pos.eval[us] - pos.eval[us ^ 1] + P[IDX_TEMPO];
    if nn().enabled {
        lin + nn_output(&pos.nn, us) as i32
    } else {
        lin
    }
}

// ---- 小さなニューラルネット（線形評価への残差項） ----
// 入力: PST / KP_OWN / KP_OPP / 持ち駒（各手番視点の疎な特徴）→ 隠れ層 NN_H（クリップ ReLU）→ 出力。
// 隠れ層の値は do_move/undo_move で差分更新する（Position::nn）。重みは nn.bin（f32 リトルエンディアン）。
pub const NN_H: usize = 32;
pub const NN_HAND: usize = IDX_TEMPO - IDX_PST; // 持ち駒特徴の先頭
pub const NN_IN: usize = NN_HAND + 8;

/// 線形評価の特徴番号 → NN の入力番号
#[inline(always)]
pub fn nn_index(idx: usize) -> Option<usize> {
    if idx >= IDX_PST && idx < IDX_TEMPO {
        Some(idx - IDX_PST)
    } else if idx >= IDX_HAND && idx < IDX_HAND + 8 {
        Some(NN_HAND + idx - IDX_HAND)
    } else {
        None
    }
}

pub struct Nn {
    pub enabled: bool,
    pub w1: Vec<[f32; NN_H]>,
    pub b1: [f32; NN_H],
    pub v: [[f32; NN_H]; 2], // [手番側, 相手側]
}

pub fn nn() -> &'static Nn {
    static N: OnceLock<Nn> = OnceLock::new();
    N.get_or_init(|| {
        let bytes: &[u8] = include_bytes!("nn.bin");
        let total = NN_IN * NN_H + NN_H + 2 * NN_H;
        let mut nn = Nn { enabled: false, w1: Vec::new(), b1: [0.0; NN_H], v: [[0.0; NN_H]; 2] };
        if bytes.len() != total * 4 {
            return nn;
        }
        let f: Vec<f32> = bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        for i in 0..NN_IN {
            let mut row = [0.0f32; NN_H];
            row.copy_from_slice(&f[i * NN_H..(i + 1) * NN_H]);
            nn.w1.push(row);
        }
        let o = NN_IN * NN_H;
        nn.b1.copy_from_slice(&f[o..o + NN_H]);
        nn.v[0].copy_from_slice(&f[o + NN_H..o + 2 * NN_H]);
        nn.v[1].copy_from_slice(&f[o + 2 * NN_H..o + 3 * NN_H]);
        nn.enabled = true;
        nn
    })
}

#[inline(always)]
pub fn nn_axpy(acc: &mut [f32; NN_H], row: &[f32; NN_H], k: f32) {
    for i in 0..NN_H {
        acc[i] += row[i] * k;
    }
}

/// 盤面全体から両色の隠れ層（活性化前）を計算する
pub fn compute_nn(pos: &Position) -> [[f32; NN_H]; 2] {
    let n = nn();
    let mut acc = [n.b1, n.b1];
    if !n.enabled {
        return acc;
    }
    eval_terms(pos, |c, idx, k| {
        if let Some(i) = nn_index(idx) {
            nn_axpy(&mut acc[c], &n.w1[i], k as f32);
        }
    });
    acc
}

/// 盤上の駒 1 枚の NN 寄与を加減算する（sign = +1 / -1）
#[inline]
pub fn nn_piece(acc: &mut [[f32; NN_H]; 2], sq: usize, p: u8, ksq: &[usize; 2], sign: f32) {
    let n = nn();
    piece_terms(sq, p, ksq, &mut |c, idx, k| {
        if let Some(i) = nn_index(idx) {
            nn_axpy(&mut acc[c], &n.w1[i], sign * k as f32);
        }
    });
}

#[inline]
pub fn nn_hand(acc: &mut [f32; NN_H], pt: usize, delta: f32) {
    nn_axpy(acc, &nn().w1[NN_HAND + pt], delta);
}

#[inline]
pub fn nn_output(acc: &[[f32; NN_H]; 2], us: usize) -> f32 {
    let n = nn();
    let mut s = 0.0f32;
    for i in 0..NN_H {
        s += n.v[0][i] * acc[us][i].clamp(0.0, 1.0) + n.v[1][i] * acc[us ^ 1][i].clamp(0.0, 1.0);
    }
    s
}
