# shogi-rsi

将棋AIの改善・対局・強さの計測を繰り返すプロジェクトです。
Rust製のエンジンは外部クレート不要で、ブラウザ対局とUSIプロトコルに対応しています。
登録バージョンごとの強さは [RATINGS.md](RATINGS.md) を参照してください。

## ブラウザで対局

Rust / Cargoを用意して起動します。

```sh
cargo run --release -- web
```

[http://127.0.0.1:8080/](http://127.0.0.1:8080/) を開き、駒と移動先をクリックして指します。
持ち駒・成り・先後選択・思考時間の変更・待った・投了に対応しています。
後手で始めるには、後手を選んで「新しい対局」を押してください。
対局はブラウザに自動保存され、再読み込み後も再開できます。

サーバーは起動したPCからのみ接続できます。終了は Ctrl+C、ポートの変更は次のコマンドです。

```sh
cargo run --release -- web --port 8081
```

iPhone・iPadでは、WebAssembly版をHTTPSで配信して利用できます。
ホーム画面に追加すると、PCも通信も使わず対局できます。
ビルド・公開・オフライン利用の手順は [Web版のガイド](docs/WEB.md) を参照してください。

## USIエンジンとして使う

```sh
cargo build --release
./target/release/shogi-rsi
```

将棋所やShogiGUIなどのUSI対応GUIに `target/release/shogi-rsi` を登録して使えます。

## 改善と計測

バージョン管理と対局にはPython 3とGitを使います。
登録バージョンは番号・コードネームで指定できます（例: `v3` / `3` / `chidori`）。

```sh
./rsi.py list                            # 登録バージョンとレーティング
./rsi.py match v2 v1 --games 100          # 対局し、正式記録とRATINGS.mdを更新
./rsi.py candidate                       # 作業中のコードをビルド（変更後は再実行）
./rsi.py match candidate v2 --games 100   # 未登録コードの試験対局
./rsi.py register --desc "何を変えたか"    # コミット済みのHEADを次のバージョンとして登録

./target/release/shogi-rsi perft 5        # 合法手生成の検証（初期局面: 19861490）
./target/release/shogi-rsi bench          # 固定局面の探索速度（NPS）
```

正式な対局記録は `results/games.jsonl` に保存し、全対局からEloを最尤推定します。
未登録コードの対局は `results/candidate_games.jsonl` に分けて保存します。
改善手順は [CLAUDE.md](CLAUDE.md)、試行記録は [docs/IDEAS.md](docs/IDEAS.md) を参照してください。

### 評価関数の調整

対局記録から局面と勝敗を集めてTexel tuningを行い、`src/params.rs` を更新します。

```sh
./target/release/shogi-rsi tune \
    --data results/games.jsonl --data results/candidate_games.jsonl \
    --data results/selfplay.jsonl --l2 1e-7
```

自己対局データは、ビルド済みのエンジン同士をアリーナで対局させて生成できます。

```sh
./target/release/shogi-rsi arena \
    --engine1 target/release/shogi-rsi --engine2 target/release/shogi-rsi \
    --name1 a --name2 b --games 400 --byoyomi 50 --concurrency 4 \
    --seed 123 --out results/selfplay.jsonl
```

## テスト

```sh
cargo test --locked

# WebAssembly版の検証（RustのWasmターゲット、Python 3、Node.jsが必要）
rustup target add wasm32-unknown-unknown  # 初回のみ
sh scripts/build-web.sh
node tests/web-engine.mjs
node tests/web-offline.mjs
```

## 主なファイル

| パス | 内容 |
|---|---|
| `src/position.rs` | 盤面・合法手生成・SFEN・千日手 |
| `src/eval.rs` / `src/params.rs` | 評価関数・評価パラメータ |
| `src/search.rs` | 探索・置換表 |
| `src/usi.rs` | USIプロトコル |
| `src/game.rs` / `src/web.rs` | 対局処理・ローカルWebサーバー |
| `web/` / `src/lib.rs` | ブラウザUI・WebAssemblyエンジン |
| `src/tune.rs` / `src/label.rs` / `src/nn_train.rs` | 評価パラメータ調整・教師データ生成・NN学習 |
| `src/arena.rs` / `rsi.py` | 対局アリーナ・バージョン管理・Elo計算 |
| `versions.json` / `results/` | バージョン台帳・対局記録・学習データ |
| `docs/` | Web版ガイド・改善アイデア・作業ログ |

## 対局ルール

ブラウザ対局とアリーナは、詰み・投了・千日手（同一局面4回）・320手で終局します。
千日手と320手は引き分けです。連続王手の千日手と入玉宣言は未対応です。

アリーナでは1手ごとの秒読み（既定100ms）で対局し、反則手や秒読みを2秒超過した場合は負けです。
序盤は駒を取らないランダムな6手で変化を付け、同じ局面で先後を入れ替えて2局ずつ指します。
