# CLAUDE — AppPromoVideo

既存アプリのリポジトリと UI スナップショットから、**動画生成 AI (Veo / Sora) に貼れるプロンプト一式と
参照画像**を出すデスクトップツール。LLM はローカル CLI (claude -p 等) をサブプロセスで叩く。

## 北極星

**「貼れば動く出力」。** 訴求文の巧さでなく、動画生成 UI にそのまま貼れる粒度と、スナップショットに
寄った参照画像の一貫性が売り。動画そのものは作らない (スコープ外を守る)。

## アーキテクチャ

> LLM は書き、Rust は検め、CLI が走査し、モデルは舞台を描き、Rust が画面を貼る。

- **`crates/promo_core`** (純関数): data_contract の名詞 / schemars で JSON Schema を機械生成
  (`--json-schema` に渡す。手書き禁止) / RepoBrief の組み立て / プロンプト本文 / export。
- **`crates/cli_runner`** (tokio::process): argv 組み立てと stream-json 解析は純粋関数で PoC。
  spawn / 本文の運搬 (stdin か一時ファイル。**argv には載せない**) / stdout・stderr 行ストリーム / timeout /
  cancel (**子孫ごと kill**: Windows Job Object・Unix pgid)。**LLM の HTTP は禁止。**
  許可ツールは Read / Glob / Grep のみで `--add-dir <repo>` とセット。**cwd は app の作業ディレクトリ**であって
  対象リポジトリではない (`-p` は cwd の hook / MCP を無確認で実行する)。
- **`crates/image_gen`**: Kataribe (`D:/Github/Kataribe/app/src-tauri/src/image_gen.rs`, specs 24-27) の移植 (Tauri 非依存を確認済み)。
  `ImageGenerator` trait + OpenAI Images / Gemini / ComfyUI (HTTP ポーリング + `/queue` 併読)。参照画像 = スナップショット。
- **`crates/pipeline`** (Phase B): IO と結線。RepoBrief の収集 (FS) / 2 タスクの実行 / 検査ループ / export。
- **`app/`**: Tauri 2 + Vue 3。HTTP とプロセスは全部 backend。進捗は Tauri event で push。
  設定は「LLM (CLI 認証委任、キー欄なし)」と「画像生成 (OpenAI / Gemini はキー必須、ComfyUI は無キー)」を別セクションに。

## 掟（Mandate）

- **データ・ファースト**: コードの前に `data_contract.yaml`（名詞）を凍結する。
- **PoC 必須**: バグ修正・新機能は Red→Green をテストで実証してから完了。推測修正は不可。
- **リサーチ先行**: 実装前に三点測量（既存コード grep / 仕様 / 記憶）。移植元は Kataribe を先に読む。
- **撤去したら grep**: 機構・enum 値・フィールドを撤去したら、その名前で全台帳（`CLAUDE.md` / `specs/*.md` /
  `data_contract.yaml` / doc comment）を grep し追従漏れを回収してから完了。機能の着地時は「どの台帳へ書いたか」を数える。
- **失敗時に謝罪しない**: 観察 → 仮説棄却 → 次の検証ステップの三段で進む。
- **層分け**: 仕様・契約はこの file 台帳に書く。Memoria (session `AppPromoVideo`) には蒸留した教訓・判断だけ。
- **入力を指す語を画像プロンプトに書かない**: 「スクリーンショットに従え」「参照画像のように」は被写体として描かれる (Kataribe #85)。

## 主要コマンド

```bash
cargo test --workspace                    # PoC (promo_core + cli_runner + pipeline + image_gen)
cargo run -q -p pipeline --bin promo -- images <pkg>/promo.json --images gemini --snapshot <png> --image-scenes 2  # 参照画像だけ
cargo clippy --workspace --all-targets    # lint
cargo run -q -p pipeline --bin promo -- brief <repo>                       # RepoBrief を見る (LLM ゼロ)
cargo run -q -p pipeline --bin promo -- run <repo> --concept "..." --snapshot <png> --model sonnet --out <dir>  # live 通し
cd app && npm install && npm run tauri dev   # GUI (sccache が落ちる時は RUSTC_WRAPPER= を前置)
cd app && npx vitest run && npm run build    # frontend の単体テストと型検査
cd app/src-tauri && cargo test && cargo clippy   # backend (独立 workspace)
```

## 現状

- 2026-09-07: リサーチ → ユーザー決定 (Tauri+Vue / 走査は CLI 委任) → 台帳凍結 → spec 01 Phase 0 着地 →
  **ユーザー査読 11 点を rev2 に反映** (promo_core 20 + cli_runner 15 = 35 green・clippy clean。image_gen / pipeline / app は未作成)。
  次 = ユーザー端末で `pwsh scripts/capture_claude_fixture.ps1` (成功 fixture) → Phase A。
- 2026-09-08: fixture 採取スクリプトがユーザー実行で 2 段落ち (PowerShell `2>` の二重オープン + 残留 claude.exe /
  Start-Process の引用符欠落) → `failures.md` #2 #3 に記録し修正。本セッションでは認証で落ちるところまで確認済み。
  **成功 fixture はユーザー端末での `pwsh scripts/capture_claude_fixture.ps1` 待ち** (Phase A の前提)。
- 2026-09-08 (2 回目): ユーザー端末の採取も 401 × 10 回再試行 (2.5 分) で失敗 → 失敗ログを fixture 化し、`ApiRetry` 解析と
  `retry_notice` (再試行を進捗に出す) を追加。原因 = CLI の OAuth 期限切れ。**User スコープの `ANTHROPIC_API_KEY` を適用して
  成功 fixture を採取** (`claude_json_schema_ok.jsonl`) → `structured_output` 確定。**Phase A の前提が揃った。**
  反証 1 件: 成功 run にも 401 の再試行が 7 回混じる → 「認証再試行の初回で打ち切る」案は撤回 (failures #4)。
- 2026-09-08 **Phase A Done** (Windows 実機): `cli_runner::runner::run` (spawn / stdin・一時ファイル / 行ストリーム / timeout /
  cancel / 終端 auth で打ち切り) + `tree_kill` (Job Object。孫が timeout 後に消えるのを PoC で固定) + `fake_cli` 7 モード。
  workspace 50 green。Unix の pgid 経路は未コンパイル。
- 2026-09-08 **Phase B Done**: `crates/pipeline` (collect / TaskRunner trait / stages の再生成ループ / export / `promo` CLI) +
  promo_core の prompts・export。65 green。**live 1 本** (Kataribe、sonnet): 0.99 USD / 4 分 22 秒、7 シーン、再生成 1 回発火。
- 2026-09-08 **Phase C Done**: `crates/image_gen` (provider.rs = Kataribe の写し・不触 / generator trait / refs / comfy_wait /
  palette) + promo_core style + pipeline reference + `promo images`。93 green (+ live 5 ignored)。**live: Gemini 2/2 (17 s)**。
  live から visual_identity の英語固定と「UI が映るシーンを最低 1 つ」を追加。ComfyUI / OpenAI の live は未実施。
- 2026-09-08 **Phase D 実装**: `app/` (Tauri 2 + Vue 3)。16 command / event `promo-progress` / 設定 2 タブ (LLM = CLI 認証委任、
  画像 = API キー) / perProvider スロット / settings.json ミラー。vue-tsc・vite build・vitest 6・src-tauri check・clippy・test 3 green。
  trait に Send 境界を追加 (Tauri async command の要求)。
- 2026-09-08 GUI FB: スナップショットのドロップ / Ctrl+V / 横並び + 拡大 (SnapshotStrip / Lightbox)。**GUI からの実行が 401** →
  7 通りの切り分けで未再現 (failures #7)、有力仮説 = 起動元端末の `ANTHROPIC_API_KEY` が別物。処方 = `cli_runner::env_scrub`
  (ホスト結合変数を子に渡さない) + 設定に認証診断 + 『OAuth ログインを使う』チェック → **OAuth ON で通過 (端末の鍵が無効で確定)**。
- 2026-09-08 **rev3**: 「スクショに全く従わない」→ 製品カットは**モデルが背景、Rust が実スクショを合成** (`image_gen::compose`)。
  カット = product | mood、`motion_prompt` (i2v 用)、コピー先は MiniMax 限定 (Veo / Sora は指示に従わない)。102 green。
  **live 成功 (Fuseforks → MiniMax i2v、ユーザー「綺麗にできた」)** = P1 + P2 が初めて同時に成立。
  次 = Phase E (別リポジトリで再現 / v2 候補: 斜め置き合成・mood の一貫性・motion の粒度)。未コミット。
- 2026-09-08 **見出しの焼き込み (opt-in・既定 OFF)**: `image_gen::fonts` (システム + app_data/fonts) + `image_gen::caption` (ab_glyph)。
  GUI 設定にフォント一覧・プレビュー。アプリで焼くか手で焼くかはユーザーのアンケート待ち。107 green。
- 2026-09-08 **Phase E rev4** (別リポジトリでの再現): `promo brief` を 7 リポジトリに当てたら RepoBrief の tree が
  生成物で埋まった。**tree の出所を `git ls-files` に**、追跡された生成物は**機械生成名で 1 行に畳む**、
  **鍵に見える名前は tree に載せない** (`collect.rs`。契約 `RepoBrief.tree_source`)。件数による畳み込みは
  実装前に棄却 (Fuseforks specs/ 52 と outcast .sqlx/ 49 は 3 件差)。実測: mxf-tool 314→51 /
  CC-Sakura 400(切り捨て)→122 / outcast 280→214。crates 112 green。
  **live (Verificator、sonnet、snapshot 0 枚)**: analyze 0.2857 USD/27.9 s + plan **attempts 1** 0.2710 USD/81.8 s
  = 0.557 USD / 110 s、6 シーン。解析は Python コアの中身 (PyAV インメモリ / Fraction の 2 ポインタ結合 /
  1 パーセンタイル検出) を正しく拾った。**別リポジトリで LLM 2 段は一発通過** = 一般化を確認。
  合成カットはスナップショット待ち (兄弟リポジトリに実 UI 画像が無く、GUI も素の vite では描けない)。
  **反証: 「子セッションでは CLI の認証が継承されない」は現行 CLI では成立しない** — `claude -p` が通る
  (failures #4 / #7 の記述は 2.1.223 時点のもの)。
- 2026-09-08 **rev5 (面の傾き)**: Phase E の合成 live で、背景がローアングルなのに貼った UI が正対のままだった
  (failures #10)。`Scene.plate_tilt` (yaw/pitch ±35 度) を LLM が書き、`PlateMode` で貼り方を切り替える —
  **perspective** (既定、射影変換で面を倒す) / **frontal** (正対固定 + 背景のアングル語を検査で弾く)。
  検査と本文がモードに依存するので `validate_scene_plan` / `scene_prompt` が `PlateMode` を取る。
  傾き 0 は従来の overlay 経路のまま (画素等価を PoC で固定)。CLI `--plate`、GUI は画像タブ。
  crates 117 green / vitest 10 / backend green・clippy clean。**i2v での良し悪しは未実測**。
- 罠台帳 `failures.md` (#1 RepoBrief の上限単位 / #2 #3 採取スクリプト / #4 401 の再試行ループと「採取できた」の誤読)。
  実測: claude 2.1.223 の stream-json 封筒 (system/init → assistant → result)。Claude デスクトップの
  子セッション内では OAuth が継承されず `authentication_failed` (fixture 化済み)。aider / gemini / codex / Flutter 無し。
- 詳細は `specs/01_promo_pipeline.md` と `data_contract.yaml`。

## 再開の手順 (2026-09-08 /sleep 時点)

1. `cargo test --workspace` (crates 108 green) と `cd app && npx vitest run` (10 green) で足場を確認。
2. **未コミット** (Initial commit 以来ゼロ)。作業前に `git add -A && git commit` で区切るのが安全。
3. 開いている判断: 見出し焼き込みの既定 (ユーザーのアンケート待ち、機構は opt-in で入っている)。
4. 次: **Phase E の live 通しをユーザー端末で** — 別リポジトリ (outcast / Verificator が候補) + UI スナップショット 1 枚。
   スナップショットは各リポジトリに無い (README の画像はロゴ・マスコットだった) ので**撮影が要る**。
   その後の候補: 傾けた絵を MiniMax i2v に通して perspective / frontal を実測 / mood カットの一貫性 / motion の粒度。
5. GUI 起動は `cd app && RUSTC_WRAPPER= npm run tauri dev`。CLI の認証は設定「OAuth ログインを使う」ON が確実 (failures #7)。

## 台帳

- `data_contract.yaml` — 名詞。`specs/NN_*.md` — 機能ごとの決定と Phase。`failures.md` — 罠台帳 (最初の罠が出たら作る)。
