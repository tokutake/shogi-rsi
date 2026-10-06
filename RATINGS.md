# レーティング表

`./rsi.py ratings` で自動生成。基準: v1-ayame = 1000。全対局の Bradley-Terry 最尤推定（±は概算の標準誤差）。

| バージョン | Elo | ± | 対局数 | 説明 |
|---|---:|---:|---:|---|
| v1-ayame | 1000 | 34 | 140 | 初版: 駒得のみの評価 + αβ/静止探索/置換表/キラー |
| v2-botan | 1041 | 28 | 200 | 探索強化: PVS / null move / LMR / history |
| v3-chidori | 1227 | 24 | 240 | 評価改善: 玉との距離による守り駒/攻め駒の配置評価 + 玉の段 |
| v4-daidai | 1272 | 22 | 340 | 枝刈り追加: reverse futility / futility / 静止探索の delta・簡易SEE |
| v5-enoki | 1372 | 19 | 420 | 評価パラメータを自己対局データで自動調整(Texel) + 利き計算のテーブル化で約10%高速化 |
| v6-fuji | 1453 | 26 | 200 | 探索: Late move pruning + 対数式LMR + Aspiration window |
| v7-ginga | 1624 | 26 | 300 | 評価: PST + 玉相対の駒位置(KP)特徴量を、探索スコア(深さ5)教師の回帰で学習 |
