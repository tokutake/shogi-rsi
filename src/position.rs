//! 将棋の盤面表現・合法手生成・SFEN入出力。
//!
//! 座標系: sq = r * 9 + c。r=0 が一段目(a)、c=0 が9筋（盤の左端）。
//! USI の "7g" は 筋7 → c = 9 - 7 = 2、段 g → r = 6。
//! 先手(BLACK=0)は r が減る方向へ進む。

pub type Color = usize;
pub const BLACK: Color = 0;
pub const WHITE: Color = 1;

pub const PAWN: u8 = 1;
pub const LANCE: u8 = 2;
pub const KNIGHT: u8 = 3;
pub const SILVER: u8 = 4;
pub const GOLD: u8 = 5;
pub const BISHOP: u8 = 6;
pub const ROOK: u8 = 7;
pub const KING: u8 = 8;
pub const PPAWN: u8 = 9;
pub const PLANCE: u8 = 10;
pub const PKNIGHT: u8 = 11;
pub const PSILVER: u8 = 12;
pub const HORSE: u8 = 13;
pub const DRAGON: u8 = 14;

pub const EMPTY: u8 = 0;

#[inline]
pub fn make_piece(c: Color, pt: u8) -> u8 {
    pt | ((c as u8) << 4)
}
#[inline]
pub fn color_of(p: u8) -> Color {
    (p >> 4) as usize
}
#[inline]
pub fn ptype(p: u8) -> u8 {
    p & 15
}
#[inline]
pub fn can_promote_type(pt: u8) -> bool {
    matches!(pt, PAWN | LANCE | KNIGHT | SILVER | BISHOP | ROOK)
}
#[inline]
pub fn promote(pt: u8) -> u8 {
    match pt {
        PAWN | LANCE | KNIGHT | SILVER => pt + 8,
        BISHOP => HORSE,
        ROOK => DRAGON,
        _ => pt,
    }
}
#[inline]
pub fn unpromote(pt: u8) -> u8 {
    match pt {
        PPAWN | PLANCE | PKNIGHT | PSILVER => pt - 8,
        HORSE => BISHOP,
        DRAGON => ROOK,
        _ => pt,
    }
}

// ---- 指し手 ----
// bit 0-6: to, bit 7-13: from (打つ手なら駒種), bit 14: 成, bit 15: 打
pub type Move = u32;
pub const NO_MOVE: Move = 0;

#[inline]
pub fn mv_normal(from: usize, to: usize, promo: bool) -> Move {
    (to as u32) | ((from as u32) << 7) | ((promo as u32) << 14)
}
#[inline]
pub fn mv_drop(pt: u8, to: usize) -> Move {
    (to as u32) | ((pt as u32) << 7) | (1 << 15)
}
#[inline]
pub fn mv_to(m: Move) -> usize {
    (m & 0x7f) as usize
}
#[inline]
pub fn mv_from(m: Move) -> usize {
    ((m >> 7) & 0x7f) as usize
}
#[inline]
pub fn mv_is_promo(m: Move) -> bool {
    m & (1 << 14) != 0
}
#[inline]
pub fn mv_is_drop(m: Move) -> bool {
    m & (1 << 15) != 0
}
#[inline]
pub fn mv_drop_pt(m: Move) -> u8 {
    ((m >> 7) & 0x7f) as u8
}

fn sq_to_usi(sq: usize) -> String {
    let r = sq / 9;
    let c = sq % 9;
    format!("{}{}", 9 - c, (b'a' + r as u8) as char)
}

fn usi_to_sq(s: &[u8]) -> Option<usize> {
    if s.len() < 2 {
        return None;
    }
    let f = s[0].checked_sub(b'0')? as usize;
    let r = s[1].checked_sub(b'a')? as usize;
    if !(1..=9).contains(&f) || r > 8 {
        return None;
    }
    Some(r * 9 + (9 - f))
}

const PIECE_CHARS: [char; 8] = [' ', 'P', 'L', 'N', 'S', 'G', 'B', 'R'];

pub fn move_to_usi(m: Move) -> String {
    if m == NO_MOVE {
        return "none".into();
    }
    if mv_is_drop(m) {
        format!("{}*{}", PIECE_CHARS[mv_drop_pt(m) as usize], sq_to_usi(mv_to(m)))
    } else {
        format!(
            "{}{}{}",
            sq_to_usi(mv_from(m)),
            sq_to_usi(mv_to(m)),
            if mv_is_promo(m) { "+" } else { "" }
        )
    }
}

// ---- 駒の動き（先手視点、dr は段の増分） ----
pub const DIRS8: [(i32, i32); 8] = [
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];

/// 先手視点で、駒種 pt が (dr,dc) へ1歩で動けるか（飛び駒の1歩目も含む）
#[inline]
pub fn can_step_black(pt: u8, dr: i32, dc: i32) -> bool {
    match pt {
        PAWN | LANCE => dr == -1 && dc == 0,
        SILVER => (dr == -1) || (dr == 1 && dc != 0),
        GOLD | PPAWN | PLANCE | PKNIGHT | PSILVER => dr == -1 || (dr == 0) || (dr == 1 && dc == 0),
        KING | HORSE | DRAGON => true,
        BISHOP => dr != 0 && dc != 0,
        ROOK => dr == 0 || dc == 0,
        _ => false,
    }
}

#[inline]
fn is_slider_dir(pt: u8, dr: i32, dc: i32) -> bool {
    // 先手視点
    match pt {
        LANCE => dr == -1 && dc == 0,
        BISHOP | HORSE => dr != 0 && dc != 0,
        ROOK | DRAGON => dr == 0 || dc == 0,
        _ => false,
    }
}


// ---- 事前計算テーブル（高速化） ----
pub struct Tables {
    /// ray[sq][dir] = その方向に並ぶマス（近い順）
    pub ray: [[[u8; 8]; 8]; 81],
    pub ray_len: [[u8; 8]; 81],
    /// step_mask[color][pt] = 1歩で動ける方向のビット集合（盤上の向き）
    pub step_mask: [[u8; 16]; 2],
    /// slide_mask[color][pt] = 飛んで動ける方向のビット集合
    pub slide_mask: [[u8; 16]; 2],
}

pub fn tables() -> &'static Tables {
    use std::sync::OnceLock;
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Tables { ray: [[[0; 8]; 8]; 81], ray_len: [[0; 8]; 81], step_mask: [[0; 16]; 2], slide_mask: [[0; 16]; 2] };
        for sq in 0..81 {
            for (d, &(dr, dc)) in DIRS8.iter().enumerate() {
                let (mut r, mut c) = ((sq / 9) as i32 + dr, (sq % 9) as i32 + dc);
                let mut n = 0;
                while (0..9).contains(&r) && (0..9).contains(&c) {
                    t.ray[sq][d][n] = (r * 9 + c) as u8;
                    n += 1;
                    r += dr;
                    c += dc;
                }
                t.ray_len[sq][d] = n as u8;
            }
        }
        for color in 0..2 {
            let sign = if color == BLACK { 1 } else { -1 };
            for pt in 1..15u8 {
                for (d, &(dr, dc)) in DIRS8.iter().enumerate() {
                    if is_slider_dir(pt, dr * sign, dc * sign) {
                        t.slide_mask[color][pt as usize] |= 1 << d;
                    } else if can_step_black(pt, dr * sign, dc * sign) {
                        t.step_mask[color][pt as usize] |= 1 << d;
                    }
                }
            }
        }
        t
    })
}

// ---- Zobrist ----
struct Zobrist {
    board: [[u64; 81]; 32],
    hand: [[[u64; 19]; 8]; 2],
    side: u64,
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn zobrist() -> &'static Zobrist {
    use std::sync::OnceLock;
    static Z: OnceLock<Zobrist> = OnceLock::new();
    Z.get_or_init(|| {
        let mut s = 0x1234_5678_9abc_def0u64;
        let mut z = Zobrist {
            board: [[0; 81]; 32],
            hand: [[[0; 19]; 8]; 2],
            side: 0,
        };
        for p in 0..32 {
            for sq in 0..81 {
                z.board[p][sq] = splitmix(&mut s);
            }
        }
        for c in 0..2 {
            for pt in 0..8 {
                for n in 0..19 {
                    z.hand[c][pt][n] = splitmix(&mut s);
                }
            }
        }
        z.side = splitmix(&mut s);
        z
    })
}

#[derive(Clone, Copy)]
struct Undo {
    mv: Move,
    captured: u8,
    hash: u64,
}

#[derive(Clone)]
pub struct Position {
    pub board: [u8; 81],
    pub hand: [[u8; 8]; 2],
    pub side: Color,
    pub king_sq: [usize; 2],
    pub hash: u64,
    pub ply: usize, // 開始局面からの手数
    history: Vec<Undo>,
}

pub const STARTPOS_SFEN: &str = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";

impl Default for Position {
    fn default() -> Self {
        Self::startpos()
    }
}

impl Position {
    pub fn startpos() -> Self {
        Self::from_sfen(STARTPOS_SFEN).unwrap()
    }

    pub fn from_sfen(sfen: &str) -> Option<Self> {
        let parts: Vec<&str> = sfen.split_whitespace().collect();
        if parts.len() < 3 {
            return None;
        }
        let mut pos = Position {
            board: [EMPTY; 81],
            hand: [[0; 8]; 2],
            side: BLACK,
            king_sq: [81, 81],
            hash: 0,
            ply: 0,
            history: Vec::with_capacity(512),
        };
        let (mut r, mut c) = (0usize, 0usize);
        let mut promoted = false;
        for ch in parts[0].chars() {
            match ch {
                '/' => {
                    r += 1;
                    c = 0;
                }
                '+' => promoted = true,
                '1'..='9' => c += ch as usize - '0' as usize,
                _ => {
                    let color = if ch.is_ascii_uppercase() { BLACK } else { WHITE };
                    let mut pt = match ch.to_ascii_uppercase() {
                        'P' => PAWN,
                        'L' => LANCE,
                        'N' => KNIGHT,
                        'S' => SILVER,
                        'G' => GOLD,
                        'B' => BISHOP,
                        'R' => ROOK,
                        'K' => KING,
                        _ => return None,
                    };
                    if promoted {
                        pt = promote(pt);
                        promoted = false;
                    }
                    if r > 8 || c > 8 {
                        return None;
                    }
                    let sq = r * 9 + c;
                    pos.board[sq] = make_piece(color, pt);
                    if pt == KING {
                        pos.king_sq[color] = sq;
                    }
                    c += 1;
                }
            }
        }
        pos.side = if parts[1] == "w" { WHITE } else { BLACK };
        if parts[2] != "-" {
            let mut n = 0u8;
            for ch in parts[2].chars() {
                if let Some(d) = ch.to_digit(10) {
                    n = n * 10 + d as u8;
                    continue;
                }
                let color = if ch.is_ascii_uppercase() { BLACK } else { WHITE };
                let pt = match ch.to_ascii_uppercase() {
                    'P' => PAWN,
                    'L' => LANCE,
                    'N' => KNIGHT,
                    'S' => SILVER,
                    'G' => GOLD,
                    'B' => BISHOP,
                    'R' => ROOK,
                    _ => return None,
                };
                pos.hand[color][pt as usize] += if n == 0 { 1 } else { n };
                n = 0;
            }
        }
        pos.hash = pos.compute_hash();
        Some(pos)
    }

    pub fn to_sfen(&self) -> String {
        let mut s = String::new();
        for r in 0..9 {
            let mut empty = 0;
            for c in 0..9 {
                let p = self.board[r * 9 + c];
                if p == EMPTY {
                    empty += 1;
                    continue;
                }
                if empty > 0 {
                    s.push_str(&empty.to_string());
                    empty = 0;
                }
                let pt = ptype(p);
                if pt > KING {
                    s.push('+');
                }
                let base = unpromote(pt);
                let ch = if base == KING { 'K' } else { PIECE_CHARS[base as usize] };
                s.push(if color_of(p) == BLACK { ch } else { ch.to_ascii_lowercase() });
            }
            if empty > 0 {
                s.push_str(&empty.to_string());
            }
            if r < 8 {
                s.push('/');
            }
        }
        s.push_str(if self.side == BLACK { " b " } else { " w " });
        let mut h = String::new();
        for c in 0..2 {
            for pt in [ROOK, BISHOP, GOLD, SILVER, KNIGHT, LANCE, PAWN] {
                let n = self.hand[c][pt as usize];
                if n == 0 {
                    continue;
                }
                if n > 1 {
                    h.push_str(&n.to_string());
                }
                let ch = PIECE_CHARS[pt as usize];
                h.push(if c == BLACK { ch } else { ch.to_ascii_lowercase() });
            }
        }
        if h.is_empty() {
            h.push('-');
        }
        s.push_str(&h);
        s.push_str(" 1");
        s
    }

    fn compute_hash(&self) -> u64 {
        let z = zobrist();
        let mut h = 0u64;
        for sq in 0..81 {
            let p = self.board[sq];
            if p != EMPTY {
                h ^= z.board[p as usize][sq];
            }
        }
        for c in 0..2 {
            for pt in 1..8 {
                h ^= z.hand[c][pt][self.hand[c][pt] as usize];
            }
        }
        if self.side == WHITE {
            h ^= z.side;
        }
        h
    }

    /// 盤面＋持ち駒＋手番のキー（千日手判定用）
    #[inline]
    pub fn key(&self) -> u64 {
        self.hash
    }

    pub fn parse_usi_move(&self, s: &str) -> Option<Move> {
        let b = s.as_bytes();
        if b.len() >= 4 && b[1] == b'*' {
            let pt = match b[0] {
                b'P' => PAWN,
                b'L' => LANCE,
                b'N' => KNIGHT,
                b'S' => SILVER,
                b'G' => GOLD,
                b'B' => BISHOP,
                b'R' => ROOK,
                _ => return None,
            };
            let to = usi_to_sq(&b[2..4])?;
            return Some(mv_drop(pt, to));
        }
        if b.len() < 4 {
            return None;
        }
        let from = usi_to_sq(&b[0..2])?;
        let to = usi_to_sq(&b[2..4])?;
        let promo = b.len() >= 5 && b[4] == b'+';
        Some(mv_normal(from, to, promo))
    }

    /// 文字列の指し手を合法手リストと照合して返す
    pub fn parse_legal_usi_move(&mut self, s: &str) -> Option<Move> {
        let m = self.parse_usi_move(s)?;
        let legal = self.legal_moves();
        legal.into_iter().find(|&x| x == m)
    }

    // ---- 着手 ----
    pub fn do_move(&mut self, m: Move) {
        let z = zobrist();
        let us = self.side;
        let to = mv_to(m);
        let mut captured = EMPTY;
        let prev_hash = self.hash;
        if mv_is_drop(m) {
            let pt = mv_drop_pt(m);
            let n = self.hand[us][pt as usize] as usize;
            self.hash ^= z.hand[us][pt as usize][n] ^ z.hand[us][pt as usize][n - 1];
            self.hand[us][pt as usize] -= 1;
            let p = make_piece(us, pt);
            self.board[to] = p;
            self.hash ^= z.board[p as usize][to];
        } else {
            let from = mv_from(m);
            let p = self.board[from];
            captured = self.board[to];
            if captured != EMPTY {
                self.hash ^= z.board[captured as usize][to];
                let cpt = unpromote(ptype(captured)) as usize;
                let n = self.hand[us][cpt] as usize;
                self.hash ^= z.hand[us][cpt][n] ^ z.hand[us][cpt][n + 1];
                self.hand[us][cpt] += 1;
            }
            self.hash ^= z.board[p as usize][from];
            self.board[from] = EMPTY;
            let np = if mv_is_promo(m) { make_piece(us, promote(ptype(p))) } else { p };
            self.board[to] = np;
            self.hash ^= z.board[np as usize][to];
            if ptype(p) == KING {
                self.king_sq[us] = to;
            }
        }
        self.hash ^= z.side;
        self.side ^= 1;
        self.ply += 1;
        self.history.push(Undo { mv: m, captured, hash: prev_hash });
    }

    pub fn undo_move(&mut self) {
        let u = self.history.pop().expect("undo without move");
        self.side ^= 1;
        self.ply -= 1;
        let us = self.side;
        let m = u.mv;
        let to = mv_to(m);
        if mv_is_drop(m) {
            let pt = mv_drop_pt(m);
            self.hand[us][pt as usize] += 1;
            self.board[to] = EMPTY;
        } else {
            let from = mv_from(m);
            let p = self.board[to];
            let orig = if mv_is_promo(m) { make_piece(us, unpromote(ptype(p))) } else { p };
            self.board[from] = orig;
            self.board[to] = u.captured;
            if u.captured != EMPTY {
                self.hand[us][unpromote(ptype(u.captured)) as usize] -= 1;
            }
            if ptype(orig) == KING {
                self.king_sq[us] = from;
            }
        }
        self.hash = u.hash;
    }

    pub fn do_null_move(&mut self) {
        let prev = self.hash;
        self.hash ^= zobrist().side;
        self.side ^= 1;
        self.ply += 1;
        self.history.push(Undo { mv: NO_MOVE, captured: EMPTY, hash: prev });
    }

    pub fn undo_null_move(&mut self) {
        let u = self.history.pop().unwrap();
        self.side ^= 1;
        self.ply -= 1;
        self.hash = u.hash;
    }

    /// 直近 n 手前の局面ハッシュ（1 = 直前の局面）
    pub fn history_hash(&self, back: usize) -> Option<u64> {
        if back == 0 || back > self.history.len() {
            None
        } else {
            Some(self.history[self.history.len() - back].hash)
        }
    }
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
    pub fn last_move(&self) -> Move {
        self.history.last().map(|u| u.mv).unwrap_or(NO_MOVE)
    }

    // ---- 利き判定 ----
    /// sq に color 側の駒の利きがあるか
    pub fn is_attacked(&self, sq: usize, by: Color) -> bool {
        let t = tables();
        for d in 0..8 {
            let len = t.ray_len[sq][d] as usize;
            if len == 0 {
                continue;
            }
            let od = 7 - d; // 攻め駒から sq への方向
            let ray = &t.ray[sq][d];
            let p = self.board[ray[0] as usize];
            if p != EMPTY {
                if color_of(p) == by {
                    let pt = ptype(p) as usize;
                    if (t.step_mask[by][pt] | t.slide_mask[by][pt]) & (1 << od) != 0 {
                        return true;
                    }
                }
                continue;
            }
            for i in 1..len {
                let p = self.board[ray[i] as usize];
                if p != EMPTY {
                    if color_of(p) == by && t.slide_mask[by][ptype(p) as usize] & (1 << od) != 0 {
                        return true;
                    }
                    break;
                }
            }
        }
        // 桂馬
        let r0 = (sq / 9) as i32;
        let c0 = (sq % 9) as i32;
        let kr = if by == BLACK { r0 + 2 } else { r0 - 2 };
        if (0..9).contains(&kr) {
            let kn = make_piece(by, KNIGHT);
            if c0 > 0 && self.board[(kr * 9 + c0 - 1) as usize] == kn {
                return true;
            }
            if c0 < 8 && self.board[(kr * 9 + c0 + 1) as usize] == kn {
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn in_check(&self) -> bool {
        self.is_attacked(self.king_sq[self.side], self.side ^ 1)
    }

    // ---- 指し手生成 ----
    #[inline]
    fn in_promo_zone(c: Color, r: i32) -> bool {
        if c == BLACK {
            r <= 2
        } else {
            r >= 6
        }
    }

    /// 行き所のない駒になるか（先手視点の段数で判定）
    #[inline]
    fn dead_end(c: Color, pt: u8, r: i32) -> bool {
        let rr = if c == BLACK { r } else { 8 - r };
        match pt {
            PAWN | LANCE => rr == 0,
            KNIGHT => rr <= 1,
            _ => false,
        }
    }

    fn push_board_move(&self, list: &mut Vec<Move>, from: usize, to: usize, pt: u8, fr: i32, tr: i32) {
        let us = self.side;
        if can_promote_type(pt) && (Self::in_promo_zone(us, fr) || Self::in_promo_zone(us, tr)) {
            list.push(mv_normal(from, to, true));
            if !Self::dead_end(us, pt, tr) {
                list.push(mv_normal(from, to, false));
            }
        } else {
            list.push(mv_normal(from, to, false));
        }
    }

    /// 疑似合法手（自玉への王手放置チェック・打ち歩詰めチェックなし）
    /// captures_only = true なら駒を取る手（と成る手）のみ
    pub fn pseudo_moves(&self, list: &mut Vec<Move>, captures_only: bool) {
        let us = self.side;
        let sign = if us == BLACK { 1 } else { -1 };
        for from in 0..81 {
            let p = self.board[from];
            if p == EMPTY || color_of(p) != us {
                continue;
            }
            let pt = ptype(p);
            let fr = (from / 9) as i32;
            if pt == KNIGHT {
                let fc = (from % 9) as i32;
                let tr = fr - 2 * sign;
                if (0..9).contains(&tr) {
                    for tc in [fc - 1, fc + 1] {
                        if (0..9).contains(&tc) {
                            let to = (tr * 9 + tc) as usize;
                            let t = self.board[to];
                            if t != EMPTY && color_of(t) == us {
                                continue;
                            }
                            if captures_only && t == EMPTY {
                                continue;
                            }
                            self.push_board_move(list, from, to, pt, fr, tr);
                        }
                    }
                }
                continue;
            }
            let t = tables();
            let smask = t.step_mask[us][pt as usize];
            let lmask = t.slide_mask[us][pt as usize];
            for d in 0..8 {
                let bit = 1u8 << d;
                if (smask | lmask) & bit == 0 {
                    continue;
                }
                let len = if lmask & bit != 0 { t.ray_len[from][d] as usize } else { (t.ray_len[from][d] as usize).min(1) };
                for i in 0..len {
                    let to = t.ray[from][d][i] as usize;
                    let tgt = self.board[to];
                    if tgt != EMPTY && color_of(tgt) == us {
                        break;
                    }
                    if !captures_only || tgt != EMPTY {
                        self.push_board_move(list, from, to, pt, fr, (to / 9) as i32);
                    }
                    if tgt != EMPTY {
                        break;
                    }
                }
            }
        }
        if captures_only {
            return;
        }
        // 打つ手
        let hand = &self.hand[us];
        if hand.iter().all(|&n| n == 0) {
            return;
        }
        let mut pawn_files = [false; 9];
        if hand[PAWN as usize] > 0 {
            for sq in 0..81 {
                if self.board[sq] == make_piece(us, PAWN) {
                    pawn_files[sq % 9] = true;
                }
            }
        }
        for pt in PAWN..=ROOK {
            if hand[pt as usize] == 0 {
                continue;
            }
            for to in 0..81 {
                if self.board[to] != EMPTY {
                    continue;
                }
                let tr = (to / 9) as i32;
                if Self::dead_end(us, pt, tr) {
                    continue;
                }
                if pt == PAWN && pawn_files[to % 9] {
                    continue;
                }
                list.push(mv_drop(pt, to));
            }
        }
    }

    /// 疑似合法手 m が合法か（自殺手・打ち歩詰めを除外）。m は do 前の局面で判定。
    pub fn is_legal(&mut self, m: Move) -> bool {
        let us = self.side;
        self.do_move(m);
        let ok = !self.is_attacked(self.king_sq[us], us ^ 1);
        let mut res = ok;
        if ok && mv_is_drop(m) && mv_drop_pt(m) == PAWN {
            // 打ち歩詰め: 打った歩が相手玉に王手をかけ、かつ相手に合法手がない
            let them = us ^ 1;
            let ksq = self.king_sq[them];
            let to = mv_to(m);
            let front = if us == BLACK { to as i32 - 9 } else { to as i32 + 9 };
            if front == ksq as i32 && !self.has_legal_move() {
                res = false;
            }
        }
        self.undo_move();
        res
    }

    pub fn has_legal_move(&mut self) -> bool {
        let mut list = Vec::with_capacity(128);
        self.pseudo_moves(&mut list, false);
        let us = self.side;
        for m in list {
            self.do_move(m);
            let ok = !self.is_attacked(self.king_sq[us], us ^ 1);
            self.undo_move();
            // ここでの打ち歩詰め再帰チェックは不要（存在判定なので歩打ちが詰めでも他の手で判定される）
            if ok {
                return true;
            }
        }
        false
    }

    pub fn legal_moves(&mut self) -> Vec<Move> {
        let mut list = Vec::with_capacity(160);
        self.pseudo_moves(&mut list, false);
        list.retain(|&m| self.is_legal(m));
        list
    }

    pub fn perft(&mut self, depth: u32) -> u64 {
        let moves = self.legal_moves();
        if depth == 1 {
            return moves.len() as u64;
        }
        let mut n = 0;
        for m in moves {
            self.do_move(m);
            n += self.perft(depth - 1);
            self.undo_move();
        }
        n
    }

    /// 千日手の判定: 現局面と同一の局面が過去に何回あったか（同一手番のみ、偶数手前を見る）
    pub fn repetition_count(&self) -> usize {
        let mut cnt = 0;
        let n = self.history.len();
        let mut back = 4;
        while back <= n {
            if self.history[n - back].hash == self.hash {
                cnt += 1;
            }
            back += 2;
        }
        cnt
    }

    pub fn piece_count(&self) -> usize {
        self.board.iter().filter(|&&p| p != EMPTY).count()
    }
}
