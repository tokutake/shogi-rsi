//! USI プロトコル。

use crate::position::*;
use crate::search::{Limits, Searcher};
use std::io::{self, BufRead, Write};

pub const ENGINE_NAME: &str = "shogi-rsi";

pub fn usi_loop() {
    let stdin = io::stdin();
    let mut pos = Position::startpos();
    let mut hash_mb = 64usize;
    let mut searcher: Option<Searcher> = None;
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }
        match tokens[0] {
            "usi" => {
                println!("id name {} {}", ENGINE_NAME, option_env!("RSI_VERSION_NAME").unwrap_or("dev"));
                println!("id author shogi-rsi project");
                println!("option name USI_Hash type spin default 64 min 1 max 4096");
                println!("usiok");
            }
            "setoption" => {
                // setoption name X value Y
                if let (Some(ni), Some(vi)) = (
                    tokens.iter().position(|&t| t == "name"),
                    tokens.iter().position(|&t| t == "value"),
                ) {
                    if ni + 1 < tokens.len() && vi + 1 < tokens.len() && tokens[ni + 1] == "USI_Hash" {
                        hash_mb = tokens[vi + 1].parse().unwrap_or(64);
                        searcher = None;
                    }
                }
            }
            "isready" => {
                if searcher.is_none() {
                    searcher = Some(Searcher::new(hash_mb));
                }
                println!("readyok");
            }
            "usinewgame" => {
                if let Some(s) = searcher.as_mut() {
                    s.tt.clear();
                }
            }
            "position" => {
                if let Some(p) = parse_position(&tokens) {
                    pos = p;
                }
            }
            "go" => {
                let s = searcher.get_or_insert_with(|| Searcher::new(hash_mb));
                let limits = parse_go(&tokens, pos.side);
                let r = s.search(&mut pos, &limits);
                if r.best == NO_MOVE {
                    println!("bestmove resign");
                } else {
                    println!("bestmove {}", move_to_usi(r.best));
                }
            }
            "d" => {
                println!("{}", pos.to_sfen());
            }
            "quit" => break,
            _ => {}
        }
        io::stdout().flush().ok();
    }
}

pub fn parse_position(tokens: &[&str]) -> Option<Position> {
    let mut pos;
    let i;
    if tokens.get(1) == Some(&"startpos") {
        pos = Position::startpos();
        i = 2;
    } else if tokens.get(1) == Some(&"sfen") {
        let end = tokens.iter().position(|&t| t == "moves").unwrap_or(tokens.len());
        pos = Position::from_sfen(&tokens[2..end].join(" "))?;
        i = end;
    } else {
        return None;
    }
    if tokens.get(i) == Some(&"moves") {
        for t in &tokens[i + 1..] {
            let m = pos.parse_usi_move(t)?;
            pos.do_move(m);
        }
    }
    Some(pos)
}

fn parse_go(tokens: &[&str], side: Color) -> Limits {
    let get = |name: &str| -> Option<u64> {
        tokens.iter().position(|&t| t == name).and_then(|i| tokens.get(i + 1)).and_then(|v| v.parse().ok())
    };
    let mut limits = Limits { time_ms: None, depth: None, nodes: None };
    if let Some(d) = get("depth") {
        limits.depth = Some(d as i32);
    }
    if let Some(n) = get("nodes") {
        limits.nodes = Some(n);
    }
    if let Some(mt) = get("movetime") {
        limits.time_ms = Some(mt);
    } else {
        let my_time = if side == BLACK { get("btime") } else { get("wtime") }.unwrap_or(0);
        let inc = if side == BLACK { get("binc") } else { get("winc") }.unwrap_or(0);
        let byoyomi = get("byoyomi").unwrap_or(0);
        if my_time > 0 || byoyomi > 0 || inc > 0 {
            let budget = my_time / 40 + byoyomi + inc;
            let margin = 30.min(budget / 4);
            limits.time_ms = Some(budget.saturating_sub(margin).max(1));
        }
    }
    if tokens.contains(&"infinite") {
        limits.time_ms = None;
    }
    if limits.time_ms.is_none() && limits.depth.is_none() && limits.nodes.is_none() && !tokens.contains(&"infinite") {
        limits.depth = Some(6);
    }
    limits
}
