//! 教師局面の生成: 固定深さの自己対局を指しながら、各局面の探索評価値をそのまま教師として書き出す。
//! label（対局記録 → 再探索）と違い、対局と評価付けを 1 回の探索で済ませる。
//!
//!   shogi-rsi gensfen --out results/gensfen.txt --positions 1000000 --depth 5 --threads 4 --seed 1
//!
//! 出力形式は label と同じ `sfen|手番側から見た探索評価値|先手から見た勝敗(1/0.5/0)`。

use super::position::*;
use super::search::{Limits, Searcher, MATE};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const ADJUDICATE: i32 = 3000;
const MAX_PLIES: usize = 320;

pub fn run_gensfen(args: &[String]) {
    let mut out = "results/gensfen.txt".to_string();
    let mut positions = 100_000usize;
    let mut depth = 5;
    let mut threads = 4;
    let mut seed = 1u64;
    let mut random_plies = 12usize;
    let mut i = 0;
    while i + 1 < args.len() {
        match args[i].as_str() {
            "--out" => out = args[i + 1].clone(),
            "--positions" => positions = args[i + 1].parse().unwrap(),
            "--depth" => depth = args[i + 1].parse().unwrap(),
            "--threads" => threads = args[i + 1].parse().unwrap(),
            "--seed" => seed = args[i + 1].parse().unwrap(),
            "--random-plies" => random_plies = args[i + 1].parse().unwrap(),
            _ => {}
        }
        i += 2;
    }
    let written = AtomicUsize::new(0);
    let file = Mutex::new(std::io::BufWriter::new(
        std::fs::OpenOptions::new().create(true).append(true).open(&out).expect("open out"),
    ));
    std::thread::scope(|sc| {
        for t in 0..threads {
            let (written, file) = (&written, &file);
            sc.spawn(move || {
                let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) ^ (t as u64 + 1).wrapping_mul(0xD1B54A32D192ED03) | 1);
                let mut s = Searcher::new(16);
                s.verbose = false;
                while written.load(Ordering::Relaxed) < positions {
                    let (recs, result) = play_game(&mut s, &mut rng, depth, random_plies);
                    let mut f = file.lock().unwrap();
                    for (sfen, score) in recs.iter() {
                        writeln!(f, "{}|{}|{}", sfen, score, result).unwrap();
                    }
                    let before = written.fetch_add(recs.len(), Ordering::Relaxed);
                    if (before + recs.len()) / 20000 != before / 20000 {
                        eprintln!("{}/{}", before + recs.len(), positions);
                    }
                }
            });
        }
    });
    file.lock().unwrap().flush().unwrap();
}

/// 1 局指して (記録した局面と評価値, 先手から見た勝敗) を返す
fn play_game(s: &mut Searcher, rng: &mut Rng, depth: i32, random_plies: usize) -> (Vec<(String, i32)>, f64) {
    let mut pos = Position::startpos();
    s.tt.clear();
    // 序盤は駒を取らないランダムな手で局面を散らす（手数もランダム）
    let n_random = random_plies / 2 + rng.below(random_plies / 2 + 1);
    for _ in 0..n_random {
        let moves: Vec<Move> = pos
            .legal_moves()
            .into_iter()
            .filter(|&m| mv_is_drop(m) || pos.board[mv_to(m)] == EMPTY)
            .collect();
        if moves.is_empty() {
            break;
        }
        pos.do_move(moves[rng.below(moves.len())]);
    }
    let mut recs = Vec::new();
    let mut last_capture = false;
    let mut adjud = 0;
    loop {
        if pos.ply >= MAX_PLIES || pos.repetition_count() >= 3 {
            return (recs, 0.5);
        }
        if !pos.has_legal_move() {
            // 手番側の負け
            return (recs, if pos.side == BLACK { 0.0 } else { 1.0 });
        }
        let r = s.search(&mut pos, &Limits { time_ms: None, depth: Some(depth), nodes: None });
        let score = r.score;
        if !last_capture && !pos.in_check() && score.abs() < MATE - 1000 {
            recs.push((pos.to_sfen(), score.clamp(-ADJUDICATE, ADJUDICATE)));
        }
        // 大差が続いたら打ち切って勝敗を決める
        if score.abs() >= ADJUDICATE {
            adjud += 1;
            if adjud >= 4 || score.abs() >= MATE - 1000 {
                let us_wins = score > 0;
                let black_wins = us_wins == (pos.side == BLACK);
                return (recs, if black_wins { 1.0 } else { 0.0 });
            }
        } else {
            adjud = 0;
        }
        let m = r.best;
        last_capture = !mv_is_drop(m) && pos.board[mv_to(m)] != EMPTY;
        pos.do_move(m);
    }
}
