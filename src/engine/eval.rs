//! 評価関数。手番側から見た評価値（センチポーン相当）を返す。
//!
//! 駒割り + 玉との距離に基づく配置評価（守り駒は自玉の近く、攻め駒は敵玉の近くを評価）
//! + 玉の位置評価。パラメータは params.rs（自動調整の対象）。
//! 評価は特徴量の線形和なので、eval_terms() が特徴量を列挙し、evaluate() と tune の両方が使う。

use super::params::P;
use super::position::*;
use std::mem::MaybeUninit;
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
pub const IDX_HANDK: usize = IDX_TEMPO + 1; // [駒種 0..7][k 0..18] 持ち駒が k 枚以上なら 1（枚数に対する非線形な価値）
pub const HANDK_W: usize = 19;
pub const NUM_PARAMS: usize = IDX_HANDK + 8 * HANDK_W;

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
                for k in 1..=n as usize {
                    f(c, IDX_HANDK + pt * HANDK_W + k, 1);
                }
            }
        }
    }
    f(pos.side, IDX_TEMPO, 1);
}

/// 線形評価の駒 1 枚分を 3 回の表引きで求めるための合成表（piece_terms の和と同一）
struct LinTables {
    /// [駒(色込み)][升]: PST + 駒割り（玉は PST + 玉の段）
    pst: [[i32; 81]; 32],
    /// [駒種][自玉との相対 17x17]: KP_OWN + 自玉との距離の項
    own: [[i32; 289]; 15],
    /// [駒種][敵玉との相対 17x17]: KP_OPP + 敵玉との距離の項
    opp: [[i32; 289]; 15],
    /// [升][玉の升] → 先手視点の相対位置の番号（後手は 288 - これ）
    rel: [[u16; 81]; 81],
}

fn lin_tables() -> &'static LinTables {
    static T: OnceLock<LinTables> = OnceLock::new();
    T.get_or_init(|| {
        // const の P を直接添字で引くと毎回スタックへ複製されるので、static への参照で引く
        static PS: [i32; NUM_PARAMS] = P;
        let pp = &PS;
        let mut t = LinTables { pst: [[0; 81]; 32], own: [[0; 289]; 15], opp: [[0; 289]; 15], rel: [[0; 81]; 81] };
        for sq in 0..81 {
            for k in 0..81 {
                let dr = (sq / 9) as i32 - (k / 9) as i32;
                let dc = (sq % 9) as i32 - (k % 9) as i32;
                t.rel[sq][k] = ((dr + 8) * 17 + dc + 8) as u16;
            }
        }
        for c in 0..2 {
            for pt in 1..15u8 {
                let p = make_piece(c, pt);
                for sq in 0..81 {
                    let rsq = if c == BLACK { sq } else { 80 - sq };
                    let mut v = pp[IDX_PST + pt as usize * 81 + rsq];
                    if pt == KING {
                        let r = sq / 9;
                        v += pp[IDX_KING_RANK + if c == BLACK { 8 - r } else { r }];
                    } else {
                        v += pp[IDX_PIECE + pt as usize];
                    }
                    t.pst[p as usize][sq] = v;
                }
            }
        }
        for pt in 1..15u8 {
            if pt == KING {
                continue;
            }
            for rel in 0..289 {
                let d = ((rel / 17) as i32 - 8).abs().max(((rel % 17) as i32 - 8).abs()) as usize;
                let (def, atk) = match pt {
                    GOLD | PPAWN | PLANCE | PKNIGHT | PSILVER => (pp[IDX_DEF_GOLD + d], pp[IDX_ATK_MINOR + d]),
                    SILVER => (pp[IDX_DEF_SILVER + d], pp[IDX_ATK_MINOR + d]),
                    KNIGHT | LANCE => (0, pp[IDX_ATK_KL + d]),
                    HORSE => (pp[IDX_DEF_HORSE + d], pp[IDX_ATK_MAJOR + d]),
                    BISHOP | ROOK | DRAGON => (0, pp[IDX_ATK_MAJOR + d]),
                    PAWN => (0, pp[IDX_ATK_PAWN + d]),
                    _ => (0, 0),
                };
                t.own[pt as usize][rel] = pp[IDX_KP_OWN + pt as usize * 289 + rel] + def;
                t.opp[pt as usize][rel] = pp[IDX_KP_OPP + pt as usize * 289 + rel] + atk;
            }
        }
        t
    })
}

/// 盤面全体から各手番の評価合計を計算する（Position が差分更新の初期値に使う）
/// 手番ボーナスを含まない各色の評価合計
pub fn compute_scores(pos: &Position) -> [i32; 2] {
    let mut score = [0i32; 2];
    for sq in 0..81 {
        let p = pos.board[sq];
        if p != EMPTY {
            score[color_of(p)] += piece_score(sq, p, &pos.king_sq);
        }
    }
    for c in 0..2 {
        for pt in 1..8 {
            for k in 1..=pos.hand[c][pt] as usize {
                score[c] += hand_score(pt, k);
            }
        }
    }
    score
}

/// 駒 1 枚分の評価（その駒の色の側に加算される値）。piece_terms の和と同じ
#[inline]
pub fn piece_score(sq: usize, p: u8, ksq: &[usize; 2]) -> i32 {
    let t = lin_tables();
    let v = t.pst[p as usize][sq];
    let pt = ptype(p) as usize;
    if pt == KING as usize {
        return v;
    }
    let c = color_of(p);
    let (ro, re) = (t.rel[sq][ksq[c]] as usize, t.rel[sq][ksq[c ^ 1]] as usize);
    let (ro, re) = if c == BLACK { (ro, re) } else { (288 - ro, 288 - re) };
    v + t.own[pt][ro] + t.opp[pt][re]
}

/// piece_terms から求める版（検証用）
pub fn piece_score_slow(sq: usize, p: u8, ksq: &[usize; 2]) -> i32 {
    let mut s = 0;
    piece_terms(sq, p, ksq, &mut |_, idx, n| s += P[idx] * n);
    s
}

/// eval_terms から求める版（検証用）
pub fn compute_scores_slow(pos: &Position) -> [i32; 2] {
    let mut score = [0i32; 2];
    eval_terms(pos, |c, idx, n| {
        if idx != IDX_TEMPO {
            score[c] += P[idx] * n;
        }
    });
    score
}

/// 持ち駒 pt の n 枚目の評価
#[inline]
pub fn hand_score(pt: usize, n: usize) -> i32 {
    P[IDX_HAND + pt] + P[IDX_HANDK + pt * HANDK_W + n]
}

#[inline]
pub fn evaluate(pos: &mut Position) -> i32 {
    let us = pos.side;
    let e = pos.lin_scores();
    let lin = e[us] - e[us ^ 1] + P[IDX_TEMPO];
    if nn().enabled {
        lin + nn_output(pos.nn_acc(), us) as i32
    } else {
        lin
    }
}

// ---- 小さなニューラルネット（線形評価への残差項） ----
// 入力: PST / KP_OWN / KP_OPP / 持ち駒（各手番視点の疎な特徴）→ 隠れ層 NN_H（クリップ ReLU）→ 出力。
// 各視点の隠れ層には自分の駒の特徴（入力 0..NN_BASE）と相手の駒の特徴（NN_BASE..NN_IN）の両方が入る
// （自分の攻め駒と相手の守り駒の組み合わせなどを非線形に評価できる）。
// 隠れ層の値は do_move/undo_move で差分更新する（Position::nn）。重みは nn.bin（f32 リトルエンディアン）。
pub const NN_H: usize = 32;
pub const NN_HAND: usize = IDX_TEMPO - IDX_PST; // 持ち駒特徴の先頭
pub const NN_HANDK: usize = NN_HAND + 8; // 持ち駒の枚数（k 枚以上）特徴の先頭
pub const NN_BASE: usize = NN_HANDK + 8 * HANDK_W;
pub const NN_IN: usize = 2 * NN_BASE;

/// 線形評価の特徴番号 → NN の入力番号
#[inline(always)]
pub fn nn_index(idx: usize) -> Option<usize> {
    if idx >= IDX_PST && idx < IDX_TEMPO {
        Some(idx - IDX_PST)
    } else if idx >= IDX_HAND && idx < IDX_HAND + 8 {
        Some(NN_HAND + idx - IDX_HAND)
    } else if idx >= IDX_HANDK && idx < NUM_PARAMS {
        Some(NN_HANDK + idx - IDX_HANDK)
    } else {
        None
    }
}

/// 隠れ層の量子化の倍率（推論は i16 の重み・i16 の隠れ層で計算し、出力で 1/NN_Q する）。
/// 隠れ層の値は ±32 まで表せる（実戦の探索では最大 9.4 程度）。
pub const NN_Q: f32 = 1024.0;
pub type NnAcc = [[i16; NN_H]; 2];

/// w1 の 1 行（i16 x 32 = 64 バイト = キャッシュライン 1 本に揃える）
#[derive(Clone, Copy)]
#[repr(C, align(64))]
pub struct NnRow(pub [i16; NN_H]);

pub struct Nn {
    pub enabled: bool,
    pub w1: Vec<NnRow>,
    pub b1: [i16; NN_H],
    pub v: [[f32; NN_H]; 2], // [手番側, 相手側]。量子化した隠れ層に掛けるため 1/NN_Q 済み
}

#[inline(always)]
fn quant(x: f32) -> i32 {
    (x * NN_Q).round() as i32
}

pub fn nn() -> &'static Nn {
    static N: OnceLock<Nn> = OnceLock::new();
    N.get_or_init(|| {
        let bytes: &[u8] = include_bytes!("nn.bin");
        let total = NN_IN * NN_H + NN_H + 2 * NN_H;
        let mut nn = Nn { enabled: false, w1: Vec::new(), b1: [0; NN_H], v: [[0.0; NN_H]; 2] };
        if bytes.len() != total * 4 {
            return nn;
        }
        let f: Vec<f32> = bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        nn.w1.reserve_exact(NN_IN);
        for i in 0..NN_IN {
            let mut row = [0i16; NN_H];
            for j in 0..NN_H {
                row[j] = quant(f[i * NN_H + j]).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            }
            nn.w1.push(NnRow(row));
        }
        let o = NN_IN * NN_H;
        for j in 0..NN_H {
            nn.b1[j] = quant(f[o + j]).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            nn.v[0][j] = f[o + NN_H + j] / NN_Q;
            nn.v[1][j] = f[o + 2 * NN_H + j] / NN_Q;
        }
        nn.enabled = true;
        nn
    })
}

/// 隠れ層への加算（w1 の行番号と係数）を集めて、各視点の隠れ層ごとに 1 回の走査でまとめて足す。
/// 各要素への加算順は 1 行ずつ足すのと同じなので結果も同一。
pub struct NnDelta<const N: usize> {
    rows: [[MaybeUninit<(u32, i32)>; N]; 2], // 先頭 n[c] 個だけ初期化済み
    n: [usize; 2],
}

impl<const N: usize> NnDelta<N> {
    #[inline(always)]
    pub fn new() -> Self {
        // MaybeUninit の配列は初期化不要（ゼロ埋めの memset を避ける）
        NnDelta { rows: [[const { MaybeUninit::uninit() }; N]; 2], n: [0, 0] }
    }
    #[inline(always)]
    fn push(&mut self, c: Color, row: usize, k: i32) {
        self.rows[c][self.n[c]] = MaybeUninit::new((row as u32, k));
        self.n[c] += 1;
    }
    /// 線形評価の特徴 (c, idx, k) を NN の両視点に加える
    #[inline(always)]
    fn term(&mut self, c: Color, idx: usize, k: i32) {
        if let Some(i) = nn_index(idx) {
            self.push(c, i, k);
            self.push(c ^ 1, NN_BASE + i, k);
        }
    }
    /// 盤上の駒 1 枚の寄与を加減算する（sign = +1 / -1）
    #[inline(always)]
    pub fn piece(&mut self, sq: usize, p: u8, ksq: &[usize; 2], sign: i32) {
        piece_terms(sq, p, ksq, &mut |c, idx, k| self.term(c, idx, sign * k));
    }
    /// 持ち駒 pt の k 枚目を加える（delta = +1）/ 除く（delta = -1）
    #[inline(always)]
    pub fn hand(&mut self, c: Color, pt: usize, k: usize, delta: i32) {
        self.push(c, NN_HAND + pt, delta);
        self.push(c ^ 1, NN_BASE + NN_HAND + pt, delta);
        self.push(c, NN_HANDK + pt * HANDK_W + k, delta);
        self.push(c ^ 1, NN_BASE + NN_HANDK + pt * HANDK_W + k, delta);
    }
    #[inline(always)]
    pub fn apply(&self, acc: &mut NnAcc) {
        let w1 = &nn().w1;
        for c in 0..2 {
            let mut a = acc[c];
            for e in &self.rows[c][..self.n[c]] {
                // SAFETY: 先頭 n[c] 個は push で初期化済み
                let (r, k) = unsafe { e.assume_init() };
                let row = &w1[r as usize].0;
                if k == 1 {
                    for i in 0..NN_H {
                        a[i] = a[i].wrapping_add(row[i]);
                    }
                } else if k == -1 {
                    for i in 0..NN_H {
                        a[i] = a[i].wrapping_sub(row[i]);
                    }
                } else {
                    let k = k as i16;
                    for i in 0..NN_H {
                        a[i] = a[i].wrapping_add(row[i].wrapping_mul(k));
                    }
                }
            }
            acc[c] = a;
        }
    }
}

/// 盤面全体から両色の隠れ層（活性化前）を計算する
pub fn compute_nn(pos: &Position) -> NnAcc {
    let n = nn();
    let mut acc = [n.b1, n.b1];
    if !n.enabled {
        return acc;
    }
    let mut d = NnDelta::<320>::new();
    eval_terms(pos, |c, idx, k| d.term(c, idx, k));
    d.apply(&mut acc);
    acc
}

#[inline]
pub fn nn_output(acc: &NnAcc, us: usize) -> f32 {
    let n = nn();
    let q = NN_Q as i16;
    let mut s = 0.0f32;
    for i in 0..NN_H {
        s += n.v[0][i] * acc[us][i].clamp(0, q) as f32 + n.v[1][i] * acc[us ^ 1][i].clamp(0, q) as f32;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_linear_matches_terms() {
        for c in 0..2 {
            for pt in 1..15u8 {
                let p = make_piece(c, pt);
                for sq in 0..81 {
                    for k0 in (0..81).step_by(7) {
                        for k1 in (0..81).step_by(5) {
                            let ksq = [k0, k1];
                            assert_eq!(piece_score(sq, p, &ksq), piece_score_slow(sq, p, &ksq));
                        }
                    }
                }
            }
        }
        let pos = Position::from_sfen("l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w RGgsn5p 1").unwrap();
        assert_eq!(compute_scores(&pos), compute_scores_slow(&pos));
    }

    #[test]
    fn nn_incremental_matches_full() {
        // 乱数で手を進め、ときどきだけ評価して（差分の鎖・玉移動を挟む）全計算と一致するか確かめる
        let mut pos = Position::from_sfen(STARTPOS_SFEN).unwrap();
        let mut seed: u64 = 12345;
        let mut rnd = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let mut checked = 0;
        for _game in 0..20 {
            let mut pos2 = Position::from_sfen(STARTPOS_SFEN).unwrap();
            for _ply in 0..150 {
                let moves = pos2.legal_moves();
                if moves.is_empty() {
                    break;
                }
                pos2.do_move(moves[rnd(moves.len())]);
                if rnd(3) == 0 {
                    let side = pos2.side;
                    let a = nn_output(pos2.nn_acc(), side);
                    let b = nn_output(&compute_nn(&pos2), side);
                    assert_eq!(a, b);
                    checked += 1;
                }
            }
            // 戻しても一致する
            while pos2.ply > 0 && rnd(4) != 0 {
                pos2.undo_move();
                let side = pos2.side;
                let a = nn_output(pos2.nn_acc(), side);
                let b = nn_output(&compute_nn(&pos2), side);
                assert_eq!(a, b);
            }
            pos = pos2;
        }
        let _ = pos;
        assert!(checked > 100);
    }
}
