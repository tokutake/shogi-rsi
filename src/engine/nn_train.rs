//! NN（線形評価への残差項）の学習。
//!
//!   shogi-rsi nntrain --labels labels.txt [--labels ...] --out src/engine/nn.bin [--epochs 40] [--init src/engine/nn.bin]
//!
//! --init には持ち駒の枚数特徴がない旧形式（v25 以前）の重みも渡せる（その重みを 0 として始める）。
//!
//! 教師は `label` が出力する `sfen|探索評価値|勝敗`。目標は手番側の勝率
//! t = λ·結果 + (1-λ)·sigmoid(探索評価値·K)、予測は sigmoid(K·(線形評価 + NN 出力))。
//! 線形評価は現行の params.rs を固定して使い、NN だけを学習する。

use super::eval::*;
use super::params::P;
use super::position::*;

// 入力特徴は全局面ぶんを 1 本の配列に詰めて持つ（局面ごとの Vec だと数百万局面でメモリが足りない）
struct Samples {
    feats: Vec<(u16, u16)>, // (特徴番号, 個数)
    start: Vec<u64>,        // 局面 i の [手番側, 相手側] は feats[start[i]..mid[i]], feats[mid[i]..start[i+1]]
    mid: Vec<u64>,
    lin: Vec<f32>,          // 線形評価（手番側から見た値）
    target: Vec<f32>,
}

impl Samples {
    fn new() -> Samples {
        Samples { feats: Vec::new(), start: vec![0], mid: Vec::new(), lin: Vec::new(), target: Vec::new() }
    }
    fn len(&self) -> usize {
        self.lin.len()
    }
    fn side(&self, i: usize, side: usize) -> &[(u16, u16)] {
        let (a, m, b) = (self.start[i] as usize, self.mid[i] as usize, self.start[i + 1] as usize);
        if side == 0 { &self.feats[a..m] } else { &self.feats[m..b] }
    }
    fn push(&mut self, f: &[Vec<(u16, u16)>; 2], lin: f32, target: f32) {
        self.feats.extend_from_slice(&f[0]);
        self.mid.push(self.feats.len() as u64);
        self.feats.extend_from_slice(&f[1]);
        self.start.push(self.feats.len() as u64);
        self.lin.push(lin);
        self.target.push(target);
    }
}

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// 10 局面に 1 つを検証用に回し、(学習, 検証) を返す
fn load(paths: &[String], lambda: f32, k: f32) -> (Samples, Samples) {
    let (mut train, mut valid) = (Samples::new(), Samples::new());
    let mut n = 0usize;
    let mut f: [Vec<(u16, u16)>; 2] = [Vec::with_capacity(96), Vec::with_capacity(96)];
    for path in paths {
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        for line in text.lines() {
            let mut it = line.split('|');
            let (Some(sfen), Some(sc), Some(res)) = (it.next(), it.next(), it.next()) else { continue };
            let (Ok(sc), Ok(res)) = (sc.parse::<f32>(), res.parse::<f32>()) else { continue };
            let Some(pos) = Position::from_sfen(sfen) else { continue };
            let us = pos.side;
            let res_us = if us == BLACK { res } else { 1.0 - res };
            let target = lambda * res_us + (1.0 - lambda) * sigmoid(sc * k);
            f[0].clear();
            f[1].clear();
            let mut lin = [0i32; 2];
            eval_terms(&pos, |c, idx, n| {
                if idx != IDX_TEMPO {
                    lin[c] += P[idx] * n;
                }
                if let Some(i) = nn_index(idx) {
                    let side = if c == us { 0 } else { 1 };
                    f[side].push((i as u16, n as u16));
                    f[side ^ 1].push(((NN_BASE + i) as u16, n as u16));
                }
            });
            let lin_us = (lin[us] - lin[us ^ 1] + P[IDX_TEMPO]) as f32;
            if n % 10 == 0 { &mut valid } else { &mut train }.push(&f, lin_us, target);
            n += 1;
        }
    }
    (train, valid)
}

struct Net {
    w1: Vec<f32>, // NN_IN * NN_H
    b1: Vec<f32>,
    v: Vec<f32>, // 2 * NN_H
}

impl Net {
    fn len() -> usize {
        NN_IN * NN_H + NN_H + 2 * NN_H
    }
    fn to_flat(&self) -> Vec<f32> {
        let mut x = Vec::with_capacity(Self::len());
        x.extend_from_slice(&self.w1);
        x.extend_from_slice(&self.b1);
        x.extend_from_slice(&self.v);
        x
    }
    fn from_flat(x: &[f32]) -> Net {
        Net {
            w1: x[..NN_IN * NN_H].to_vec(),
            b1: x[NN_IN * NN_H..NN_IN * NN_H + NN_H].to_vec(),
            v: x[NN_IN * NN_H + NN_H..].to_vec(),
        }
    }
    fn forward(&self, d: &Samples, si: usize, h: &mut [[f32; NN_H]; 2], a: &mut [[f32; NN_H]; 2]) -> f32 {
        for side in 0..2 {
            a[side].copy_from_slice(&self.b1);
            for &(i, n) in d.side(si, side) {
                let n = n as f32;
                let row = &self.w1[i as usize * NN_H..(i as usize + 1) * NN_H];
                for j in 0..NN_H {
                    a[side][j] += row[j] * n;
                }
            }
            for j in 0..NN_H {
                h[side][j] = a[side][j].clamp(0.0, 1.0);
            }
        }
        let mut out = 0.0;
        for side in 0..2 {
            for j in 0..NN_H {
                out += self.v[side * NN_H + j] * h[side][j];
            }
        }
        out
    }
}

fn loss(net: &Net, data: &Samples, k: f32, use_nn: bool) -> f64 {
    let nth = 4;
    let chunk = (data.len() + nth - 1) / nth;
    let idx: Vec<usize> = (0..data.len()).collect();
    let total: f64 = std::thread::scope(|sc| {
        let hs: Vec<_> = idx
            .chunks(chunk)
            .map(|ch| {
                sc.spawn(move || {
                    let mut h = [[0.0; NN_H]; 2];
                    let mut a = [[0.0; NN_H]; 2];
                    let mut l = 0.0f64;
                    for &si in ch {
                        let o = if use_nn { net.forward(data, si, &mut h, &mut a) } else { 0.0 };
                        let p = sigmoid(k * (data.lin[si] + o));
                        l += ((data.target[si] - p) as f64).powi(2);
                    }
                    l
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum()
    });
    total / data.len() as f64
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn uniform(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

pub fn run_nntrain(args: &[String]) {
    let mut labels = Vec::new();
    let mut out = "src/engine/nn.bin".to_string();
    let mut init: Option<String> = None;
    let mut epochs = 40;
    let mut lambda = 0.0f32;
    let mut k = 0.004f32;
    let mut lr_w = 0.002f32;
    let mut lr_v = 0.5f32;
    let mut batch = 4096usize;
    let mut l2 = 0.0f32;
    let mut i = 0;
    while i + 1 < args.len() {
        match args[i].as_str() {
            "--labels" => labels.push(args[i + 1].clone()),
            "--out" => out = args[i + 1].clone(),
            "--init" => init = Some(args[i + 1].clone()),
            "--epochs" => epochs = args[i + 1].parse().unwrap(),
            "--lambda" => lambda = args[i + 1].parse().unwrap(),
            "--k" => k = args[i + 1].parse().unwrap(),
            "--lr-w" => lr_w = args[i + 1].parse().unwrap(),
            "--lr-v" => lr_v = args[i + 1].parse().unwrap(),
            "--batch" => batch = args[i + 1].parse().unwrap(),
            "--l2" => l2 = args[i + 1].parse().unwrap(),
            _ => {}
        }
        i += 2;
    }
    let (train, valid) = load(&labels, lambda, k);
    eprintln!("samples: {}", train.len() + valid.len());
    let mut rng = Rng(0x2545F4914F6CDD1D);
    let mut net = match &init {
        Some(path) => {
            let b = std::fs::read(path).unwrap();
            let f: Vec<f32> = b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
            let old_base = NN_HANDK; // 持ち駒の枚数特徴がない旧形式の 1 視点あたりの入力数
            if f.len() == 2 * old_base * NN_H + 3 * NN_H {
                // 持ち駒の枚数特徴がない旧形式: 各視点の末尾に 0 の行を足す（出力は旧形式と同じ）
                let mut w1 = vec![0.0; NN_IN * NN_H];
                w1[..old_base * NN_H].copy_from_slice(&f[..old_base * NN_H]);
                w1[NN_BASE * NN_H..(NN_BASE + old_base) * NN_H].copy_from_slice(&f[old_base * NN_H..2 * old_base * NN_H]);
                let o = 2 * old_base * NN_H;
                Net { w1, b1: f[o..o + NN_H].to_vec(), v: f[o + NN_H..].to_vec() }
            } else {
                assert_eq!(f.len(), Net::len(), "init size mismatch");
                Net::from_flat(&f)
            }
        }
        None => Net {
            w1: (0..NN_IN * NN_H).map(|_| (rng.uniform() - 0.5) * 0.1).collect(),
            b1: vec![0.5; NN_H],
            v: vec![0.0; 2 * NN_H],
        },
    };
    eprintln!("linear only: valid loss {:.6}", loss(&net, &valid, k, false));
    eprintln!("initial   : valid loss {:.6}", loss(&net, &valid, k, true));

    let n_par = Net::len();
    let mut m = vec![0.0f32; n_par];
    let mut vv = vec![0.0f32; n_par];
    let (b1c, b2c, eps) = (0.9f32, 0.999f32, 1e-8f32);
    let mut step = 0i32;
    let mut best = loss(&net, &valid, k, true);
    let mut best_net = net.to_flat();
    let mut order: Vec<usize> = (0..train.len()).collect();
    let nth = 4;
    for epoch in 1..=epochs {
        // シャッフル
        for i in (1..order.len()).rev() {
            let j = (rng.next() % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        for bt in order.chunks(batch) {
            let flat = net.to_flat();
            let cur = Net::from_flat(&flat);
            let chunk = (bt.len() + nth - 1) / nth;
            let parts: Vec<Vec<f32>> = std::thread::scope(|sc| {
                let hs: Vec<_> = bt
                    .chunks(chunk)
                    .map(|ch| {
                        let cur = &cur;
                        let train = &train;
                        sc.spawn(move || {
                            let mut g = vec![0.0f32; n_par];
                            let mut h = [[0.0; NN_H]; 2];
                            let mut a = [[0.0; NN_H]; 2];
                            for &si in ch {
                                let o = cur.forward(train, si, &mut h, &mut a);
                                let p = sigmoid(k * (train.lin[si] + o));
                                let gout = -2.0 * (train.target[si] - p) * p * (1.0 - p) * k;
                                for side in 0..2 {
                                    let mut da = [0.0f32; NN_H];
                                    for j in 0..NN_H {
                                        g[NN_IN * NN_H + NN_H + side * NN_H + j] += gout * h[side][j];
                                        if a[side][j] > 0.0 && a[side][j] < 1.0 {
                                            da[j] = gout * cur.v[side * NN_H + j];
                                        }
                                    }
                                    for j in 0..NN_H {
                                        g[NN_IN * NN_H + j] += da[j];
                                    }
                                    for &(fi, n) in train.side(si, side) {
                                        let n = n as f32;
                                        let base = fi as usize * NN_H;
                                        for j in 0..NN_H {
                                            g[base + j] += da[j] * n;
                                        }
                                    }
                                }
                            }
                            g
                        })
                    })
                    .collect();
                hs.into_iter().map(|h| h.join().unwrap()).collect()
            });
            step += 1;
            let bn = bt.len() as f32;
            let mut x = flat;
            for idx in 0..n_par {
                let mut g = 0.0;
                for p in parts.iter() {
                    g += p[idx];
                }
                g /= bn;
                let is_v = idx >= NN_IN * NN_H + NN_H;
                if l2 > 0.0 && !is_v {
                    g += l2 * x[idx];
                }
                m[idx] = b1c * m[idx] + (1.0 - b1c) * g;
                vv[idx] = b2c * vv[idx] + (1.0 - b2c) * g * g;
                let mh = m[idx] / (1.0 - b1c.powi(step));
                let vh = vv[idx] / (1.0 - b2c.powi(step));
                let lr = if is_v { lr_v } else { lr_w };
                x[idx] -= lr * mh / (vh.sqrt() + eps);
            }
            net = Net::from_flat(&x);
        }
        let tl = loss(&net, &train, k, true);
        let vl = loss(&net, &valid, k, true);
        eprintln!("epoch {:3}  train {:.6}  valid {:.6}", epoch, tl, vl);
        if vl < best {
            best = vl;
            best_net = net.to_flat();
            let bytes: Vec<u8> = best_net.iter().flat_map(|f| f.to_le_bytes()).collect();
            std::fs::write(&out, bytes).expect("write nn");
        }
    }
    eprintln!("best valid loss {:.6} -> {}", best, out);
}
