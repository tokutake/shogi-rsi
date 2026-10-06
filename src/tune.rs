//! Texel tuning: 対局記録（results/*.jsonl の moves）から局面と勝敗を集め、
//! 評価値から勝率を予測するロジスティック回帰の損失を最小化するよう params.rs を調整する。
//!
//!   shogi-rsi tune --data results/games.jsonl [--data ...] [--epochs 300] [--out src/params.rs]

use crate::eval::{eval_terms, IDX_PIECE, NUM_PARAMS};
use crate::params::P;
use crate::position::*;

struct Sample {
    feats: Vec<(u16, i16)>, // (param idx, 先手から見た係数)
    result: f64,            // 先手から見た結果 1 / 0.5 / 0
}

fn json_str<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{}\":\"", key);
    let i = line.find(&pat)? + pat.len();
    let j = line[i..].find('"')? + i;
    Some(&line[i..j])
}

fn load_labels(paths: &[String], lambda: f64) -> Vec<Sample> {
    let mut out = Vec::new();
    for path in paths {
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        for line in text.lines() {
            let mut it = line.split('|');
            let (Some(sfen), Some(sc), Some(res)) = (it.next(), it.next(), it.next()) else { continue };
            let (Ok(sc), Ok(res)) = (sc.parse::<f64>(), res.parse::<f64>()) else { continue };
            let Some(pos) = Position::from_sfen(sfen) else { continue };
            let sc_black = if pos.side == BLACK { sc } else { -sc };
            let t = lambda * res + (1.0 - lambda) * sigmoid(sc_black * 0.004);
            let mut feats: Vec<(u16, i16)> = Vec::with_capacity(128);
            eval_terms(&pos, |c, idx, n| {
                let v = if c == BLACK { n } else { -n };
                feats.push((idx as u16, v as i16));
            });
            out.push(Sample { feats, result: t });
        }
    }
    out
}

fn load_samples(paths: &[String], min_ply: usize) -> Vec<Sample> {
    let mut out = Vec::new();
    for path in paths {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("skip {}: {}", path, e);
                continue;
            }
        };
        for line in text.lines() {
            let (Some(black), Some(winner), Some(moves)) =
                (json_str(line, "black"), json_str(line, "winner"), json_str(line, "moves"))
            else {
                continue;
            };
            let reason = json_str(line, "reason").unwrap_or("");
            if reason == "illegal" || reason == "timeout" {
                continue;
            }
            let result = if winner.is_empty() {
                0.5
            } else if winner == black {
                1.0
            } else {
                0.0
            };
            let mut pos = Position::startpos();
            for (ply, ms) in moves.split_whitespace().enumerate() {
                let m = match pos.parse_usi_move(ms) {
                    Some(m) => m,
                    None => break,
                };
                let capture = !mv_is_drop(m) && pos.board[mv_to(m)] != EMPTY;
                pos.do_move(m);
                // 駒を取った直後・王手がかかっている局面は静かでないので除外
                if ply + 1 < min_ply || capture || pos.in_check() {
                    continue;
                }
                let mut feats: Vec<(u16, i16)> = Vec::with_capacity(64);
                eval_terms(&pos, |c, idx, n| {
                    let v = if c == BLACK { n } else { -n };
                    feats.push((idx as u16, v as i16));
                });
                out.push(Sample { feats, result });
            }
        }
    }
    out
}

#[inline]
fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

fn predict(params: &[f64], s: &Sample, k: f64) -> f64 {
    let mut e = 0.0;
    for &(i, v) in s.feats.iter() {
        e += params[i as usize] * v as f64;
    }
    sigmoid(k * e)
}

fn loss(params: &[f64], data: &[Sample], k: f64) -> f64 {
    data.iter().map(|s| (s.result - predict(params, s, k)).powi(2)).sum::<f64>() / data.len() as f64
}

pub fn run_tune(args: &[String]) {
    let mut data_paths = Vec::new();
    let mut label_paths = Vec::new();
    let mut lambda = 0.3;
    let mut fixed_k: Option<f64> = None;
    let mut epochs = 300;
    let mut out = "src/params.rs".to_string();
    let mut lr = 0.5;
    let mut l2 = 1e-6;
    let mut min_ply = 16;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--data" => data_paths.push(args[i + 1].clone()),
            "--labels" => label_paths.push(args[i + 1].clone()),
            "--lambda" => lambda = args[i + 1].parse().unwrap(),
            "--k" => fixed_k = Some(args[i + 1].parse().unwrap()),
            "--epochs" => epochs = args[i + 1].parse().unwrap(),
            "--out" => out = args[i + 1].clone(),
            "--lr" => lr = args[i + 1].parse().unwrap(),
            "--l2" => l2 = args[i + 1].parse().unwrap(),
            "--min-ply" => min_ply = args[i + 1].parse().unwrap(),
            _ => {
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    let mut data = load_samples(&data_paths, min_ply);
    data.extend(load_labels(&label_paths, lambda));
    eprintln!("samples: {}", data.len());
    if data.len() < 1000 {
        eprintln!("not enough samples");
        return;
    }
    // 検証用に 1/10 を分ける
    let mut train = Vec::new();
    let mut valid = Vec::new();
    for (i, smp) in data.into_iter().enumerate() {
        if i % 10 == 0 {
            valid.push(smp);
        } else {
            train.push(smp);
        }
    }

    let mut params: Vec<f64> = P.iter().map(|&v| v as f64).collect();
    let init = params.clone();
    // 1. スケール K を探す
    let mut best_k = 0.004;
    let mut best_l = f64::MAX;
    for step in 1..=60 {
        let k = step as f64 * 0.0002;
        let l = loss(&params, &train, k);
        if l < best_l {
            best_l = l;
            best_k = k;
        }
    }
    let k = fixed_k.unwrap_or(best_k);
    eprintln!("K = {:.4}  train loss {:.6}  valid loss {:.6}", k, best_l, loss(&params, &valid, k));

    // 2. Adam で最適化（玉の価値・空マスは固定）
    let fixed = |i: usize| i == IDX_PIECE || i == IDX_PIECE + KING as usize || i == crate::eval::IDX_HAND;
    let mut m = vec![0.0; NUM_PARAMS];
    let mut v = vec![0.0; NUM_PARAMS];
    let (b1, b2, eps) = (0.9, 0.999, 1e-8);
    let mut best_valid = loss(&params, &valid, k);
    let mut best_params = params.clone();
    for epoch in 1..=epochs {
        let mut grad = vec![0.0; NUM_PARAMS];
        let nth = 4;
        let chunk = (train.len() + nth - 1) / nth;
        let parts: Vec<Vec<f64>> = std::thread::scope(|sc| {
            let hs: Vec<_> = train
                .chunks(chunk)
                .map(|ch| {
                    let params = &params;
                    sc.spawn(move || {
                        let mut g = vec![0.0; NUM_PARAMS];
                        for s in ch {
                            let p = predict(params, s, k);
                            let gg = -2.0 * (s.result - p) * p * (1.0 - p) * k;
                            for &(i, x) in s.feats.iter() {
                                g[i as usize] += gg * x as f64;
                            }
                        }
                        g
                    })
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for p in parts {
            for i in 0..NUM_PARAMS {
                grad[i] += p[i];
            }
        }
        let n = train.len() as f64;
        for i in 0..NUM_PARAMS {
            if fixed(i) {
                continue;
            }
            // 初期値（現行パラメータ）への L2 正則化でデータ不足時の暴走を防ぐ
            let g = grad[i] / n + l2 * (params[i] - init[i]);
            m[i] = b1 * m[i] + (1.0 - b1) * g;
            v[i] = b2 * v[i] + (1.0 - b2) * g * g;
            let mh = m[i] / (1.0 - b1.powi(epoch));
            let vh = v[i] / (1.0 - b2.powi(epoch));
            params[i] -= lr * mh / (vh.sqrt() + eps);
        }
        if epoch % 20 == 0 || epoch == epochs {
            let tl = loss(&params, &train, k);
            let vl = loss(&params, &valid, k);
            eprintln!("epoch {:4}  train {:.6}  valid {:.6}", epoch, tl, vl);
            if vl < best_valid {
                best_valid = vl;
                best_params = params.clone();
            }
        }
    }
    write_params(&out, &best_params);
    eprintln!("wrote {} (best valid loss {:.6})", out, best_valid);
}

fn write_params(path: &str, params: &[f64]) {
    let names = [
        ("PIECE_VALUE (盤上, 駒種 0..14)", 0, 15),
        ("HAND_VALUE (0..7)", 15, 8),
        ("DEF_GOLD (距離 0..8)", 23, 9),
        ("DEF_SILVER", 32, 9),
        ("DEF_HORSE", 41, 9),
        ("ATK_MINOR", 50, 9),
        ("ATK_MAJOR", 59, 9),
        ("ATK_PAWN", 68, 9),
        ("KING_RANK (自陣から見た段)", 77, 9),
        ("ATK_KL (香・桂)", 86, 9),
        ("PST [駒種0..14][升81]", 95, 15 * 81),
        ("KP_OWN [駒種0..14][自玉との相対17x17]", 95 + 15 * 81, 15 * 289),
        ("KP_OPP [駒種0..14][敵玉との相対17x17]", 95 + 15 * 81 + 15 * 289, 15 * 289),
    ];
    let mut s = format!(
        "//! 評価関数のパラメータ。`shogi-rsi tune` で自動生成・上書きされる。\n//! レイアウトは eval.rs の IDX_* を参照。\n\npub const P: [i32; {}] = [\n",
        params.len()
    );
    for (name, start, len) in names.iter() {
        s.push_str(&format!("    // {}\n    ", name));
        for (k, v) in params[*start..start + len].iter().enumerate() {
            if k > 0 && k % 17 == 0 {
                s.push_str("\n    ");
            }
            s.push_str(&format!("{}, ", v.round() as i32));
        }
        s.push_str("\n");
    }
    s.push_str("];\n");
    std::fs::write(path, s).expect("write params");
}
