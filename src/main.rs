#![allow(dead_code)]
mod arena;
mod eval;
mod position;
mod search;
mod usi;

use position::*;

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("perft") => {
            let d: u32 = args[2].parse().unwrap();
            let mut pos = if args.len() > 3 {
                Position::from_sfen(&args[3..].join(" ")).unwrap()
            } else {
                Position::startpos()
            };
            let t = std::time::Instant::now();
            let n = pos.perft(d);
            println!("perft {} = {} ({:.2}s)", d, n, t.elapsed().as_secs_f64());
        }
        Some("bench") => {
            // 固定局面群を固定深さで探索し、ノード数と NPS を出す（高速化の計測用）
            let depth: i32 = arg_value(&args, "--depth").and_then(|v| v.parse().ok()).unwrap_or(6);
            let positions = [
                STARTPOS_SFEN,
                "l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w RGgsn5p 1",
                "lnsgk2nl/1r4gs1/p1pppp1pp/1p4p2/7P1/2P6/PP1PPPP1P/1SG4R1/LN2KGSNL b Bb 1",
                "ln1g3nl/1r1sg1k2/p1pppsbpp/6p2/1p5P1/2P6/PPSPPPP1P/1B2K2R1/LN1G1GSNL b - 1",
                "l2g3nl/2s2gk2/2npp1sp1/p1p1bpp1p/1r5P1/P1PPP1P1P/1PB2PS2/2G1G1K2/LNS4RL b NP 1",
            ];
            let mut s = search::Searcher::new(64);
            s.verbose = false;
            let mut total = 0u64;
            let t = std::time::Instant::now();
            for sfen in positions.iter() {
                let mut pos = Position::from_sfen(sfen).unwrap();
                s.tt.clear();
                let r = s.search(&mut pos, &search::Limits { time_ms: None, depth: Some(depth), nodes: None });
                total += r.nodes;
                println!("{:<80} best {} score {}", sfen, move_to_usi(r.best), r.score);
            }
            let secs = t.elapsed().as_secs_f64();
            println!("bench: nodes {} time {:.2}s nps {:.0}", total, secs, total as f64 / secs);
        }
        Some("arena") => {
            let e1 = arg_value(&args, "--engine1").expect("--engine1 path");
            let e2 = arg_value(&args, "--engine2").expect("--engine2 path");
            let cfg = arena::ArenaConfig {
                engines: [
                    arena::EngineSpec { name: arg_value(&args, "--name1").unwrap_or("engine1".into()), path: e1 },
                    arena::EngineSpec { name: arg_value(&args, "--name2").unwrap_or("engine2".into()), path: e2 },
                ],
                games: arg_value(&args, "--games").and_then(|v| v.parse().ok()).unwrap_or(20),
                byoyomi_ms: arg_value(&args, "--byoyomi").and_then(|v| v.parse().ok()).unwrap_or(100),
                concurrency: arg_value(&args, "--concurrency").and_then(|v| v.parse().ok()).unwrap_or(2),
                seed: arg_value(&args, "--seed").and_then(|v| v.parse().ok()).unwrap_or(1),
                max_plies: arg_value(&args, "--max-plies").and_then(|v| v.parse().ok()).unwrap_or(320),
                out: arg_value(&args, "--out"),
                opening_plies: arg_value(&args, "--opening-plies").and_then(|v| v.parse().ok()).unwrap_or(6),
            };
            arena::run_arena(cfg);
        }
        _ => usi::usi_loop(),
    }
}
