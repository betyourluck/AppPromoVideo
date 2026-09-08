# history — AppPromoVideo の作業ログ

**この file は台帳ではなく履歴です。** 「今どうなっているか」は `CLAUDE.md`、決定と Phase は
`specs/NN_*.md`、名詞は `data_contract.yaml`、罠は `failures.md` が正本。ここに書くのは
**いつ何が起きて何が分かったか**だけで、ここを読まないと現状が分からない状態にはしない。

追記は新しいものを末尾へ。過去のエントリは**その時点の記録として書き換えない** — 後で覆った事実は
新しいエントリに「反証」として書き、古いエントリはそのまま残す。

---

## 2026-09-07

- リサーチ (Kataribe grep / Memoria recall / 環境実測) → ユーザー決定 2 点 (Tauri+Vue / 走査は CLI 委任)
  → 台帳凍結 → spec 01 Phase 0 着地 → **ユーザー査読 11 点を rev2 に反映**
  (promo_core 20 + cli_runner 15 = 35 green・clippy clean。image_gen / pipeline / app は未作成)。

## 2026-09-08

### fixture 採取 (Phase A の前提)

- 採取スクリプトがユーザー実行で 2 段落ち (PowerShell `2>` の二重オープン + 残留 claude.exe /
  `Start-Process` の引用符欠落) → `failures.md` #2 #3 に記録して修正。
- 2 回目も 401 × 10 回再試行 (2.5 分) で失敗 → 失敗ログを fixture 化し、`ApiRetry` 解析と
  `retry_notice` (再試行を進捗に出す) を追加。原因 = CLI の OAuth 期限切れ。
  User スコープの `ANTHROPIC_API_KEY` を適用して成功 fixture を採取 (`claude_json_schema_ok.jsonl`)
  → `structured_output` 確定。
- **反証 1 件**: 成功 run にも 401 の再試行が 7 回混じる → 「認証再試行の初回で打ち切る」案は撤回 (failures #4)。

### Phase A Done (Windows 実機)

`cli_runner::runner::run` (spawn / stdin・一時ファイル / 行ストリーム / timeout / cancel / 終端 auth で打ち切り)
+ `tree_kill` (Job Object。孫が timeout 後に消えるのを PoC で固定) + `fake_cli` 7 モード。50 green。
Unix の pgid 経路は未コンパイル。

### Phase B Done

`crates/pipeline` (collect / TaskRunner trait / stages の再生成ループ / export / `promo` CLI) +
promo_core の prompts・export。65 green。
**live 1 本** (Kataribe、sonnet): 0.99 USD / 4 分 22 秒、7 シーン、再生成 1 回発火。

### Phase C Done

`crates/image_gen` (provider.rs = Kataribe の写し・不触 / generator trait / refs / comfy_wait / palette)
+ promo_core style + pipeline reference + `promo images`。93 green (+ live 5 ignored)。
**live: Gemini 2/2 (17 s)**。live から visual_identity の英語固定と「UI が映るシーンを最低 1 つ」を追加。
ComfyUI / OpenAI の live は未実施。

### Phase D 実装

`app/` (Tauri 2 + Vue 3)。16 command / event `promo-progress` / 設定 2 タブ / perProvider スロット /
settings.json ミラー。trait に Send 境界を追加 (Tauri async command の要求)。

### GUI フィードバック → 認証の切り分け

スナップショットのドロップ / Ctrl+V / 横並び + 拡大 (SnapshotStrip / Lightbox)。
**GUI からの実行が 401** → 7 通りの切り分けで未再現 (failures #7)、有力仮説 = 起動元端末の
`ANTHROPIC_API_KEY` が別物。処方 = `cli_runner::env_scrub` (ホスト結合変数を子に渡さない) +
設定に認証診断 + 「OAuth ログインを使う」チェック → **OAuth ON で通過** (端末の鍵が無効で確定)。

### rev3 — モデルは舞台を描き、Rust が画面を貼る

「スクショに全く従わない」→ 製品カットは**モデルが背景、Rust が実スクショを合成** (`image_gen::compose`)。
カット = product | mood、`motion_prompt` (i2v 用)、コピー先は MiniMax 限定 (Veo / Sora は指示に従わない)。102 green。
**live 成功** (Fuseforks → MiniMax i2v、ユーザー「綺麗にできた」)。

### 見出しの焼き込み (opt-in・既定 OFF)

`image_gen::fonts` (システム + app_data/fonts) + `image_gen::caption` (ab_glyph)。
GUI 設定にフォント一覧・プレビュー。アプリで焼くか手で焼くかはユーザーのアンケート待ち。107 green。

### 初回コミット

Initial commit `a75c3bc` の上に 4 本 (台帳と足場 / crates / app / failures #8 の回収)。
罠 #8 が `app/failures.md` に書かれていて root に無かった — GUI 作業中に cwd が `app/` だったための分裂。

### Phase E rev4 — tree の出所を git に

`promo brief` を 7 リポジトリに当てたら RepoBrief の tree が生成物で埋まった (failures #9)。
**tree の出所を `git ls-files` に**、追跡された生成物は**機械生成名で 1 行に畳む**、
**鍵に見える名前は tree に載せない** (契約 `RepoBrief.tree_source`)。
件数による畳み込みは実装前に棄却 (Fuseforks `specs/` 52 と outcast `.sqlx/` 49 は 3 件差で分離不能)。
実測: mxf-tool 314→51 / CC-Sakura 400(切り捨て)→122 / outcast 280→214 / Verificator 114→68 /
Kataribe 188→176 / Fuseforks 141→107 / KindleScan 25→17。112 green。

**live (Verificator、sonnet、snapshot 0 枚)**: analyze 0.2857 USD / 27.9 s + plan **attempts 1**
0.2710 USD / 81.8 s = 0.557 USD / 110 s、6 シーン。解析は Python コアの中身 (PyAV インメモリ /
Fraction の 2 ポインタ結合 / 1 パーセンタイル検出) を正しく拾った。**別リポジトリで LLM 2 段は一発通過。**

**反証**: 「Claude デスクトップの子セッション内では OAuth が継承されない」は**現行 CLI では成立しない**
(`claude -p` が通る)。failures #4 / #7 の記述は claude 2.1.223 時点のもの。

### rev5 — 面の傾き

合成 live で、背景がローアングルなのに貼った UI が正対のままだった (failures #10)。
`Scene.plate_tilt` (yaw/pitch ±35 度) を LLM が書き、`PlateMode` で貼り方を切り替える —
**perspective** (既定、射影変換で面を倒す) / **frontal** (正対固定 + 背景のアングル語を検査で弾く)。
検査と本文がモードに依存するので `validate_scene_plan` / `scene_prompt` が `PlateMode` を取る。
傾き 0 は従来の overlay 経路のまま (画素等価を PoC で固定)。117 green。

**live 検算** (AppPromoVideo 自身 + 実 GUI スクショ): LLM の傾き指定は意味と一致 —
`Top-down view` → pitch -20、`three-quarter angle` → yaw +18、正対の背景は null。

**やらかし**: `cargo fmt --all` を打ったところ `rustfmt.toml` が無いため既定の 100 桁で全ファイルが
整形され直し、`image_gen/src/provider.rs` (「Kataribe の写し・不触」と明記したファイル) まで書き換わった。
21 ファイルを HEAD に戻して回収。**このリポジトリで `cargo fmt --all` を打たない** (130 桁で書かれている)。

### rev6 — 出力解像度

MiniMax 実測で「判別しにくい文字は作り変えられる」(ユーザー) → canvas 1344×768 が窓 1282×842 より低く、
**構造的に必ず 0.711 倍に縮んでいた** (failures #11)。`canvas_for_snapshot` で比率を保ったまま等倍に
収まる大きさへ拡げる (実測 1890×1080、縮小率 1.000、長辺上限 3840px)。上限で縮小が残る時は進捗に警告。
`fit_to_canvas` で mood カットの JPEG 1376×768 も同寸 PNG に揃える。120 green。

### Remotion は採用しない (ユーザー判断)

北極星「動画そのものは作らない」を守る。検討の記録は `specs/01` の「検討した代案」に。
公式 MCP (`@remotion/mcp`) は非推奨で Agent Skills 推奨、という事実もそこに含む。

### CLAUDE.md をルーター化 (ユーザー指示)

Remotion の Agent Skills の構造 (2.5 KB のルーター → 11 KB の REFERENCE → 2〜5 KB の葉) を見て、
毎ターン読まれる `CLAUDE.md` (13 KB、うち大半が履歴) を分離。履歴はこの file へ。
