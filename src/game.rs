//! Shared validated game API for native and browser engines.
use crate::position::*;
use crate::search::{Limits, Searcher};

fn outcome(pos: &mut Position, plies: usize) -> String {
    if !pos.has_legal_move() {
        return format!(
            "{}の勝ち（詰み）",
            if pos.side == BLACK {
                "後手"
            } else {
                "先手"
            }
        );
    }
    if pos.repetition_count() >= 3 {
        return "千日手で引き分け".into();
    }
    if plies >= 320 {
        return "320手で引き分け".into();
    }
    String::new()
}

// Body: human side, thinking milliseconds, state/play, then USI moves.
pub fn state(body: &str) -> Result<String, &'static str> {
    let mut words = body.split_whitespace();
    let human: usize = words
        .next()
        .and_then(|v| v.parse().ok())
        .filter(|&v| v < 2)
        .ok_or("Invalid side")?;
    let ms: u64 = words
        .next()
        .and_then(|v| v.parse().ok())
        .filter(|&v| (100..=5000).contains(&v))
        .ok_or("Invalid time")?;
    let action = words.next().ok_or("Missing action")?;
    if action != "state" && action != "play" {
        return Err("Invalid action");
    }
    let mut moves: Vec<String> = words.map(str::to_owned).collect();
    if moves.len() > 320 {
        return Err("Too many moves");
    }
    let mut pos = Position::startpos();
    for (i, text) in moves.iter().enumerate() {
        if !outcome(&mut pos, i).is_empty() {
            return Err("Game already ended");
        }
        let m = pos.parse_legal_usi_move(text).ok_or("Illegal move")?;
        // The engine parser tolerates suffixes; the web API accepts canonical USI only.
        if move_to_usi(m) != *text {
            return Err("Invalid move notation");
        }
        pos.do_move(m);
    }
    let mut result = outcome(&mut pos, moves.len());
    if action == "play" && pos.side != human && result.is_empty() {
        let mut searcher = Searcher::new(32);
        searcher.verbose = false;
        let r = searcher.search(
            &mut pos,
            &Limits {
                time_ms: Some(ms),
                depth: None,
                nodes: None,
            },
        );
        if r.best == NO_MOVE {
            result = format!(
                "{}の勝ち（AI投了）",
                if human == BLACK { "先手" } else { "後手" }
            );
        } else {
            if !pos.legal_moves().contains(&r.best) {
                return Err("Engine returned illegal move");
            }
            moves.push(move_to_usi(r.best));
            pos.do_move(r.best);
            result = outcome(&mut pos, moves.len());
        }
    }
    let legal = if result.is_empty() {
        pos.legal_moves()
    } else {
        Vec::new()
    };
    let strings = |v: Vec<String>| {
        v.into_iter()
            .map(|s| format!("\"{}\"", s))
            .collect::<Vec<_>>()
            .join(",")
    };
    Ok(format!("{{\"board\":{:?},\"hands\":{:?},\"side\":{},\"check\":{},\"moves\":[{}],\"legal\":[{}],\"result\":\"{}\"}}",
        pos.board, pos.hand, pos.side, pos.in_check(), strings(moves), strings(legal.into_iter().map(move_to_usi).collect()), result))
}
