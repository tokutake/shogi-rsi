# shogi-rsi

将棋AIを「改善タスクを定期的に回して、どこまで強くなるか」を試すためのプロジェクト。

- エンジン本体: Rust（外部クレートなし）。USI プロトコル対応なので将棋所/ShogiGUI などからも使える。
- バージョン管理: 改善版は **番号 + コードネーム**（例: `v3-chidori`）で登録する。git のハッシュを覚える必要はない。
  「v3 より Elo が 300 高い AI を作って」のように指示できる。`v3` / `3` / `chidori` のどれでも指定可能。
- 強さの計測: 自前の対局アリーナで旧バージョンと対局させ、全対局から Elo を最尤推定する（[RATINGS.md](RATINGS.md)）。

## クイックスタート

```sh
cargo build --release                 # エンジンのビルド
./target/release/shogi-rsi            # USI エンジンとして起動
./target/release/shogi-rsi perft 5    # 合法手生成の検証（初期局面 5手 = 19861490）
./target/release/shogi-rsi bench      # 固定局面探索の速度計測（NPS）

./rsi.py list                          # 登録バージョンとレーティング
./rsi.py match v2 v1 --games 100       # 対局して記録、RATINGS.md 更新
./rsi.py match candidate v2 --games 100  # 作業中のコードを試す（記録は別ファイル）
./rsi.py register --desc "何を変えたか"   # HEAD を次のバージョンとして登録
```

## ディレクトリ

| パス | 内容 |
|---|---|
| `src/position.rs` | 盤面・合法手生成・SFEN・千日手 |
| `src/eval.rs` | 評価関数 |
| `src/search.rs` | 探索（反復深化 αβ / PVS / null move / LMR / 静止探索 / 置換表） |
| `src/usi.rs` | USI プロトコル |
| `src/arena.rs` | 対局アリーナ（審判つき、並列対局、JSON Lines 出力） |
| `rsi.py` | バージョン登録・旧版ビルド・対局・Elo 計算 |
| `versions.json` | バージョン台帳（番号・コードネーム・コミット・説明） |
| `results/games.jsonl` | 正式な対局記録（登録済みバージョン同士）。Elo はここから計算 |
| `results/candidate_games.jsonl` | 未登録コードの試験対局（参考記録） |
| `RATINGS.md` | 自動生成のレーティング表 |
| `CLAUDE.md` | 定期改善タスク（エージェント）向けの手順書 |
| `docs/WORKLOG.md` | 作業ログ |

## 対局ルール（アリーナ）

- 1手あたり秒読み（既定 100ms）、持ち時間なし。
- 序盤の多様性: 駒を取らない手をランダムに 6 手指した局面から開始し、同じ局面で先後を入れ替えて 2 局。
- 終局: 詰み（手がない側の負け）、投了、反則手（負け）、時間切れ（秒読み+2秒超過で負け）、
  千日手（同一局面4回で引き分け、連続王手の千日手は未判定）、320 手で引き分け。
- 入玉宣言は未実装。
