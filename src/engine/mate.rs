//! 詰み探索（df-pn）。攻め方は王手だけ、受け方は全ての合法手（王手を受ける手）を読む。
//!
//! 証明（詰み）は経路に依存しない（千日手・手数制限は全て「詰まない」側に倒す）ので、
//! 見つけた詰みは正しい。逆に不詰の判定は経路依存のことがあり、詰みを見逃すことはある。

use super::position::*;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use super::browser_clock::Instant;

const INF: u32 = 100_000_000;
/// 詰み探索で読む最大手数
const MAX_MATE_PLY: usize = 41;

#[derive(Clone, Copy, Default)]
struct Entry {
    key: u64,
    pn: u32,
    dn: u32,
}

pub struct MateSolver {
    table: Vec<Entry>,
    mask: usize,
    nodes: u64,
    node_limit: u64,
    start: Instant,
    time_ms: u64,
    stopped: bool,
    path: Vec<u64>,
}

/// 打てる升か（行き所のない段・二歩）
#[inline]
fn drop_ok(pos: &Position, pt: u8, to: usize) -> bool {
    let r = to / 9;
    let rr = if pos.side == BLACK { r } else { 8 - r };
    match pt {
        PAWN => {
            if rr == 0 {
                return false;
            }
            let own_pawn = make_piece(pos.side, PAWN);
            let mut s = to % 9;
            while s < 81 {
                if pos.board[s] == own_pawn {
                    return false;
                }
                s += 9;
            }
            true
        }
        LANCE => rr != 0,
        KNIGHT => rr > 1,
        _ => true,
    }
}

/// 疑似合法手 m が合法か（自玉への王手放置・打ち歩詰めの除外）
#[inline]
fn legal(pos: &mut Position, m: Move) -> bool {
    if mv_is_drop(m) && mv_drop_pt(m) == PAWN {
        return pos.is_legal(m);
    }
    let us = pos.side;
    pos.do_move(m);
    let ok = !pos.is_attacked(pos.king_sq[us], us ^ 1);
    pos.undo_move();
    ok
}

/// 攻め方の王手になる合法手を (手, 指した後の局面のキー) で列挙する
fn gen_checks(pos: &mut Position, out: &mut Vec<(Move, u64)>) {
    let t = tables();
    let us = pos.side;
    let them = us ^ 1;
    let ksq = pos.king_sq[them];
    let mut list = Vec::with_capacity(128);
    pos.pseudo_board_moves(&mut list);
    // 王手になる駒打ち: 敵玉から各方向に空き升をたどり、その升に打つと玉に利く駒
    for d in 0..8 {
        let od = 7 - d;
        for i in 0..t.ray_len[ksq][d] as usize {
            let s = t.ray[ksq][d][i] as usize;
            if pos.board[s] != EMPTY {
                break;
            }
            for pt in [PAWN, LANCE, SILVER, GOLD, BISHOP, ROOK] {
                if pos.hand[us][pt as usize] == 0 {
                    continue;
                }
                let dirs = if i == 0 { t.step_mask[us][pt as usize] | t.slide_mask[us][pt as usize] } else { t.slide_mask[us][pt as usize] };
                if dirs & (1 << od) != 0 && drop_ok(pos, pt, s) {
                    list.push(mv_drop(pt, s));
                }
            }
        }
    }
    if pos.hand[us][KNIGHT as usize] > 0 {
        // 敵玉の升から相手の桂の動きで行ける升 = こちらの桂が玉に利く升
        let mut b = t.step_bb[them][KNIGHT as usize][ksq];
        while b != 0 {
            let s = b.trailing_zeros() as usize;
            b &= b - 1;
            if pos.board[s] == EMPTY && drop_ok(pos, KNIGHT, s) {
                list.push(mv_drop(KNIGHT, s));
            }
        }
    }
    for m in list {
        if mv_is_drop(m) && mv_drop_pt(m) == PAWN && !pos.is_legal(m) {
            continue;
        }
        pos.do_move(m);
        if !pos.is_attacked(pos.king_sq[us], them) && pos.is_attacked(pos.king_sq[them], us) {
            out.push((m, pos.key()));
        }
        pos.undo_move();
    }
}

/// 王手されている受け方の合法手を (手, 指した後の局面のキー) で列挙する
fn gen_evasions(pos: &mut Position, out: &mut Vec<(Move, u64)>) {
    let t = tables();
    let us = pos.side;
    let them = us ^ 1;
    let ksq = pos.king_sq[us];
    let mut list = Vec::with_capacity(64);
    pos.pseudo_board_moves(&mut list);
    // 合駒: 飛び駒の王手の間の升にだけ打てる
    if pos.hand[us].iter().any(|&n| n > 0) {
        let occ = pos.occ[0] | pos.occ[1];
        for d in 0..8 {
            let blockers = t.ray_mask[ksq][d] & occ;
            if blockers == 0 {
                continue;
            }
            let first = first_blocker(blockers, d);
            let p = pos.board[first];
            let od = 7 - d;
            if color_of(p) != them || t.slide_mask[them][ptype(p) as usize] & (1 << od) == 0 {
                continue;
            }
            for i in 0..t.ray_len[ksq][d] as usize {
                let s = t.ray[ksq][d][i] as usize;
                if s == first {
                    break;
                }
                for pt in PAWN..=ROOK {
                    if pos.hand[us][pt as usize] > 0 && drop_ok(pos, pt, s) {
                        list.push(mv_drop(pt, s));
                    }
                }
            }
        }
    }
    for m in list {
        if !legal(pos, m) {
            continue;
        }
        pos.do_move(m);
        out.push((m, pos.key()));
        pos.undo_move();
    }
}

impl MateSolver {
    pub fn new(mb: usize) -> Self {
        let n = (mb * 1024 * 1024 / std::mem::size_of::<Entry>()).next_power_of_two() / 2;
        let n = n.max(1024);
        MateSolver {
            table: vec![Entry::default(); n],
            mask: n - 1,
            nodes: 0,
            node_limit: u64::MAX,
            start: Instant::now(),
            time_ms: u64::MAX,
            stopped: false,
            path: Vec::with_capacity(MAX_MATE_PLY + 2),
        }
    }

    pub fn clear(&mut self) {
        for e in self.table.iter_mut() {
            *e = Entry::default();
        }
    }

    pub fn nodes(&self) -> u64 {
        self.nodes
    }

    #[inline]
    fn lookup(&self, key: u64) -> Option<(u32, u32)> {
        let e = &self.table[key as usize & self.mask];
        if e.key == key && (e.pn | e.dn) != 0 {
            Some((e.pn, e.dn))
        } else {
            None
        }
    }

    #[inline]
    fn store(&mut self, key: u64, pn: u32, dn: u32) {
        let e = &mut self.table[key as usize & self.mask];
        // 詰み・不詰の確定値は、未確定の値で上書きしない
        if e.key == key && (e.pn == 0 || e.dn == 0) && pn != 0 && dn != 0 {
            return;
        }
        *e = Entry { key, pn, dn };
    }

    /// 手番側（攻め方）が王手の連続で詰ませられるなら、その初手を返す
    pub fn solve(&mut self, pos: &mut Position, node_limit: u64, time_ms: u64) -> Option<Move> {
        self.nodes = 0;
        self.node_limit = node_limit;
        self.time_ms = time_ms;
        self.start = Instant::now();
        self.stopped = false;
        self.path.clear();
        let (pn, _) = self.mid(pos, INF, INF, true, 0);
        if pn != 0 {
            return None;
        }
        let mut ch = Vec::new();
        gen_checks(pos, &mut ch);
        ch.into_iter().find(|&(_, k)| matches!(self.lookup(k), Some((0, _)))).map(|(m, _)| m)
    }

    fn mid(&mut self, pos: &mut Position, th_pn: u32, th_dn: u32, or: bool, ply: usize) -> (u32, u32) {
        self.nodes += 1;
        if self.nodes & 255 == 0 && self.start.elapsed().as_millis() as u64 >= self.time_ms {
            self.stopped = true;
        }
        if self.nodes >= self.node_limit {
            self.stopped = true;
        }
        if ply >= MAX_MATE_PLY {
            return (INF, 0);
        }
        let key = pos.key();
        let mut children = Vec::with_capacity(32);
        if or {
            gen_checks(pos, &mut children);
        } else {
            gen_evasions(pos, &mut children);
        }
        if children.is_empty() {
            let r = if or { (INF, 0) } else { (0, INF) };
            self.store(key, r.0, r.1);
            return r;
        }
        self.path.push(key);
        let mut pn;
        let mut dn;
        loop {
            // 子の値を集める。OR: pn = min, dn = sum。AND: pn = sum, dn = min
            let mut best = 0;
            let mut best_v = INF + 1; // OR なら子の pn、AND なら子の dn の最小
            let mut second = INF + 1;
            let mut sum: u64 = 0;
            let mut best_other = 0u32; // 最善の子のもう一方の値
            for (i, &(_, ck)) in children.iter().enumerate() {
                let (cp, cd) = if self.path.contains(&ck) { (INF, 0) } else { self.lookup(ck).unwrap_or((1, 1)) };
                let (v, o) = if or { (cp, cd) } else { (cd, cp) };
                if v < best_v {
                    second = best_v;
                    best_v = v;
                    best = i;
                    best_other = o;
                } else if v < second {
                    second = v;
                }
                sum += o as u64;
            }
            let sum = sum.min(INF as u64 - 1) as u32;
            if or {
                pn = best_v;
                dn = if pn == 0 { INF } else { sum };
            } else {
                dn = best_v;
                pn = if dn == 0 { INF } else { sum };
            }
            self.store(key, pn, dn);
            if pn >= th_pn || dn >= th_dn || self.stopped {
                break;
            }
            let (cth_pn, cth_dn) = if or {
                let a = th_pn.min(second.saturating_add(1)).min(INF);
                let b = (th_dn as u64 - dn as u64 + best_other as u64).min(INF as u64) as u32;
                (a, b)
            } else {
                let b = th_dn.min(second.saturating_add(1)).min(INF);
                let a = (th_pn as u64 - pn as u64 + best_other as u64).min(INF as u64) as u32;
                (a, b)
            };
            let m = children[best].0;
            pos.do_move(m);
            let r = self.mid(pos, cth_pn, cth_dn, !or, ply + 1);
            pos.undo_move();
            if ply + 1 >= MAX_MATE_PLY {
                // 手数制限の不詰は保存されないので、ここで子を不詰として扱う
                let ck = children[best].1;
                self.store(ck, r.0, r.1);
            }
        }
        self.path.pop();
        (pn, dn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solve(sfen: &str) -> Option<String> {
        let mut pos = Position::from_sfen(sfen).unwrap();
        let mut s = MateSolver::new(4);
        s.solve(&mut pos, 200_000, 10_000).map(move_to_usi)
    }

    #[test]
    fn head_gold_mate() {
        // 頭金
        assert_eq!(solve("4k4/9/4P4/9/9/9/9/9/4K4 b G 1").as_deref(), Some("G*5b"));
    }

    #[test]
    fn no_mate_without_pieces() {
        assert_eq!(solve("4k4/9/9/9/9/9/9/9/4K4 b - 1"), None);
    }

    #[test]
    fn pawn_drop_mate_is_not_mate() {
        // 打ち歩詰めしかない局面は詰みではない
        assert_eq!(solve("7nk/9/7G1/9/9/9/9/9/K8 b P 1"), None);
    }

    #[test]
    fn three_ply_mate() {
        // 3手詰め: ▲2二銀成 △同玉 ▲2三金 などではなく、確実な例として 1 段目の玉を金銀で詰める
        let r = solve("6snk/9/7G1/9/9/9/9/9/K8 b GS 1");
        assert!(r.is_some());
    }

    #[test]
    fn mate_line_is_really_mate() {
        // 見つけた詰みを最後までたどり、受け方に手がなくなることを確かめる
        let sfens = [
            "6snk/9/7G1/9/9/9/9/9/K8 b GS 1",
            "l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL b RGgsn5p 1",
        ];
        for sfen in sfens {
            let mut pos = Position::from_sfen(sfen).unwrap();
            let mut s = MateSolver::new(4);
            if let Some(m) = s.solve(&mut pos, 200_000, 10_000) {
                // 受け方のどの手にも、次の手番で再び詰みが見つかる（最大 41 手）
                assert!(pos.is_legal(m));
                pos.do_move(m);
                assert!(pos.in_check());
                let ev = pos.legal_moves();
                for e in ev {
                    pos.do_move(e);
                    assert!(s.solve(&mut pos, 200_000, 10_000).is_some(), "{} after {}", sfen, move_to_usi(e));
                    pos.undo_move();
                }
            }
        }
    }
}

/// 詰み探索の検証・統計（対局記録の局面で）。
/// `matestat --data FILE [--games N] [--nodes N]`
#[cfg(not(target_arch = "wasm32"))]
pub fn run_matestat(args: &[String]) {
    let get = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned());
    let data = get("--data").expect("--data");
    let games: usize = get("--games").and_then(|v| v.parse().ok()).unwrap_or(200);
    let node_limit: u64 = get("--nodes").and_then(|v| v.parse().ok()).unwrap_or(20000);
    let text = std::fs::read_to_string(&data).expect("read data");
    let lines: Vec<&str> = text.lines().collect();
    let mut solver = MateSolver::new(16);
    let (mut npos, mut nfound, mut nodes_total, mut ms_total) = (0u64, 0u64, 0u64, 0f64);
    let (mut lost_with_mate, mut games_n, mut earlier_sum, mut earlier_n) = (0, 0, 0usize, 0usize);
    let mut max_ms = 0f64;
    for line in lines.iter().rev().take(games) {
        let get_field = |f: &str| -> Option<String> {
            let k = format!("\"{}\":\"", f);
            let i = line.find(&k)? + k.len();
            let j = line[i..].find('"')? + i;
            Some(line[i..j].to_string())
        };
        let (Some(moves), Some(winner), Some(black)) = (get_field("moves"), get_field("winner"), get_field("black")) else { continue };
        let reason = get_field("reason").unwrap_or_default();
        if reason != "mate" {
            continue;
        }
        let winner_color = if winner == black { BLACK } else { WHITE };
        games_n += 1;
        solver.clear();
        let mut pos = Position::startpos();
        let mvs: Vec<&str> = moves.split_whitespace().collect();
        let mut first_win_mate: Option<usize> = None;
        let mut loser_had_mate = false;
        for (ply, u) in mvs.iter().enumerate() {
            // 生成の検証
            let legal_all = pos.legal_moves();
            let mut ch = Vec::new();
            gen_checks(&mut pos, &mut ch);
            let mut want: Vec<Move> = legal_all
                .iter()
                .copied()
                .filter(|&m| {
                    pos.do_move(m);
                    let c = pos.in_check();
                    pos.undo_move();
                    c
                })
                .collect();
            let mut got: Vec<Move> = ch.iter().map(|x| x.0).collect();
            want.sort();
            got.sort();
            assert_eq!(want, got, "checks differ at {}", pos.to_sfen());
            if pos.in_check() {
                let mut ev = Vec::new();
                gen_evasions(&mut pos, &mut ev);
                let mut got: Vec<Move> = ev.iter().map(|x| x.0).collect();
                let mut want = legal_all.clone();
                got.sort();
                want.sort();
                assert_eq!(want, got, "evasions differ at {}", pos.to_sfen());
            }
            if !ch.is_empty() {
                let t = std::time::Instant::now();
                let r = solver.solve(&mut pos, node_limit, 100_000);
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                npos += 1;
                nodes_total += solver.nodes();
                ms_total += ms;
                max_ms = max_ms.max(ms);
                if r.is_some() {
                    nfound += 1;
                    if pos.side == winner_color {
                        first_win_mate.get_or_insert(ply);
                    } else {
                        loser_had_mate = true;
                    }
                }
            }
            let Some(m) = pos.parse_usi_move(u) else { break };
            pos.do_move(m);
        }
        if loser_had_mate {
            lost_with_mate += 1;
        }
        if let Some(p) = first_win_mate {
            earlier_sum += mvs.len() - p;
            earlier_n += 1;
        }
    }
    println!(
        "games {} positions-with-check {} found {} avg-nodes {:.0} avg-ms {:.2} max-ms {:.1}",
        games_n,
        npos,
        nfound,
        nodes_total as f64 / npos.max(1) as f64,
        ms_total / npos.max(1) as f64,
        max_ms
    );
    println!(
        "loser had a mate (missed) in {} games; winner's first found mate {:.1} plies before the end (avg over {})",
        lost_with_mate,
        earlier_sum as f64 / earlier_n.max(1) as f64,
        earlier_n
    );
}
