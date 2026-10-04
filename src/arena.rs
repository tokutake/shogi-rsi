//! 対局アリーナ: 2つの USI エンジンを指定局数対局させ、結果を JSON Lines で出力する。
//! 審判（合法手判定・詰み・千日手・手数制限）は本クレートの Position を使う。

use crate::position::*;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct EngineSpec {
    pub name: String,
    pub path: String,
}

struct Engine {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<String>,
}

impl Engine {
    fn spawn(path: &str) -> std::io::Result<Engine> {
        let mut child = Command::new(path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        let mut e = Engine { child, stdin, rx };
        e.send("usi");
        e.wait_for("usiok", 10000)?;
        e.send("setoption name USI_Hash value 32");
        e.send("isready");
        e.wait_for("readyok", 10000)?;
        Ok(e)
    }
    fn send(&mut self, s: &str) {
        let _ = writeln!(self.stdin, "{}", s);
        let _ = self.stdin.flush();
    }
    fn wait_for(&mut self, prefix: &str, timeout_ms: u64) -> std::io::Result<String> {
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let now = std::time::Instant::now();
            if now >= deadline {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
            }
            match self.rx.recv_timeout(deadline - now) {
                Ok(l) if l.starts_with(prefix) => return Ok(l),
                Ok(_) => continue,
                Err(_) => return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout")),
            }
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.send("quit");
        std::thread::sleep(Duration::from_millis(20));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct ArenaConfig {
    pub engines: [EngineSpec; 2],
    pub games: usize,
    pub byoyomi_ms: u64,
    pub concurrency: usize,
    pub seed: u64,
    pub max_plies: usize,
    pub out: Option<String>,
    pub opening_plies: usize,
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// ランダムな序盤局面（駒を取らない手だけで opening_plies 手進める）
fn make_opening(seed: u64, plies: usize) -> Vec<String> {
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let mut pos = Position::startpos();
    let mut moves = Vec::new();
    for _ in 0..plies {
        let legal: Vec<Move> = pos
            .legal_moves()
            .into_iter()
            .filter(|&m| !mv_is_drop(m) && pos.board[mv_to(m)] == EMPTY && ptype(pos.board[mv_from(m)]) != KING)
            .collect();
        if legal.is_empty() {
            break;
        }
        let m = legal[(rng.next() % legal.len() as u64) as usize];
        moves.push(move_to_usi(m));
        pos.do_move(m);
    }
    moves
}

pub struct GameResult {
    pub black: String,
    pub white: String,
    pub winner: Option<usize>, // Some(BLACK/WHITE) or None (draw)
    pub reason: String,
    pub plies: usize,
    pub opening: Vec<String>,
    pub moves: Vec<String>,
}

fn play_game(
    engines: &mut [Engine; 2],
    names: [&str; 2],
    black_idx: usize,
    opening: &[String],
    byoyomi: u64,
    max_plies: usize,
) -> GameResult {
    let mut pos = Position::startpos();
    let mut moves: Vec<String> = Vec::new();
    for m in opening {
        let mv = pos.parse_legal_usi_move(m).expect("bad opening");
        pos.do_move(mv);
        moves.push(m.clone());
    }
    for e in engines.iter_mut() {
        e.send("usinewgame");
    }
    let result = |winner: Option<usize>, reason: &str, moves: &Vec<String>| GameResult {
        black: names[black_idx].to_string(),
        white: names[black_idx ^ 1].to_string(),
        winner,
        reason: reason.to_string(),
        plies: moves.len(),
        opening: opening.to_vec(),
        moves: moves.clone(),
    };
    loop {
        let side = pos.side;
        let eidx = if side == BLACK { black_idx } else { black_idx ^ 1 };
        let legal = pos.legal_moves();
        if legal.is_empty() {
            return result(Some(side ^ 1), "mate", &moves);
        }
        if moves.len() >= max_plies {
            return result(None, "max_plies", &moves);
        }
        let e = &mut engines[eidx];
        e.send(&format!("position startpos moves {}", moves.join(" ")));
        e.send(&format!("go btime 0 wtime 0 byoyomi {}", byoyomi));
        let line = match e.wait_for("bestmove", byoyomi + 2000) {
            Ok(l) => l,
            Err(_) => return result(Some(side ^ 1), "timeout", &moves),
        };
        let mstr = line.split_whitespace().nth(1).unwrap_or("resign").to_string();
        if mstr == "resign" || mstr == "win" {
            return result(Some(side ^ 1), "resign", &moves);
        }
        let mv = match pos.parse_usi_move(&mstr) {
            Some(m) if legal.contains(&m) => m,
            _ => return result(Some(side ^ 1), "illegal", &moves),
        };
        pos.do_move(mv);
        moves.push(mstr);
        if pos.repetition_count() >= 3 {
            return result(None, "repetition", &moves);
        }
    }
}

pub fn run_arena(cfg: ArenaConfig) {
    let total = cfg.games;
    let next = Arc::new(Mutex::new(0usize));
    let results: Arc<Mutex<Vec<GameResult>>> = Arc::new(Mutex::new(Vec::new()));
    let out_file = cfg.out.as_ref().map(|p| {
        Arc::new(Mutex::new(
            std::fs::OpenOptions::new().create(true).append(true).open(p).expect("cannot open out file"),
        ))
    });
    let cfg = Arc::new(cfg);
    let mut handles = Vec::new();
    for _ in 0..cfg.concurrency.max(1) {
        let next = next.clone();
        let results = results.clone();
        let cfg = cfg.clone();
        let out_file = out_file.clone();
        handles.push(std::thread::spawn(move || {
            let mut engines = [
                Engine::spawn(&cfg.engines[0].path).expect("spawn engine 0"),
                Engine::spawn(&cfg.engines[1].path).expect("spawn engine 1"),
            ];
            let names = [cfg.engines[0].name.as_str(), cfg.engines[1].name.as_str()];
            loop {
                let g = {
                    let mut n = next.lock().unwrap();
                    if *n >= total {
                        break;
                    }
                    *n += 1;
                    *n - 1
                };
                let opening = make_opening(cfg.seed.wrapping_add((g / 2) as u64), cfg.opening_plies);
                let black_idx = g % 2;
                let r = play_game(&mut engines, names, black_idx, &opening, cfg.byoyomi_ms, cfg.max_plies);
                let winner_name = match r.winner {
                    Some(c) => {
                        if c == BLACK {
                            r.black.clone()
                        } else {
                            r.white.clone()
                        }
                    }
                    None => String::new(),
                };
                let json = format!(
                    "{{\"black\":\"{}\",\"white\":\"{}\",\"winner\":\"{}\",\"reason\":\"{}\",\"plies\":{},\"byoyomi\":{},\"opening\":\"{}\",\"moves\":\"{}\"}}",
                    r.black,
                    r.white,
                    winner_name,
                    r.reason,
                    r.plies,
                    cfg.byoyomi_ms,
                    r.opening.join(" "),
                    r.moves.join(" ")
                );
                if let Some(f) = &out_file {
                    let mut f = f.lock().unwrap();
                    writeln!(f, "{}", json).ok();
                }
                eprintln!("game {:>4}: {}", g + 1, json);
                results.lock().unwrap().push(r);
            }
        }));
    }
    for h in handles {
        h.join().ok();
    }
    let results = results.lock().unwrap();
    let a = &cfg.engines[0].name;
    let (mut w, mut l, mut d) = (0, 0, 0);
    for r in results.iter() {
        match r.winner {
            None => d += 1,
            Some(c) => {
                let wn = if c == BLACK { &r.black } else { &r.white };
                if wn == a {
                    w += 1
                } else {
                    l += 1
                }
            }
        }
    }
    let n = (w + l + d) as f64;
    let score = (w as f64 + 0.5 * d as f64) / n.max(1.0);
    let elo = if score <= 0.0 {
        -999.0
    } else if score >= 1.0 {
        999.0
    } else {
        -400.0 * (1.0 / score - 1.0).log10()
    };
    println!(
        "{} vs {}: +{} -{} ={}  score {:.3}  elo diff {:+.0}",
        a, cfg.engines[1].name, w, l, d, score, elo
    );
}
