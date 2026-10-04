# shogi-rsi: 改善タスクの手順書

このリポジトリは将棋AIを継続的に改善し、どこまで強くなるかを記録するプロジェクト。
定期実行される改善タスクは以下の手順に従うこと。

## 1サイクルの手順

1. 現状確認
   - `./rsi.py list` で最新バージョンと Elo を確認する。
   - `docs/IDEAS.md` を読み、試した案・未着手の案を把握する。
2. 改善案を 1 つ選んで実装する（小さく、計測可能な単位で）。
   - 高速化: `./target/release/shogi-rsi bench` の NPS / 同深さのノード数で比較。
   - 探索・評価: 対局で比較。
   - ルール部分を触ったら必ず perft で検証する:
     - `shogi-rsi perft 5` → 19861490
     - `shogi-rsi perft 3 "l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w RGgsn5p 1"` → 4809015
3. 候補を試験対局する:
   - `./rsi.py match candidate <最新版> --games 200 --byoyomi 100`
   - 目安: 勝率 55% 以上（+35 Elo 程度）かつ 200 局以上なら採用候補。
     差が小さい場合は局数を増やす（Elo ±20 の判定には数百局必要）。
4. 採用するならコミットして登録する:
   - `git commit` → `./rsi.py register --desc "変更内容の要約"`
   - 登録後、正式記録として `./rsi.py gauntlet <新版> --games 100` を流し、`RATINGS.md` を更新する。
   - `versions.json`, `results/games.jsonl`, `RATINGS.md` をコミットする。
5. 不採用なら `docs/IDEAS.md` に「試した・結果・不採用理由」を残して変更を戻す。

## ルール

- バージョンは番号＋コードネームで呼ぶ（`v3-chidori`）。ハッシュでは呼ばない。
- 登録済みバージョンのコミットは書き換えない（rebase/force-push 禁止）。旧版は worktree からビルドされる。
- 対局条件（byoyomi など）を変えたときは、比較は同条件の対局同士で行うこと。
- アリーナ（審判）のコードは src/arena.rs と src/position.rs を共有している。審判のルールを変更したら
  その旨を docs/IDEAS.md に記録する。
- テストコマンド: `cargo build --release && ./target/release/shogi-rsi perft 4`（719731）
