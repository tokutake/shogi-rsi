# Web版のビルドとオフライン利用

[READMEに戻る](../README.md)

## ビルド

ブラウザ内で同じRustエンジンを動かすWebAssembly版をビルドします（外部クレート不要）。
Rust / CargoとPython 3が必要です。

```sh
rustup target add wasm32-unknown-unknown  # 初回のみ
sh scripts/build-web.sh                 # dist/ に静的アプリを生成
```

PCで確認する場合は、ビルド後に `cargo run --release -- web` で開けます。
WebAssembly版をビルドしていない場合はPC側のAIで動作します。

## 配信

GitHub Pagesへの公開は `.github/workflows/pages.yml` で行います。
リポジトリの **Settings → Pages → Build and deployment → Source** を **GitHub Actions** に設定してください。
mainへのpush時にビルドとテストを行い、成功した `dist/` を公開します。
PRではビルドとテストだけを行います。main上での手動実行（Actions → Build and deploy web app → Run workflow）も可能です。

公開後のURLは https://tokutake.github.io/shogi-rsi/ です。
公開状況はActionsの実行結果で確認できます。
`dist/` は別のHTTPS対応の静的ホスティングにも配置できます。サブディレクトリへの配置にも対応しています。
公開サーバーにPC版のAPIを公開する必要はありません。

## iPhone・iPadでオフライン利用

iPhone・iPadへのインストールはHTTPS URLで行ってください。

1. iPhoneのSafariで配信先のHTTPS URLを開きます。
2. 「端末内AI」「オフライン準備完了」の表示を確認します。
3. Safariの共有メニューから「ホーム画面に追加」を選びます。
4. ホーム画面から一度開き、「オフライン準備完了」を確認してから外出します。

以後はPCも通信も不要です。AIの思考はWeb Workerで実行し、思考時間は端末の速度に合わせて選べます。
先後選択・成り・持ち駒・待った・投了に対応しています。指し手・先後・思考時間・投了状態を端末に保存し、
AI思考中に閉じた場合は保存済みの手からAIの思考を再開します。
保存はURL（オリジン）ごとで、Safariとホーム画面アプリで別になる場合があります。
ブラウザのサイトデータ削除やOSによる保存領域の削除後は、通信できる場所で再度開いてください。

## 更新

ビルド時に配布ファイルの内容からオフラインキャッシュのバージョンを自動生成します。
更新後、通信できる場所でアプリを開くと新版が保存され、アプリを閉じて開き直すと反映されます。

## 検証

検証は `cargo test --locked` と、WebAssembly版ビルド後の `node tests/web-engine.mjs`、
`node tests/web-offline.mjs` で行えます。実際のWasmとWorkerを使い、合法手・成り・打ち・千日手・
思考時間・対局の復元を確認し、Pagesのサブディレクトリでのオフラインキャッシュと更新も確認します。

