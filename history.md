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

### rev7 — run の履歴 (ユーザー指摘「再起動すると出力が揮発」)

調べたら揮発ではなく**破壊**だった: `<app>_Promo_Package` に run の識別子が無く、2 回目が 1 回目を
上書きしていた (failures #12)。`<pkg>/runs/<run_id>/` に隔離し、`app_data/runs.json` に索引を持たせ、
GUI に一覧・復元・2 run の左右比較・削除を足した。crates 122 / backend 7 / vitest 10 green。

気づかなかった理由: 開発中は毎回別のリポジトリで試しており、同じアプリに 2 回続けて回したのは
perspective / frontal の対を作った時が初めて。その時は手で別フォルダにコピーしてから回していたので、
**自分で回避策を打っていたことに気づいていなかった**。

### public 化に備えて fixture を伏せた

`crates/cli_runner/fixtures/*.jsonl` に home path (`C:/Users/...`) とセッション UUID が残っていたので
`scripts/redact_stream.py` を拡張して潰した (`memory_paths` / `terminal_slash_commands` を落とし、
入れ子のどこに現れるか分からない home path と UUID は行を JSON に直した後で正規表現で置換)。
UUID は空にせず `<uuid>` に置き換える — `session_id` が空でないことを前提にしたテストがあるため。
潰した後も workspace 122 green。API キー本体は元から入っていない (`apiKeySource` の名前だけ)。

### rev7 の live 確認 (ユーザー実機)

同じアプリを 2 回実行して履歴に 2 行 (19:36 / 19:40、4 と 5 シーン、どちらも gemini、0.944 / 0.919 USD)。
**上書きは起きず、左右比較も描画された。** スクリーンショットから回帰を 1 件回収 — 「面」列が
`Perspective` (Rust の識別子) で出ていた。`RunRecord` の aspect / language / plate_mode は
`format!("{:?}")` ではなく **契約の表記** (`16:9` / `ja` / `perspective`) を入れる。
既存の 2 行は表示専用なのでそのまま残す。比較画像に `max-height: 46vh` を入れてダイアログ内に収めた。

### bundle identifier を jp.outcasts.apppromovideo へ (public 化の直前、ユーザー判断)

`jp.outcasts.apppromo` だった。兄弟アプリ (`jp.outcasts.concordia` / `.fuseforks`) は末尾が製品名なので、
ここだけ略称になっていた。identifier は **app_data (`%APPDATA%/<id>/`: .env / settings.json / runs.json /
fonts/ / snapshots/) と WebView プロファイル (`%LOCALAPPDATA%/<id>/EBWebView/`) の場所を決める**ため、
配布後に変えると使う人の設定・キー・履歴が黙って消える。**公開前にしか変えられない**ので今やった。

移行は Roaming のフォルダ名変更で済ませ、WebView プロファイルは作り直させた
(設定は settings.json ミラーから復元される — ミラーはこの事故のために作った機構、Kataribe 2026-08-28 実機)。
localStorage の接頭辞 `apppromo.` は identifier とは別物なので揃えていない (揃えると既存キーの移行が要る)。

### identifier 移行の live 確認 (ユーザー実機)

新 identifier で起動し、**履歴 3 行が読め、設定も復元された**。WebView プロファイルを移していないので
localStorage は空から始まったが、リポジトリパス・スナップショット・世界観テキストが戻っている =
`settings.json` ミラー (Kataribe 2026-08-28 の実機事故を受けて作った機構) が**初めて本番で機能した**。
同じ画面で rev7 の左右比較 (perspective 19:40 / frontal 21:52、scene 4) も動作。

**観察 (仮説つき)**: frontal の run だけ 1.422 USD で、perspective の 0.919 / 0.944 の約 1.5 倍。
frontal は `ProductBackdropAngled` の検査が増えるので再生成が発火した可能性が高いが、
**attempts も違反種別も永続化していないので確認できない**。測るには `RunRecord` に attempts と
違反種別を持たせる必要がある (未実施)。

### rev8 — 見出しの焼き込みを既定 ON へ (開いている判断 2 の決着)

ユーザーから「テロップとコピー字幕を追加できますか」→ 調べたら**焼き込みは rev3 期から入っていて、
既定が OFF なだけ**だった。字幕ファイル (SRT / WebVTT) を提案したが「そんな本格的なものはいらない、
画像に文字を入れるくらい」で不採用。既定 ON + フォントの自動選択のみ実施。

`pickCaptionFont` (frontend の純関数、Red 5 本 → 実装): ①日本語グリフ必須 ②app_data/fonts のものを
最優先 ③**太めのゴシック優先** (画像に焼くので細い明朝・教科書体は写真の上で読めない) ④同点は family 名。
`store.ensureCaptionFont()` を `makeImages()` の先頭に置いたので、設定画面を開かなくても効く。
選べなければ焼かずに進み理由を出す = 「ON なのに何も起きない」を無言にしない。vitest 10→15 green。

### rev8 の穴: 既定 ON は保存済み設定に負ける

GUI 確認の直前に気づいた。`migrateImageGenSettings` が `enabled: c.enabled === true` だったので、
①保存済みの `false` (旧既定が書かれたもの) が勝つ ②`enabled` キーが**無い**保存も `false` になる
(`undefined === true`)。②は既定へ落とすべきなのでバグ。`typeof c.enabled === "boolean" ? c.enabled : 既定`
に直した (Red 1 本)。①は仕様どおり — 明示された false はユーザーの意思かもしれないので上書きしない。
既存ユーザー (= マスター) は GUI で 1 度チェックを入れる必要がある。vitest 15→17 green。

### 見出しの豆腐を切り分け → 実バグは優先語が ASCII だけだったこと (failures #13)

ユーザー実機で豆腐 (□)。仮説を 4 つ潰した (フォントにグリフが無い / burn_caption が壊れている /
出力が壊れている / 自動選択が Hack を選んだ) — **全部外れ**。豆腐の画像は実行中の Lightbox が
前の run の画像を映していたもので、出力ファイル自体は正しく焼けていた。

ただし切り分けの過程で実バグが出た: `pickCaptionFont` の優先語が ASCII だけで、実機の family 名は
`游ゴシック` / `メイリオ` / `HGPｺﾞｼｯｸE` と日本語表記。当たらないと同点になり localeCompare で
ASCII 名が勝つため、`Aharoni Bold` が `游ゴシック` に勝っていた。手元で `Noto Sans JP` が
選ばれていたのは Noto が ASCII 名だった偶然。優先語に日本語表記と半角カナを追加 (vitest 17→18)。
診断用の probe は撤去。

### 見出しの live 確認 → スコア式の符号反転を発見 (failures #14)

ユーザー実機で見出しが正しく焼かれた (`シーン構成もプロンプトも、まとめて出力。`)。**開いている判断 2 は決着。**
ただし自動選択の経路は通っていない — ユーザーが `NotoSansCJKjp-Black.otf` を手で選んだため。

そのフォント名を見て気づいた: `pickCaptionFont` のスコアが `100 - index*10` で、優先リストを
7→12 語に伸ばしたときに末尾 (`"sans"`) が **-10** になり、何にも当たらない 0 に負けるようになっていた。
`"Noto Sans CJK JP"` は `"noto sans jp"` に当たらず (`cjk` が挟まる) `"sans"` にだけ当たるので該当。
`(PREFER.length - hit) * 10` に変更 (vitest 18→19)。**同じセッション内で自分がリストを伸ばして壊していた。**

### rev9 — 見出しを 1 枚ごとに変えられるようにした

ユーザー要望「1 枚ごとに位置を決めたい。今あるところは既定として、結果ペインで位置・フォント・色・
大きさを変えられたら」。壁は**焼いた後の 1 枚しか残っていなかった**こと — 変えるには背景から作り直すしかなく、
位置を試すたびに別の絵になる。ユーザー判断でディスク約 2 倍を受け入れ、焼く前の合成を `<run>/base/` に残した。

`reburn_caption` は base から焼いて直下の完成品を置き換える。**生成 API を呼ばないので無料**、毎回 base から
焼くので**何度やっても劣化しない**。上書きは `PromoJson.caption_overrides` に入るので run を開き直しても効く。
LLM のスキーマ (`Scene`) には足していない — 埋めるのは人。全フィールド `Option` でフィールド単位に既定へ落ちる。
crates 122→126 / backend 8 / vitest 19 green。**live 未実施、rev9 より前の run は base/ が無いので焼き直せない。**

### rev10 — base/ に置くのを合成画像から背景へ (failures #15)

ユーザーの問い「背景だけ生成させて、文字はアプリで焼き込んでいるのではないのですか？」。
**その認識が正しく、私の説明が誤っていた** — 焼く行為は無料で、コストは焼く対象を作り直すことにあった
(背景はローカル変数で一度も書き出していなかった)。答えるために合成を読み直したら、rev9 の欠陥に気づいた:
`base/` の合成画像には `with_caption_band` が帯の位置を焼き込んでおり、位置を変えると重なる。

`base/` を背景に変え、焼き直しを**合成からやり直す**形に。`layout_for` を帯を決める唯一の場所にして
生成と共有 (生成側もジョブ既定でなくカットごとの位置で合成するようになった)。
`PromoJson.plate_mode` を追加し、焼き直しの傾き適用を promo.json 自身が決めるようにした。
crates 126→127 / backend 8 / vitest 19 green。**live 未実施、rev10 より前の run は焼き直せない。**

## 2026-09-09

### rev11 — はめ込みも 1 枚ごとに変えられるように

rev10 で合成からやり直せるようになった副産物として、傾き・大きさ・位置も無料で変えられる。
ユーザー要望を受けて UI まで通した。`Layout` に横方向のずらしが無く中央固定だったので追加
(正対・傾きの両経路)。`PlateOverride` は全フィールド `Option` で、傾きの上書きは LLM の
`scene.plate_tilt` に勝つが触っていない軸は残る。縦位置を指定すると見出しの帯のずらしを置き換える。
範囲は Rust 側で丸める (UI の入力を信用しない)。`copy_text` が無い product カットでも
はめ込みだけ触れるようにした。crates 127→129 / backend 8 / vitest 19 green。**live 未実施。**

