# レーティング表

`./rsi.py ratings` で自動生成。基準: v1-ayame = 1000。全対局の Bradley-Terry 最尤推定（±は概算の標準誤差）。

| バージョン | Elo | ± | 対局数 | 説明 |
|---|---:|---:|---:|---|
| v1-ayame | - | - | 0 | 初版: 駒得のみの評価 + αβ/静止探索/置換表/キラー |
| v2-botan | - | - | 0 | 探索強化: PVS / null move / LMR / history |
