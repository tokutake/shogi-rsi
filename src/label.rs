//! 教師データ作成: 対局記録から静かな局面を抽出し、探索（固定深さ）の評価値を付けて保存する。
//!
//!   shogi-rsi label --data results/games.jsonl [--data ...] --out results/labels.txt --depth 5 --stride 2 --threads 4
//!
//! 出力は 1 行 1 局面: `sfen|手番側から見た探索評価値|先手から見た勝敗(1/0.5/0)`。tune が読み込む。

use crate::position::*;
use crate::search::{Limits, Searcher};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub fn json_str<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{}\":\"", key);
    let i = line.find(&pat)? + pat.len();
    let j = line[i..].find('"')? + i;
    Some(&line[i..j])
}

pub fn run_label(args: &[String]) {
    let mut data = Vec::new();
    let mut out = "results/labels.txt".to_string();
    let mut depth = 5;
    let mut stride = 2;
    let mut threads = 4;
    let mut offset = 0;
    let mut i = 0;
    while i + 1 < args.len() {
        match args[i].as_str() {
            "--data" => data.push(args[i + 1].clone()),
            "--out" => out = args[i + 1].clone(),
            "--depth" => depth = args[i + 1].parse().unwrap(),
            "--stride" => stride = args[i + 1].parse().unwrap(),
            "--threads" => threads = args[i + 1].parse().unwrap(),
            "--offset" => offset = args[i + 1].parse().unwrap(),
            _ => {}
        }
        i += 2;
    }
    let mut items: Vec<(String, f64)> = Vec::new();
    let mut counter = 0usize;
    for path in &data {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => continue,
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
            let result = if winner.is_empty() { 0.5 } else if winner == black { 1.0 } else { 0.0 };
            let mut pos = Position::startpos();
            for (ply, ms) in moves.split_whitespace().enumerate() {
                let Some(m) = pos.parse_usi_move(ms) else { break };
                let capture = !mv_is_drop(m) && pos.board[mv_to(m)] != EMPTY;
                pos.do_move(m);
                if ply + 1 < 8 || capture || pos.in_check() {
                    continue;
                }
                counter += 1;
                if (counter + offset) % stride == 0 {
                    items.push((pos.to_sfen(), result));
                }
            }
        }
    }
    // 途中で打ち切っても偏らないよう、順序をシャッフルする
    let mut rs = 0x9E3779B97F4A7C15u64;
    for i in (1..items.len()).rev() {
        rs ^= rs << 13;
        rs ^= rs >> 7;
        rs ^= rs << 17;
        items.swap(i, (rs % (i as u64 + 1)) as usize);
    }
    eprintln!("positions to label: {}", items.len());
    let next = AtomicUsize::new(0);
    let file = Mutex::new(std::io::BufWriter::new(std::fs::File::create(&out).expect("create out")));
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| {
                let mut s = Searcher::new(32);
                s.verbose = false;
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= items.len() {
                        break;
                    }
                    let (sfen, result) = &items[k];
                    let mut pos = Position::from_sfen(sfen).unwrap();
                    let r = s.search(&mut pos, &Limits { time_ms: None, depth: Some(depth), nodes: None });
                    if r.depth == 0 {
                        continue;
                    }
                    let mut f = file.lock().unwrap();
                    writeln!(f, "{}|{}|{}", sfen, r.score, result).unwrap();
                    if k % 20000 == 0 {
                        eprintln!("{}/{}", k, items.len());
                    }
                }
            });
        }
    });
    file.lock().unwrap().flush().unwrap();
}
