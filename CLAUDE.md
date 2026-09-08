# CLAUDE — AppPromoVideo

既存アプリのリポジトリと UI スナップショットから、**動画生成 AI に貼れるプロンプト一式と参照画像**を出す
デスクトップツール。LLM はローカル CLI (`claude -p` 等) をサブプロセスで叩く。

## 北極星

**「貼れば動く出力」。** 訴求文の巧さでなく、動画生成 UI にそのまま貼れる粒度と、スナップショットに
寄った参照画像の一貫性が売り。**動画そのものは作らない (スコープ外を守る)。**

## アーキテクチャ

> LLM は書き、Rust は検め、CLI が走査し、モデルは舞台を描き、Rust が画面を貼る。

- **`crates/promo_core`** (純関数): data_contract の名詞 / schemars で JSON Schema を機械生成
  (`--json-schema` に渡す。手書き禁止) / RepoBrief の組み立て / プロンプト本文 / export。
- **`crates/cli_runner`** (tokio::process): argv 組み立てと stream-json 解析は純粋関数で PoC。
  spawn / 本文の運搬 (stdin か一時ファイル。**argv には載せない**) / 行ストリーム / timeout /
  cancel (**子孫ごと kill**: Windows Job Object・Unix pgid)。**LLM の HTTP は禁止。**
  許可ツールは Read / Glob / Grep のみで `--add-dir <repo>` とセット。**cwd は app の作業ディレクトリ**であって
  対象リポジトリではない (`-p` は cwd の hook / MCP を無確認で実行する)。
- **`crates/image_gen`**: Kataribe (`D:/Github/Kataribe/app/src-tauri/src/image_gen.rs`, specs 24-27) の移植。
  `ImageGenerator` trait + OpenAI Images / Gemini / ComfyUI。**`provider.rs` は写しで不触。**
- **`crates/pipeline`**: IO と結線。RepoBrief の収集 / 2 タスクの実行 / 検査ループ / export / `promo` CLI。
- **`app/`**: Tauri 2 + Vue 3。HTTP とプロセスは全部 backend。進捗は Tauri event で push。
  設定は「LLM (CLI 認証委任、キー欄なし)」と「画像生成 (キー必須。ComfyUI は無キー)」を別セクションに。

## 掟（Mandate）

- **データ・ファースト**: コードの前に `data_contract.yaml`（名詞）を凍結する。
- **PoC 必須**: バグ修正・新機能は Red→Green をテストで実証してから完了。推測修正は不可。
  **Red を観測していない時はそう申告する。**
- **リサーチ先行**: 実装前に三点測量（既存コード grep / 仕様 / 記憶）。移植元は Kataribe を先に読む。
- **撤去したら grep**: 機構・enum 値・フィールドを撤去したら、その名前で**全台帳**（下の一覧）を grep し
  追従漏れを回収してから完了。機能の着地時は「どの台帳へ書いたか」を数える。
  `history.md` だけは**追記専用**で回収の対象外 — 過去のエントリはその時点の事実として書き換えない。
- **ユーザーの変更を上書きしない**: 会話の外でコードや設定が変わっていることがある。驚くような差分を
  見つけたら意図的だと考え、上書きせず確認する。
- **失敗時に謝罪しない**: 観察 → 仮説棄却 → 次の検証ステップの三段で進む。
- **層分け**: 仕様・契約はこの file 台帳に書く。Memoria (session `AppPromoVideo`) には蒸留した教訓・判断だけ。
- **入力を指す語を画像プロンプトに書かない**: 「スクリーンショットに従え」「参照画像のように」は
  被写体として描かれる (Kataribe #85)。
- **`cargo fmt --all` を打たない**: `rustfmt.toml` が無く、既定の 100 桁でリポジトリ全体が整形し直される
  (実測 2026-09-08、`provider.rs` まで書き換わった)。整形するならファイル単位で。

## 台帳 (どこに何が書いてあるか)

| file | 中身 | 見るとき |
|---|---|---|
| `data_contract.yaml` | **名詞と契約**。型・上限・enum・不変条件 | 実装の前。ここが正本 |
| `specs/NN_*.md` | **決定と Phase**。rev ごとの判断と理由、接地の限界 | なぜそうなっているかを知りたいとき |
| `failures.md` | **罠台帳**。症状 → 真因 → 処方 → 一般化 | 同じ形の問題に当たったとき |
| `history.md` | **作業ログ**。いつ何が起きて何が分かったか | 経緯をたどるとき。**読まなくても現状は分かる** |
| `CLAUDE.md` | この file。北極星 / アーキテクチャ / 掟 / 現状 / 台帳の地図 | 毎回 |

## 主要コマンド

```bash
cargo test --workspace                    # PoC (promo_core + cli_runner + pipeline + image_gen)
cargo clippy --workspace --all-targets    # lint
cargo run -q -p pipeline --bin promo -- brief <repo>   # RepoBrief を見る (LLM ゼロ)
cargo run -q -p pipeline --bin promo -- run <repo> --concept "..." --snapshot <png> --model sonnet \
  --plate perspective|frontal --images gemini --out <dir>          # live 通し
cargo run -q -p pipeline --bin promo -- images <pkg>/promo.json --images gemini --snapshot <png>  # 参照画像だけ
cd app && RUSTC_WRAPPER= npm run tauri dev   # GUI
cd app && npx vitest run && npm run build    # frontend の単体テストと型検査
cd app/src-tauri && cargo test && cargo clippy   # backend (独立 workspace)
```

## 現状 (2026-09-08)

- **spec 01 は Phase 0〜E 着地、rev6 まで反映済み。** crates 120 green / vitest 10 / backend 3 green・clippy clean。
- コミットは Initial `a75c3bc` の上に 8 本。working tree clean、**未 push**。
- 通し (解析 → 構成 → 参照画像 → 合成) は **3 リポジトリで live 成功** (Kataribe / Verificator / AppPromoVideo 自身)。
- CLI の認証は現行 `claude` なら子セッションからも通る。落ちる時は GUI 設定「OAuth ログインを使う」ON。

**開いている判断**

1. **`PlateMode` の既定** — perspective (面を傾ける) と frontal (正対固定) を両方実装済み。
   等倍 1890×1080 の対を MiniMax i2v に通した結果待ち。
2. **見出し焼き込みの既定** — 機構は opt-in・既定 OFF で入っている。ユーザーのアンケート待ち。

**次の候補**: 傾きと可読性の境目 (角度を上げるとどこで文字が壊れるか) / mood カットのモチーフ一貫性 /
MiniMax 向け `motion_prompt` の粒度 / ComfyUI と OpenAI の live / Unix の `tree_kill` / `--add-dir` 外 Read の拒否確認。

## 再開の手順

1. `cargo test --workspace` (120 green) と `cd app && npx vitest run` (10 green) で足場を確認。
2. `git log --oneline -5` で直前の着地を見る。詳しい経緯は `history.md`。
3. 上の「開いている判断」に答えが来ていないか確認してから着手する。
