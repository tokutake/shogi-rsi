#!/usr/bin/env python3
"""shogi-rsi 改善ループ用ツール。

バージョンは「番号 + コードネーム」で管理する（例: v3 / chidori）。
どちらの書き方でもコマンドに渡せる。git のハッシュは覚えなくてよい。

  ./rsi.py list                       登録済みバージョンとレーティング一覧
  ./rsi.py build [NAME|all]           登録バージョンのバイナリを engines/ にビルド
  ./rsi.py candidate                  作業ツリーの現在のコードを engines/candidate にビルド
  ./rsi.py match A B [--games N]      A と B を対局させ results/games.jsonl に記録
  ./rsi.py gauntlet A [--vs B C ...]  A を複数バージョンと対局（既定: 直近3バージョン）
  ./rsi.py register --desc "..."      HEAD のコミットを新バージョンとして登録
  ./rsi.py ratings                    全対局から Elo を計算し RATINGS.md を更新
  ./rsi.py bench [NAME]               固定局面の探索速度 (NPS) を計測

A/B には "candidate"（未登録の作業中コード）も指定できる。
"""

import argparse
import datetime
import json
import math
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
VERSIONS = os.path.join(ROOT, "versions.json")
GAMES = os.path.join(ROOT, "results", "games.jsonl")
ENGINES = os.path.join(ROOT, "engines")
BUILD = os.path.join(ROOT, "build")
RATINGS_MD = os.path.join(ROOT, "RATINGS.md")
ANCHOR_ELO = 1000.0

# コードネーム（登録順に割り当てる。足りなくなったら追記する）
CODENAMES = [
    "ayame", "botan", "chidori", "daidai", "enoki", "fuji", "ginga", "hayate",
    "ibuki", "junpu", "kaede", "raiden", "mizuho", "nagi", "oboro", "potomak",
    "rindo", "sakura", "tsubaki", "ukon", "wakaba", "yamato", "zuiun", "akebono",
    "benibana", "chigusa", "fubuki", "gekko", "hibari", "inazuma", "kagero", "mikazuki",
]


def sh(cmd, **kw):
    print("+", " ".join(cmd), file=sys.stderr)
    return subprocess.run(cmd, check=True, **kw)


def load_versions():
    if not os.path.exists(VERSIONS):
        return []
    with open(VERSIONS) as f:
        return json.load(f)


def save_versions(vs):
    with open(VERSIONS, "w") as f:
        json.dump(vs, f, indent=2, ensure_ascii=False)
        f.write("\n")


def label(v):
    return f"v{v['number']}-{v['name']}"


def find_version(key, vs=None):
    vs = vs if vs is not None else load_versions()
    k = key.lower().strip()
    if k.startswith("v") and k[1:].isdigit():
        k = k[1:]
    for v in vs:
        if k == str(v["number"]) or k == v["name"] or k == label(v):
            return v
    if k in ("latest", "head") and vs:
        return vs[-1]
    if k == "best" and vs:
        r = compute_ratings()
        return max(vs, key=lambda v: r.get(label(v), (-1e9, 0))[0])
    sys.exit(f"unknown version: {key}")


def engine_path(lbl):
    return os.path.join(ENGINES, lbl, "shogi-rsi")


def cargo_build(src_dir, version_label):
    env = dict(os.environ)
    env["RSI_VERSION_NAME"] = version_label
    target = os.path.join(BUILD, "target")
    sh(["cargo", "build", "--release", "--quiet", "--target-dir", target], cwd=src_dir, env=env)
    return os.path.join(target, "release", "shogi-rsi")


def build_version(v):
    lbl = label(v)
    dst = engine_path(lbl)
    if os.path.exists(dst):
        return dst
    wt = os.path.join(BUILD, "wt", lbl)
    if not os.path.exists(wt):
        os.makedirs(os.path.dirname(wt), exist_ok=True)
        sh(["git", "-C", ROOT, "worktree", "add", "--detach", "--force", wt, v["commit"]])
    binary = cargo_build(wt, lbl)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copy2(binary, dst)
    return dst


def build_candidate():
    binary = cargo_build(ROOT, "candidate")
    dst = engine_path("candidate")
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copy2(binary, dst)
    return dst


def resolve_engine(key):
    """'candidate' か登録バージョン名から (ラベル, バイナリパス) を返す"""
    if key == "candidate":
        p = engine_path("candidate")
        if not os.path.exists(p):
            build_candidate()
        return "candidate", p
    v = find_version(key)
    return label(v), build_version(v)


def arena_binary():
    # 審判は常に現在のコードのアリーナを使う
    p = os.path.join(BUILD, "arena", "shogi-rsi")
    binary = cargo_build(ROOT, "arena")
    os.makedirs(os.path.dirname(p), exist_ok=True)
    shutil.copy2(binary, p)
    return p


def run_match(a, b, games, byoyomi, concurrency, seed, record=True):
    la, pa = resolve_engine(a)
    lb, pb = resolve_engine(b)
    arena = arena_binary()
    os.makedirs(os.path.dirname(GAMES), exist_ok=True)
    out = GAMES if record else os.path.join(BUILD, "scratch_games.jsonl")
    if la == "candidate" or lb == "candidate":
        # candidate の対局は正式記録とは分ける（コードが変わるため）
        out = os.path.join(ROOT, "results", "candidate_games.jsonl")
    cmd = [arena, "arena", "--engine1", pa, "--engine2", pb, "--name1", la, "--name2", lb,
           "--games", str(games), "--byoyomi", str(byoyomi), "--concurrency", str(concurrency),
           "--seed", str(seed), "--out", out]
    print("+", " ".join(cmd), file=sys.stderr)
    res = subprocess.run(cmd, check=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    print(res.stdout.strip())
    return res.stdout


def load_games(path=GAMES):
    if not os.path.exists(path):
        return []
    games = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                games.append(json.loads(line))
    return games


def compute_ratings(games=None, anchor=None):
    """Bradley-Terry（引き分けは 0.5）の最尤推定。戻り値 {label: (elo, stderr)}"""
    games = games if games is not None else load_games()
    vs = load_versions()
    if anchor is None and vs:
        anchor = label(vs[0])
    players = sorted({g["black"] for g in games} | {g["white"] for g in games})
    if not players:
        return {}
    idx = {p: i for i, p in enumerate(players)}
    n = len(players)
    # pair stats
    score = {}
    count = {}
    for g in games:
        a, b = idx[g["black"]], idx[g["white"]]
        w = g["winner"]
        sa = 0.5 if w == "" else (1.0 if w == g["black"] else 0.0)
        for (x, y, s) in ((a, b, sa), (b, a, 1.0 - sa)):
            score[(x, y)] = score.get((x, y), 0.0) + s
            count[(x, y)] = count.get((x, y), 0) + 1
    # 全勝/全敗で発散しないよう、各組に 0.5 勝 0.5 敗の仮想対局を足す（事前分布）
    for (x, y) in list(count.keys()):
        score[(x, y)] += 0.5
        count[(x, y)] += 1
    r = [0.0] * n  # natural log-strength
    for _ in range(2000):
        maxd = 0.0
        for i in range(n):
            wins = sum(s for (x, y), s in score.items() if x == i)
            denom = sum(c / (math.exp(r[i]) + math.exp(r[y])) for (x, y), c in count.items() if x == i)
            if denom <= 0 or wins <= 0:
                continue
            new = math.log(wins / denom)
            maxd = max(maxd, abs(new - r[i]))
            r[i] = new
        # 正規化
        m = sum(r) / n
        r = [v - m for v in r]
        if maxd < 1e-9:
            break
    k = 400.0 / math.log(10)
    elo = [v * k for v in r]
    # 標準誤差（フィッシャー情報量の対角近似）
    err = []
    for i in range(n):
        info = 0.0
        for (x, y), c in count.items():
            if x == i:
                p = 1.0 / (1.0 + math.exp(r[y] - r[i]))
                info += c * p * (1 - p)
        err.append(k / math.sqrt(info) if info > 0 else float("inf"))
    shift = ANCHOR_ELO - (elo[idx[anchor]] if anchor in idx else 0.0)
    return {p: (elo[idx[p]] + shift, err[idx[p]]) for p in players}


def write_ratings_md(ratings):
    vs = load_versions()
    games = load_games()
    ngames = {}
    for g in games:
        for p in (g["black"], g["white"]):
            ngames[p] = ngames.get(p, 0) + 1
    lines = [
        "# レーティング表",
        "",
        f"`./rsi.py ratings` で自動生成。基準: {label(vs[0]) if vs else '-'} = {ANCHOR_ELO:.0f}。"
        "全対局の Bradley-Terry 最尤推定（±は概算の標準誤差）。",
        "",
        "| バージョン | Elo | ± | 対局数 | 説明 |",
        "|---|---:|---:|---:|---|",
    ]
    for v in vs:
        lbl = label(v)
        if lbl in ratings:
            e, s = ratings[lbl]
            lines.append(f"| {lbl} | {e:.0f} | {s:.0f} | {ngames.get(lbl, 0)} | {v['description']} |")
        else:
            lines.append(f"| {lbl} | - | - | 0 | {v['description']} |")
    lines.append("")
    with open(RATINGS_MD, "w") as f:
        f.write("\n".join(lines))


def cmd_list(args):
    vs = load_versions()
    r = compute_ratings()
    for v in vs:
        lbl = label(v)
        e = r.get(lbl)
        es = f"{e[0]:7.0f} ±{e[1]:4.0f}" if e else "      -      "
        print(f"{lbl:<20} elo {es}  {v['commit'][:10]}  {v['description']}")


def cmd_build(args):
    vs = load_versions()
    targets = vs if args.name in (None, "all") else [find_version(args.name, vs)]
    for v in targets:
        print(build_version(v))


def cmd_candidate(args):
    print(build_candidate())


def cmd_match(args):
    if args.a == "candidate" or args.b == "candidate":
        build_candidate()
    run_match(args.a, args.b, args.games, args.byoyomi, args.concurrency, args.seed)
    if args.a != "candidate" and args.b != "candidate":
        write_ratings_md(compute_ratings())


def cmd_gauntlet(args):
    vs = load_versions()
    if args.a == "candidate":
        build_candidate()
        me = "candidate"
    else:
        me = label(find_version(args.a, vs))
    opps = args.vs
    if not opps:
        opps = [label(v) for v in vs if label(v) != me][-3:]
    for o in opps:
        run_match(args.a, o, args.games, args.byoyomi, args.concurrency, args.seed)
    if me != "candidate":
        write_ratings_md(compute_ratings())


def cmd_register(args):
    vs = load_versions()
    status = subprocess.run(["git", "-C", ROOT, "status", "--porcelain", "--", "src", "Cargo.toml"],
                            check=True, stdout=subprocess.PIPE, text=True).stdout.strip()
    if status and not args.force:
        sys.exit("src/ に未コミットの変更があります。先にコミットしてください (--force で無視)")
    commit = subprocess.run(["git", "-C", ROOT, "rev-parse", "HEAD"], check=True,
                            stdout=subprocess.PIPE, text=True).stdout.strip()
    for v in vs:
        if v["commit"] == commit:
            sys.exit(f"このコミットは既に {label(v)} として登録済みです")
    number = (vs[-1]["number"] + 1) if vs else 1
    used = {v["name"] for v in vs}
    name = args.name or next(n for n in CODENAMES if n not in used)
    v = {
        "number": number,
        "name": name,
        "commit": commit,
        "parent": label(vs[-1]) if vs else None,
        "created": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "description": args.desc,
    }
    vs.append(v)
    save_versions(vs)
    print(f"registered {label(v)} at {commit[:10]}")
    build_version(v)
    write_ratings_md(compute_ratings())


def cmd_ratings(args):
    r = compute_ratings()
    write_ratings_md(r)
    for p, (e, s) in sorted(r.items(), key=lambda kv: -kv[1][0]):
        print(f"{p:<20} {e:7.0f} ±{s:4.0f}")


def cmd_bench(args):
    if args.name in (None, "candidate"):
        path = build_candidate()
    else:
        path = build_version(find_version(args.name))
    sh([path, "bench", "--depth", str(args.depth)])


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("list"); s.set_defaults(f=cmd_list)
    s = sub.add_parser("build"); s.add_argument("name", nargs="?"); s.set_defaults(f=cmd_build)
    s = sub.add_parser("candidate"); s.set_defaults(f=cmd_candidate)
    for nm, fn in (("match", cmd_match), ("gauntlet", cmd_gauntlet)):
        s = sub.add_parser(nm)
        s.add_argument("a")
        if nm == "match":
            s.add_argument("b")
        else:
            s.add_argument("--vs", nargs="*")
        s.add_argument("--games", type=int, default=100)
        s.add_argument("--byoyomi", type=int, default=100, help="1手あたりの思考時間(ms)")
        s.add_argument("--concurrency", type=int, default=max(1, (os.cpu_count() or 2)))
        s.add_argument("--seed", type=int, default=None)
        s.set_defaults(f=fn)
    s = sub.add_parser("register")
    s.add_argument("--desc", required=True)
    s.add_argument("--name", help="コードネームを手動指定")
    s.add_argument("--force", action="store_true")
    s.set_defaults(f=cmd_register)
    s = sub.add_parser("ratings"); s.set_defaults(f=cmd_ratings)
    s = sub.add_parser("bench"); s.add_argument("name", nargs="?"); s.add_argument("--depth", type=int, default=6); s.set_defaults(f=cmd_bench)
    args = p.parse_args()
    if getattr(args, "seed", 0) is None:
        # 対局ごとに違う序盤になるよう、既存対局数から seed を決める
        args.seed = len(load_games()) + len(load_games(os.path.join(ROOT, "results", "candidate_games.jsonl"))) + 1
    args.f(args)


if __name__ == "__main__":
    main()
