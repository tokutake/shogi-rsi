# shogi-rsi

将棋AIを「改善タスクを定期的に回して、どこまで強くなるか」を試すためのプロジェクト。

- エンジン本体: Rust（外部クレートなし）。USI プロトコル対応なので将棋所/ShogiGUI などからも使える。
- バージョン管理: 改善版は **番号 + コードネーム**（例: `v3-chidori`）で登録する。git のハッシュを覚える必要はない。
  「v3 より Elo が 300 高い AI を作って」のように指示できる。`v3` / `3` / `chidori` のどれでも指定可能。
- 強さの計測: 自前の対局アリーナで旧バージョンと対局させ、全対局から Elo を最尤推定する（[RATINGS.md](RATINGS.md)）。

## クイックスタート

### ブラウザで人間 vs AI

```sh
cargo run --release -- web
```

ブラウザで http://127.0.0.1:8080/ を開きます。駒と移動先をクリックして指せます。
持ち駒を打つ・成り選択・先後選択・AIの思考時間変更・待った・投了に対応しています。
後手を選んで「新しい対局」を押すとAIから指します。終了はターミナルで Ctrl+C。
ポートを変更する場合は `cargo run --release -- web --port 8081`。
サーバーは自分のPCからのみ接続できます。対局は端末に自動保存され、再読み込み後も再開できます。
終局判定はアリーナと同様（千日手・320手で引き分け、連続王手の千日手と入玉宣言は未対応）。

### iPadで遊ぶ

PCとiPadを同じWi-Fiに接続し、PCでLAN接続を有効にして起動します。

```sh
cargo run --release -- web --lan
```

iPadのSafariで `http://<PCのLAN IPアドレス>:8080/` を開きます。
例えばPCのIPが `192.168.1.20` なら `http://192.168.1.20:8080/` です。
Macでは「システム設定 → Wi-Fi → 接続中のネットワークの詳細 → TCP/IP」でIPを確認できます。
ファイアウォールの接続許可が表示されたら許可してください。
駒と移動先を順にタップして指します。縦向き・横向きに対応しています。
AIはPCで動作するので、対局中はPCとサーバーを起動したままにしてください。

`--lan` を付けた場合のみLAN内の端末から接続できます（認証はありません）。
自宅など信頼できるネットワークで使い、ルーターのポート転送は不要です。
接続できない場合は同じWi-Fiか、ゲストWi-Fiの端末間通信制限やPCのファイアウォールを確認してください。
ポート変更は `cargo run --release -- web --lan --port 8081`。
端末ごとに別の対局になり、Safariの再読み込み後も保存した対局から再開できます。

### iPhoneで外出先・オフラインでも遊ぶ

ブラウザ内で同じRustエンジンを動かすWebAssembly版をビルドします（外部クレート不要）。

```sh
rustup target add wasm32-unknown-unknown  # 初回のみ
sh scripts/build-web.sh                 # dist/ に静的アプリを生成
```

GitHub Pagesへの公開は `.github/workflows/pages.yml` で行います。
リポジトリの **Settings → Pages → Build and deployment → Source** を **GitHub Actions** に設定してください。
mainへのpush時にビルドとテストを行い、成功した `dist/` を公開します。
PRではビルドとテストだけを行います。main上での手動実行（Actions → Build and deploy web app → Run workflow）も可能です。

公開後のURLは https://tokutake.github.io/shogi-rsi/ です。
初回はマージとPages設定が必要で、公開状況はActionsの実行結果で確認できます。
`dist/` は別のHTTPS対応の静的ホスティングにも配置できます。サブディレクトリへの配置にも対応しています。
公開サーバーにPC版のAPIを公開する必要はありません。

1. iPhoneのSafariで配信先のHTTPS URLを開きます。
2. 「端末内AI」「オフライン準備完了」の表示を確認します。
3. Safariの共有メニューから「ホーム画面に追加」を選びます。
4. ホーム画面から一度開き、「オフライン準備完了」を確認してから外出します。

以後はPCも通信も不要です。AIの思考はWeb Workerで実行し、思考時間は端末の速度に合わせて選べます。
先後選択・成り・持ち駒・待った・投了に対応しています。指し手・先後・思考時間・投了状態を端末に保存し、
AI思考中に閉じた場合は保存済みの手からAIの思考を再開します。
保存はURL（オリジン）ごとで、Safariとホーム画面アプリで別になる場合があります。
ブラウザのサイトデータ削除やOSによる保存領域の削除後は、通信できる場所で再度開いてください。

PCで確認する場合は、ビルド後に `cargo run --release -- web` で開けます。
`--lan` を使えば同じWi-FiのiPhoneでも端末内AIを試せますが、LAN IPのHTTP URLでは
オフライン保存は使えません。外出用のインストールはHTTPS URLで行ってください。
WebAssembly版をビルドしていない場合は従来のPC接続のAIで動作します。
ビルド時に配布ファイルの内容からオフラインキャッシュのバージョンを自動生成します。
更新後、通信できる場所でアプリを開くと新版が保存され、アプリを閉じて開き直すと反映されます。

検証は `cargo test --locked` と、WebAssembly版ビルド後の `node tests/web-engine.mjs`、
`node tests/web-offline.mjs` で行えます。実際のWasmとWorkerを使い、合法手・成り・打ち・千日手・
思考時間・対局の復元を確認し、Pagesのサブディレクトリでのオフラインキャッシュと更新も確認します。

### エンジン・計測

```sh
cargo build --release                 # エンジンのビルド
./target/release/shogi-rsi            # USI エンジンとして起動
./target/release/shogi-rsi perft 5    # 合法手生成の検証（初期局面 5手 = 19861490）
./target/release/shogi-rsi bench      # 固定局面探索の速度計測（NPS）

./rsi.py list                          # 登録バージョンとレーティング
./rsi.py match v2 v1 --games 100       # 対局して記録、RATINGS.md 更新
./rsi.py match candidate v2 --games 100  # 作業中のコードを試す（記録は別ファイル）
./rsi.py register --desc "何を変えたか"   # HEAD を次のバージョンとして登録

# 評価関数の自動調整（対局記録の全指し手から局面と勝敗を集めて Texel tuning、src/params.rs を上書き）
./target/release/shogi-rsi tune --data results/games.jsonl --data results/candidate_games.jsonl --data results/selfplay.jsonl --l2 1e-7

# 自己対局データの生成（tune 用）
./build/arena/shogi-rsi arena --engine1 engines/v6-fuji/shogi-rsi --engine2 engines/v6-fuji/shogi-rsi \
    --name1 a --name2 b --games 400 --byoyomi 50 --concurrency 4 --seed 123 --out results/selfplay.jsonl
```

## 現在の到達点

[RATINGS.md](RATINGS.md) を参照。土台構築セッション（約1時間）で v1 → v6 で約 +520 Elo（v1-ayame = 1000 → v6-fuji ≈ 1520）。

## ディレクトリ

| パス | 内容 |
|---|---|
| `src/position.rs` | 盤面・合法手生成・SFEN・千日手 |
| `src/eval.rs` | 評価関数 |
| `src/search.rs` | 探索（反復深化 αβ / PVS / null move / LMR / 静止探索 / 置換表） |
| `src/usi.rs` | USI プロトコル |
| `src/params.rs` | 評価パラメータ（`shogi-rsi tune` が生成） |
| `src/tune.rs` | Texel tuning |
| `src/arena.rs` | 対局アリーナ（審判つき、並列対局、JSON Lines 出力） |
| `rsi.py` | バージョン登録・旧版ビルド・対局・Elo 計算 |
| `versions.json` | バージョン台帳（番号・コードネーム・コミット・説明） |
| `results/games.jsonl` | 正式な対局記録（登録済みバージョン同士）。Elo はここから計算 |
| `results/candidate_games.jsonl` | 未登録コードの試験対局（参考記録、tune の学習データにも使う） |
| `results/selfplay.jsonl` | 自己対局（tune の学習データ） |
| `docs/IDEAS.md` | 改善アイデアと試行記録（不採用案のパッチは docs/patches/） |
| `RATINGS.md` | 自動生成のレーティング表 |
| `CLAUDE.md` | 定期改善タスク（エージェント）向けの手順書 |
| `docs/WORKLOG.md` | 作業ログ |

## 対局ルール（アリーナ）

- 1手あたり秒読み（既定 100ms）、持ち時間なし。
- 序盤の多様性: 駒を取らない手をランダムに 6 手指した局面から開始し、同じ局面で先後を入れ替えて 2 局。
- 終局: 詰み（手がない側の負け）、投了、反則手（負け）、時間切れ（秒読み+2秒超過で負け）、
  千日手（同一局面4回で引き分け、連続王手の千日手は未判定）、320 手で引き分け。
- 入玉宣言は未実装。
