//! 探索部: 反復深化 + αβ(negamax) + 静止探索 + 置換表 + キラー/MVV-LVA。

use crate::eval::{evaluate, PIECE_VALUE};
use crate::position::*;
use std::time::Instant;

pub const INF: i32 = 32000;
pub const MATE: i32 = 31000;
pub const MAX_PLY: usize = 128;

#[derive(Clone, Copy, Default)]
struct TTEntry {
    key: u64,
    mv: Move,
    score: i32,
    depth: i8,
    flag: u8, // 0 none, 1 exact, 2 lower, 3 upper
}

pub struct TT {
    table: Vec<TTEntry>,
    mask: usize,
}

impl TT {
    pub fn new(mb: usize) -> Self {
        let n = (mb * 1024 * 1024 / std::mem::size_of::<TTEntry>()).next_power_of_two() / 2;
        TT { table: vec![TTEntry::default(); n.max(1024)], mask: n.max(1024) - 1 }
    }
    pub fn clear(&mut self) {
        for e in self.table.iter_mut() {
            *e = TTEntry::default();
        }
    }
    #[inline]
    fn probe(&self, key: u64) -> Option<TTEntry> {
        let e = self.table[(key as usize) & self.mask];
        if e.flag != 0 && e.key == key {
            Some(e)
        } else {
            None
        }
    }
    #[inline]
    fn store(&mut self, key: u64, mv: Move, score: i32, depth: i32, flag: u8) {
        let e = &mut self.table[(key as usize) & self.mask];
        if e.key != key || depth as i8 >= e.depth || flag == 1 {
            *e = TTEntry { key, mv, score, depth: depth as i8, flag };
        }
    }
}

pub struct Limits {
    pub time_ms: Option<u64>,
    pub depth: Option<i32>,
    pub nodes: Option<u64>,
}

pub struct SearchResult {
    pub best: Move,
    pub score: i32,
    pub depth: i32,
    pub nodes: u64,
}

pub struct Searcher {
    pub tt: TT,
    nodes: u64,
    start: Instant,
    time_ms: Option<u64>,
    node_limit: Option<u64>,
    stopped: bool,
    killers: [[Move; 2]; MAX_PLY + 1],
    history: Vec<i32>, // [piece(32)][to(81)]
    pub verbose: bool,
    scratch: Vec<(i32, Move)>,
    move_bufs: Vec<Vec<Move>>,
    quiet_bufs: Vec<Vec<Move>>,
}

#[inline]
fn score_to_tt(s: i32, ply: usize) -> i32 {
    if s > MATE - 1000 {
        s + ply as i32
    } else if s < -MATE + 1000 {
        s - ply as i32
    } else {
        s
    }
}
#[inline]
fn score_from_tt(s: i32, ply: usize) -> i32 {
    if s > MATE - 1000 {
        s - ply as i32
    } else if s < -MATE + 1000 {
        s + ply as i32
    } else {
        s
    }
}

impl Searcher {
    pub fn new(tt_mb: usize) -> Self {
        Searcher {
            tt: TT::new(tt_mb),
            nodes: 0,
            start: Instant::now(),
            time_ms: None,
            node_limit: None,
            stopped: false,
            killers: [[NO_MOVE; 2]; MAX_PLY + 1],
            history: vec![0; 32 * 81],
            verbose: true,
            scratch: Vec::with_capacity(256),
            move_bufs: vec![Vec::with_capacity(160); MAX_PLY + 2],
            quiet_bufs: vec![Vec::with_capacity(64); MAX_PLY + 2],
        }
    }

    #[inline]
    fn check_stop(&mut self) {
        if self.nodes & 1023 == 0 {
            if let Some(t) = self.time_ms {
                if self.start.elapsed().as_millis() as u64 >= t {
                    self.stopped = true;
                }
            }
        }
        if let Some(n) = self.node_limit {
            if self.nodes >= n {
                self.stopped = true;
            }
        }
    }

    pub fn search(&mut self, pos: &mut Position, limits: &Limits) -> SearchResult {
        self.start = Instant::now();
        self.nodes = 0;
        self.stopped = false;
        self.time_ms = limits.time_ms;
        self.node_limit = limits.nodes;
        self.killers = [[NO_MOVE; 2]; MAX_PLY + 1];
        for h in self.history.iter_mut() {
            *h /= 8;
        }
        let max_depth = limits.depth.unwrap_or(64).min(MAX_PLY as i32 - 10);

        let legal = pos.legal_moves();
        let mut best = legal.first().copied().unwrap_or(NO_MOVE);
        let mut best_score = -INF;
        let mut done_depth = 0;
        if legal.len() <= 1 {
            return SearchResult { best, score: 0, depth: 0, nodes: 0 };
        }
        for depth in 1..=max_depth {
            // Aspiration window: 前回の評価値の周辺の窓で探索し、外れたら窓を広げて再探索
            let mut delta = 60;
            let (mut lo, mut hi) = if depth >= 4 && best_score.abs() < MATE - 1000 {
                (best_score - delta, best_score + delta)
            } else {
                (-INF, INF)
            };
            let mut score;
            loop {
                score = self.negamax(pos, depth, 0, lo, hi, true);
                if self.stopped {
                    break;
                }
                if score <= lo {
                    lo = (lo - delta).max(-INF);
                } else if score >= hi {
                    hi = (hi + delta).min(INF);
                } else {
                    break;
                }
                delta *= 2;
            }
            if self.stopped && depth > 1 {
                break;
            }
            if let Some(e) = self.tt.probe(pos.key()) {
                if e.mv != NO_MOVE {
                    best = e.mv;
                }
            }
            best_score = score;
            done_depth = depth;
            if self.verbose {
                let ms = self.start.elapsed().as_millis() as u64;
                let sc = if score.abs() > MATE - 1000 {
                    let plies = MATE - score.abs();
                    format!("mate {}", if score > 0 { plies } else { -plies })
                } else {
                    format!("cp {}", score)
                };
                println!(
                    "info depth {} score {} nodes {} nps {} time {} pv {}",
                    depth,
                    sc,
                    self.nodes,
                    self.nodes * 1000 / ms.max(1),
                    ms,
                    move_to_usi(best)
                );
            }
            if score.abs() > MATE - 1000 {
                break;
            }
            // 次の反復が終わりそうになければ打ち切る
            if let Some(t) = self.time_ms {
                if self.start.elapsed().as_millis() as u64 * 2 > t {
                    break;
                }
            }
        }
        SearchResult { best, score: best_score, depth: done_depth, nodes: self.nodes }
    }

    fn order_moves(&mut self, pos: &Position, moves: &mut [Move], tt_move: Move, ply: usize) {
        let mut scored = std::mem::take(&mut self.scratch);
        scored.clear();
        scored.extend(moves.iter().map(|&m| {
                let s = if m == tt_move {
                    1_000_000
                } else if !mv_is_drop(m) && pos.board[mv_to(m)] != EMPTY {
                    let victim = PIECE_VALUE[ptype(pos.board[mv_to(m)]) as usize];
                    let attacker = PIECE_VALUE[ptype(pos.board[mv_from(m)]) as usize];
                    if attacker > victim + 50 && pos.is_attacked(mv_to(m), pos.side ^ 1) {
                        // 取り返される損な取りは静かな手の後ろに回す
                        60_000 + victim - attacker / 10
                    } else {
                        100_000 + victim * 10 - attacker / 10
                    }
                } else if mv_is_promo(m) {
                    90_000
                } else if ply <= MAX_PLY && (self.killers[ply][0] == m || self.killers[ply][1] == m) {
                    80_000
                } else {
                    self.history[Self::hist_idx(pos, m)].min(70_000)
                };
                (s, m)
            }));
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        for (i, &(_, m)) in scored.iter().enumerate() {
            moves[i] = m;
        }
        self.scratch = scored;
    }

    #[inline]
    fn hist_idx(pos: &Position, m: Move) -> usize {
        let p = if mv_is_drop(m) { make_piece(pos.side, mv_drop_pt(m)) } else { pos.board[mv_from(m)] };
        p as usize * 81 + mv_to(m)
    }

    /// 疑似合法手を指して、合法なら true（不合法なら戻して false）
    #[inline]
    fn try_move(&mut self, pos: &mut Position, m: Move) -> bool {
        if mv_is_drop(m) && mv_drop_pt(m) == PAWN {
            if !pos.is_legal(m) {
                return false;
            }
            pos.do_move(m);
            return true;
        }
        let us = pos.side;
        pos.do_move(m);
        if pos.is_attacked(pos.king_sq[us], us ^ 1) {
            pos.undo_move();
            return false;
        }
        true
    }

    fn negamax(&mut self, pos: &mut Position, depth: i32, ply: usize, mut alpha: i32, beta: i32, pv: bool) -> i32 {
        if depth <= 0 {
            return self.qsearch(pos, ply, alpha, beta, 0);
        }
        self.nodes += 1;
        self.check_stop();
        if self.stopped {
            return 0;
        }
        if ply > 0 && pos.repetition_count() >= 1 {
            return 0;
        }
        if ply >= MAX_PLY - 1 {
            return evaluate(pos);
        }
        let key = pos.key();
        let mut tt_move = NO_MOVE;
        if let Some(e) = self.tt.probe(key) {
            tt_move = e.mv;
            if !pv && ply > 0 && e.depth as i32 >= depth {
                let s = score_from_tt(e.score, ply);
                match e.flag {
                    1 => return s,
                    2 if s >= beta => return s,
                    3 if s <= alpha => return s,
                    _ => {}
                }
            }
        }
        let in_check = pos.in_check();
        let mut depth = if in_check { depth + 1 } else { depth };
        // Internal iterative reduction: TT 手がない深いノードは 1 浅く読む
        if tt_move == NO_MOVE && depth >= 4 && !in_check {
            depth -= 1;
        }

        let static_eval = if in_check { -INF } else { evaluate(pos) };

        // Reverse futility pruning（静的評価が beta を大きく上回るなら打ち切り）
        if !pv && !in_check && depth <= 3 && ply > 0 && beta.abs() < MATE - 1000 && static_eval - 150 * depth >= beta {
            return static_eval;
        }

        // Null move pruning
        if !pv && !in_check && depth >= 3 && ply > 0 && beta.abs() < MATE - 1000 && pos.last_move() != NO_MOVE {
            if static_eval >= beta {
                let r = 2 + depth / 4;
                pos.do_null_move();
                let score = -self.negamax(pos, depth - 1 - r, ply + 1, -beta, -beta + 1, false);
                pos.undo_null_move();
                if self.stopped {
                    return 0;
                }
                if score >= beta {
                    return if score > MATE - 1000 { beta } else { score };
                }
            }
        }

        let mut moves = std::mem::take(&mut self.move_bufs[ply]);
        moves.clear();
        pos.pseudo_moves(&mut moves, false);
        self.order_moves(pos, &mut moves, tt_move, ply);

        let orig_alpha = alpha;
        let mut best_score = -INF;
        let mut best_move = NO_MOVE;
        let mut legal = 0;
        let mut quiets_tried = std::mem::take(&mut self.quiet_bufs[ply]);
        quiets_tried.clear();
        for &m in moves.iter() {
            let quiet = !mv_is_promo(m) && (mv_is_drop(m) || pos.board[mv_to(m)] == EMPTY);
            let hidx = Self::hist_idx(pos, m);
            if !self.try_move(pos, m) {
                continue;
            }
            legal += 1;
            let gives_check = pos.in_check();
            // Late move pruning: 浅い深さで後半の静かな手は読まない
            if !pv && !in_check && !gives_check && quiet && depth <= 3 && legal > 8 + 4 * depth * depth
                && best_score > -MATE + 1000
            {
                pos.undo_move();
                continue;
            }
            // Futility pruning: 浅い深さで静的評価が alpha に遠く届かない静かな手は読まない
            if !pv && !in_check && !gives_check && quiet && depth <= 2 && legal > 1
                && alpha.abs() < MATE - 1000 && static_eval + 120 * depth <= alpha
            {
                pos.undo_move();
                continue;
            }
            let mut score;
            if legal == 1 {
                score = -self.negamax(pos, depth - 1, ply + 1, -beta, -alpha, pv);
            } else {
                // Late move reduction
                let mut r = 0;
                if depth >= 3 && quiet && !in_check && !gives_check && legal > 3 {
                    r = (0.75 + (depth as f64).ln() * (legal as f64).ln() / 2.25) as i32;
                    r = r.clamp(1, depth - 2);
                    if pv {
                        r -= 1;
                    }
                    let h = self.history[hidx];
                    if h > 2000 {
                        r -= 1;
                    } else if h < -500 {
                        r += 1;
                    }
                    r = r.clamp(0, depth - 2);
                }
                score = -self.negamax(pos, depth - 1 - r, ply + 1, -alpha - 1, -alpha, false);
                if score > alpha && r > 0 {
                    score = -self.negamax(pos, depth - 1, ply + 1, -alpha - 1, -alpha, false);
                }
                if score > alpha && score < beta && pv {
                    score = -self.negamax(pos, depth - 1, ply + 1, -beta, -alpha, true);
                }
            }
            pos.undo_move();
            if self.stopped {
                self.move_bufs[ply] = moves;
                self.quiet_bufs[ply] = quiets_tried;
                return 0;
            }
            if score > best_score {
                best_score = score;
                best_move = m;
                if score > alpha {
                    alpha = score;
                    if alpha >= beta {
                        if quiet {
                            if self.killers[ply][0] != m {
                                self.killers[ply][1] = self.killers[ply][0];
                                self.killers[ply][0] = m;
                            }
                            self.history[hidx] += depth * depth;
                            for &q in quiets_tried.iter() {
                                let qi = Self::hist_idx(pos, q);
                                self.history[qi] -= depth * depth / 2;
                            }
                        }
                        break;
                    }
                }
            }
            if quiet {
                quiets_tried.push(m);
            }
        }
        self.move_bufs[ply] = moves;
        self.quiet_bufs[ply] = quiets_tried;
        if legal == 0 {
            // 詰み（将棋では手がない＝負け）
            return -MATE + ply as i32;
        }
        let flag = if best_score >= beta {
            2
        } else if best_score > orig_alpha {
            1
        } else {
            3
        };
        self.tt.store(key, best_move, score_to_tt(best_score, ply), depth, flag);
        best_score
    }

    fn qsearch(&mut self, pos: &mut Position, ply: usize, mut alpha: i32, beta: i32, qdepth: i32) -> i32 {
        self.nodes += 1;
        self.check_stop();
        if self.stopped {
            return 0;
        }
        if ply >= MAX_PLY - 1 {
            return evaluate(pos);
        }
        let in_check = qdepth < 4 && pos.in_check();
        let mut best = -INF;
        if !in_check {
            let stand = evaluate(pos);
            if stand >= beta {
                return stand;
            }
            if stand > alpha {
                alpha = stand;
            }
            best = stand;
        }
        let mut moves = std::mem::take(&mut self.move_bufs[ply]);
        moves.clear();
        pos.pseudo_moves(&mut moves, !in_check);
        self.order_moves(pos, &mut moves, NO_MOVE, MAX_PLY);
        let mut legal = 0;
        for &m in moves.iter() {
            if !in_check && !mv_is_drop(m) {
                let victim = PIECE_VALUE[ptype(pos.board[mv_to(m)]) as usize];
                let attacker = PIECE_VALUE[ptype(pos.board[mv_from(m)]) as usize];
                // Delta pruning
                if best + victim + 200 <= alpha && !mv_is_promo(m) {
                    continue;
                }
                // 安い駒を高い駒で取り、取り返される手は読まない（簡易 SEE）
                if attacker > victim + 50 && pos.is_attacked(mv_to(m), pos.side ^ 1) {
                    continue;
                }
            }
            if !self.try_move(pos, m) {
                continue;
            }
            legal += 1;
            let score = -self.qsearch(pos, ply + 1, -beta, -alpha, qdepth + 1);
            pos.undo_move();
            if self.stopped {
                self.move_bufs[ply] = moves;
                return 0;
            }
            if score > best {
                best = score;
                if score > alpha {
                    alpha = score;
                    if alpha >= beta {
                        break;
                    }
                }
            }
        }
        self.move_bufs[ply] = moves;
        if in_check && legal == 0 {
            return -MATE + ply as i32;
        }
        best
    }
}
