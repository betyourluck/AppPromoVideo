# spec 01: プロモ動画パイプライン — リポジトリから「動画生成用プロンプト + 参照画像」まで

**ID**: 01
**Date**: 2026-09-07
**Status**: Draft rev2 (2026-09-07 ユーザー査読 11 点を反映。Phase 0 着地・Phase A 未着手)
**Branch**: main 直接コミット (Kataribe と同じ流儀。Phase 単位で刻む)

---

## Goal

既存アプリのリポジトリと UI スナップショットから、訴求ポイント → シーン構成 → 参照画像 →
Veo/Sora に貼れるプロンプト一式を出すデスクトップツール。LLM はローカル CLI (claude -p 等) を
サブプロセスで叩く (HTTP 直呼び禁止)。動画の生成そのものはスコープ外。

北極星: **「貼れば動く出力」**。訴求文の巧さでなく、動画生成 UI にそのまま貼れる粒度と、
スナップショットに寄った参照画像の一貫性が売り。

---

## 構想文 (ユーザー提示 2026-09-07) からの訂正

| 構想文 | 訂正 | 根拠 |
|---|---|---|
| Flutter + flutter_rust_bridge v2 | **Tauri 2 + Vue 3** | ユーザー決定 2026-09-07。Kataribe の image_gen.rs (1463 行、3 プロバイダ PoC 済み) と設定 UI がそのまま移植できる。この機体に Flutter / dart / frb codegen が無い |
| Rust Edition 2024 / 1.78 以上 | edition 2024 / **rust-version 1.85** | 2024 edition は 1.85 から。機体は 1.95 |
| ComfyUI は WebSocket で進捗追跡 | **HTTP ポーリング** (v1)。WS は進捗率が要るときだけ | Kataribe が /prompt → /history 1 秒間隔 → /view で実機 Green。WS は複雑さの割に v1 で要る情報が無い |
| 「API キー管理負担を抑える」 | **LLM のキー管理を不要にする** に絞る | 画像生成の OpenAI / Gemini はキーが要る。完全に無キーなのは ComfyUI 経路だけ |
| Rust がリポジトリを走査してプロンプトを組む | **CLI に委任** (Rust は圧縮マニフェストを stdin に流し、claude に --add-dir + Read/Glob/Grep を許可) | ユーザー決定 2026-09-07。走査器の実装が最小。aider / custom は Read 系ツールが無いのでマニフェストだけで書かせる |
| 入力例の endpoint `[http://…](http://…)` | `http://127.0.0.1:8188` | Markdown リンクの残骸。JSON として不正 |
| Kataribe で「動画生成 API の呼び方」を調べる | Kataribe に動画生成は無い。**画像生成** (specs 24-27) が移植元 | grep 0 件 (2026-09-07) |

---

## アーキテクチャ (Kataribe の 3 分割と同型)

```
crates/promo_core   純関数のみ (FS・HTTP・プロセスなし)。型 (data_contract の名詞) / schemars で JSON Schema
                    機械生成 / RepoBrief の刈り込みと整形 / 検査 / fenced JSON 救済 / コピー整形
crates/cli_runner   tokio::process。argv 組み立て (純粋) / stream-json 解析 (純粋) /
                    spawn + 本文の運搬 (stdin | 一時ファイル) + stdout/stderr の行ストリーム + timeout + cancel
crates/image_gen    Kataribe app/src-tauri/src/image_gen.rs の移植 (Tauri 非依存を grep で確認済み)
crates/pipeline     IO と結線 (Phase B)。RepoBrief の収集 (FS) / 解析→構成の 2 タスク / 検査ループ /
                    export の書き出し。Kataribe の harness に相当
app/                Tauri 2 + Vue 3。HTTP とプロセスは全部 backend。進捗は Tauri event で push
```

- **LLM は書き、Rust は検める。** LLM 出力は schemars 由来の schema で構造化し、Rust が
  件数・尺・言語 (video_prompt は英語) を検査して弾く (Kataribe の「LLM は提案し、エンジンが裁く」の縮退形)。
- **走査は CLI の道具に委任、送る土台は Rust が握る。** RepoBrief (tree / README / manifest 先頭) を
  stdin に流し、CLI には Read / Glob / Grep だけ許す。Write / Edit / Bash は許可しない
  (ユーザーのリポジトリを書き換える経路を構造的に持たない)。
- **参照画像 = スナップショットの添付。** 「色調・レイアウトを抽出して写像」は、画像を参照として
  渡す経路 (Gemini inlineData / OpenAI images/edits / ComfyUI %ref_n%) の上に、VisualIdentity から
  合成したスタイルアンカー文を接頭辞で足す形。**プロンプト本文に「スクリーンショットに従え」と
  書かない** (Kataribe #85: 入力を指す語は被写体として描かれる)。

---

## Stories

### P1: 貼れば動く
> ユーザーとして、リポジトリと世界観を入れたら、Veo/Sora のプロンプト欄にそのまま貼れる
> シーン別プロンプトが出てほしい。なぜなら動画生成 UI 側で書き直す時間が今の律速だから。

**Acceptance**:
- Given リポジトリ path と video_concept, When 解析 → 構成を実行, Then ScenePlan (3〜8 scene、
  各 video_prompt は英語) が schema 検証を通って GUI に出る
- Given scene と コピー先 (Veo / Sora / 汎用), When コピーを押す, Then Veo / Sora は `video_prompt` だけが
  入り (比率と尺は各サービスの UI で選ぶ)、汎用は別行に `[aspect 16:9 | 5s]` が付く。`--ar` はどこにも出ない

### P2: 参照画像がアプリに寄る
> ユーザーとして、生成された参照画像が自分のアプリの色と UI に見えてほしい。
> なぜなら汎用の綺麗な絵は宣伝にならないから。

**Acceptance**:
- Given スナップショット 1 枚以上, When 参照画像を生成, Then 3 プロバイダのどれでもスナップショットが
  参照として送られる (encode 純関数の PoC で固定)。枚数は `reference_limits` (openai 既定 1 / gemini 3 /
  comfy 3) で切り詰め、切り詰めたことを UI に出す
- Given スナップショット 0 枚, When 設定を保存, Then palette 3 色の手入力が必須になる

### P3: CLI が無い・落ちた・止まった が見える
> ユーザーとして、CLI の不在・認証切れ・ハングが 1 行の理由つきで見えてほしい。
> なぜなら黙って止まるツールは信用できないから。

**Acceptance**:
- Given 存在しない executable, When 実行, Then CliError::NotFound がトーストに出る (spawn 前)
- Given 認証切れの claude (fixtures/claude_auth_failed.jsonl), When 解析, Then CliError::Auth
- Given timeout 経過, When 実行中, Then プロセスが kill され CliError::Timeout

---

## Phase 計画

| Phase | 内容 | PoC (Red→Green) | 状態 |
|---|---|---|---|
| 0 | 台帳凍結 (本 spec / data_contract / CLAUDE.md) + workspace 骨格 + promo_core 型と schema + cli_runner の argv 組み立てと stream-json 解析 | schema が Scene の必須項目を含む / 認証失敗 fixture が Auth に分類される / claude argv に stdin 本文が載らない / fenced JSON 救済 | **着手** |
| A | 成功 fixture 採取 → `structured_output` 確定。cli_runner の実行: spawn / 本文の運搬 (stdin・一時ファイル) / 行ストリーム / timeout / cancel / NotFound / 終端エラーでの打ち切り / **子孫ごと kill** (Windows Job Object・Unix pgid) | `tests/runner.rs` 10 本 (fake_cli 7 モード): stdin 往復 + structured / NotFound / **timeout 後に孫が消える** / cancel / 終了コード + stderr tail / 一時ファイルの往復と削除 / custom の fenced 救済 / 終端 auth で 60s の hang を待たない / process_alive の検算 / scratch が repo 外 | **Done (2026-09-08、Windows 実機)**。Unix 側は未コンパイル |
| B | crates/pipeline: RepoBrief の収集 (FS) → 解析 → ScenePlan、Rust 側検査ループ (違反を戻して再生成、最大 2 回)、export 書き出し、live 用 CLI `promo` | collect 4 本 (除外と深さ / README 大小 + manifest 順 / PNG ヘッダと非対応拒否 / 全件エラー) + stages 4 本 (fake runner: 解析 / **違反 → 再生成 → 通過** / 上限で失敗 / 形違い) + export 1 本 + promo_core prompts 3 + export 3。**live: Kataribe で通し** (analyze 62 s + plan 2 attempts 183 s、0.99 USD、7 シーン 30 s) | **Done (2026-09-08)**。`--add-dir` 外の拒否確認は未実施 (未決へ) |
| C | image_gen 移植 (provider.rs = Kataribe の写し、以後不触) + `ImageGenerator` trait + 参照上限 + ComfyUI の `/queue` 併読とバックオフ + palette の画素算出 + style anchor + pipeline::reference | Kataribe 同梱 PoC 10 green (live 5 は ignored) + refs 3 + comfy_wait 4 + palette 3 + compose 2 + style 3 + reference 3。**live: Gemini で 2/2 成功 (17 s)**。目視で 1 場面・暖色暗色基調 | **Done (2026-09-08)**。ComfyUI / OpenAI の live は未実施 (`/queue` の形は実機未確認) |
| D | Tauri 2 + Vue 3 殻 (`app/`): 3 ペイン (入力 / 結果 / 進捗ログ)、設定 2 タブ (LLM = CLI 認証委任 / 画像 = API キー)、perProvider スロット、settings.json ミラー、プロバイダ別コピー、export、16 command + event | vue-tsc + vite build / vitest 6 (toBackendCli・toBackendConfig の漏れ封鎖・migrate) / src-tauri cargo check・clippy・test 3 | **実装済 (2026-09-08)。GUI 目視はユーザー待ち** |
| E | 別リポジトリでの再現。まず LLM ゼロの `promo brief` を 7 本に当て、RepoBrief の一般化を測る → rev4 (tree の出所を git に) | collect 8 本 (既存 4 + git 優先 / 機械生成名の畳み込み / 鍵名の遮蔽 / `venv`)。**実測 7 リポジトリ**: mxf-tool 314→51 / CC-Sakura 400(切り捨て)→122 / outcast 280→214 / Verificator 114→68 / Kataribe 188→176 / Fuseforks 141→107 / KindleScan 25→17 | **rev4 着地 + live 1 本 (2026-09-08)** |
| E-live | Verificator (Python + PyAV コア / Tauri GUI、放送用途) に解析 → 構成を通す。スナップショット 0 枚 (`validate_scene_plan(_, 0)` は product カットを要求しない) | brief 9,380 字 / tree 68 行。analyze 0.2857 USD / 27.9 s。plan **attempts 1 (再生成なし)** 0.2710 USD / 81.8 s。合計 **0.557 USD / 110 s**、6 シーン・尺合計 30。`ui_traits` は GUI のソースを実際に読んで書かれていた (frameless title bar / dashed drop zone / monospace path)。`motion_prompt` にシーン間の連続性が自発的に出た | **Done (2026-09-08)**。合成カットはスナップショット待ちで未検証 |

---

## 決定事項 (rev2。取り消し線は rev1 からの変更)

1. **本文は argv に載せない (全 kind、例外なし)** — claude / custom は stdin、aider は runner が一時ファイルへ書き
   `--message-file` (公式 `-f`)。~~例外は aider (`--message`)~~ は撤回 (査読 1)。`--yes` は存在せず `--yes-always`。
2. **構造化は schemars 単一真実源** — `--json-schema` に渡す schema は `ScenePlan` / `AnalyzedSummary` の型から機械生成。手書き禁止。
3. **fenced JSON 救済を全 kind で持つ** — 規則は「最後の ```json フェンス優先 → 括弧バランスの取れた最上位オブジェクトの最後」。
   ~~最初の `{` から最後の `}`~~ は 2 オブジェクトを跨いで拾う (Red で再現) ので廃止 (査読 4)。claude の一次経路は
   `result.structured_output` (公式文書で確認)。
4. **許可ツールは Read / Glob / Grep のみ、`--add-dir` とセット、cwd は app の作業ディレクトリ** — `-p` は cwd の
   `.claude/settings.json` の hook と `.mcp.json` を信頼ダイアログなしで実行する (公式)。解析対象を cwd にしない。
   `--dangerously-skip-permissions` と `--bare` (OAuth を使わない = キー必須) は既定で付けない。不変条件は純関数のテストで固定 (査読 3)。
5. **タイムアウト既定 600 秒・最小 30、kill は子孫ごと** — Windows は Job Object、Unix は pgid へ SIGTERM → 5s → SIGKILL。
   ~~`child.kill()` + wait~~ は Windows で TerminateProcess となり孫が孤児化する (査読 8)。
6. **画像の保存名は `scene_NN_ref_MM.png`** — 同一シーン複数枚で衝突しない (査読 9)。参照の送信枚数はプロバイダ別上限で切り詰め、切り詰めを UI に出す。
7. **video_prompt に `--ar` を書かせず、コピーもプロバイダ別** — Veo / Sora はプロンプトのみ、汎用は別行にメタ。~~コピー時に `--ar` 付与~~ は Midjourney 記法で誤り (査読 5)。
8. **CLI は画像を見ない (v1)。palette はスナップショットの画素から Rust が算出** — image crate で k-means / ヒストグラム (LLM 不要・純関数)、
   ユーザー編集可、スナップショット 0 枚なら手入力必須。~~README / テーマ設定から LLM が推定~~ は README にテーマ色が無いリポジトリが大半で接地しない (査読 11)。
9. **RepoBrief は全 kind で同一バイト列 = 最低保証** — 上限は文字数で統一 (tree 400 行 × 100 字 / README 8000 字 / manifest 5 × 2000 字 / 総量 64k 字)。
   claude の深掘りは上乗せであって別経路ではない (査読 3)。刈り込みは promo_core (純粋)、収集は pipeline (IO) (査読 6)。
10. **キーの 2 層構造を UI で明示** — LLM は CLI 認証委任 (キー欄なし)、画像は OpenAI / Gemini がキー必須で ComfyUI だけ無キー。「完全無キー」と読める文言を置かない (査読 2)。
11. **ComfyUI の待機と失敗を分ける** — `/history` 未着の間は `/queue` を併読し、どこにも無ければ消失として即失敗。ポーリングは 1→2→4→5s (査読 7)。
12. **製品 UI が映るシーンを最低 1 つ要求する** (2026-09-08 live 起点) — 情景だけでは参照スナップショットの着地点が無く、宣伝としても製品が映らない。
13. **visual_identity は言語設定に関わらず英語** — 画像モデルに渡る文字列なので、copy の言語 (ja) に引きずられない (live で日本語が混入した)。

## rev3 (2026-09-08、ユーザー FB「スクショに全く従わない、ありえない画面」+ 参考ツイート = 画像ファースト → MiniMax i2v)

14. **カットは `product` と `mood` の 2 種。product は合成、mood だけ生成** — 実スクショの画素を Rust が背景に貼る (`image_gen::compose`)。
    モデルには背景だけ描かせ (画面・端末・文字の語は validate で弾く)、参照は送らない。「モデルは舞台を描き、Rust が画面を貼る」。
    #85 の「参照を指すな」はキャラの見た目を**寄せる**規律で、UI を**写す**要求には逆だった (failures #8)。
15. **画像ファースト、動画は i2v** — 各カットは静止画 1 枚 (ストア画像兼用) が主役。`motion_prompt` (動きとカメラだけ) を追加し、
    `video_prompt` は text-to-video の保険に降格。決定 12 (UI シーン最低 1 つ) は決定 14 の `NoProductCut` 検査に置き換え。
16. **コピー先は MiniMax 限定** — Veo / Sora は指示に従わない (ユーザー実測) ので選択肢から外し、汎用 t2v を保険で 1 つ残す。
    MiniMax へは画像ファイルと motion_prompt の組で渡す。
17. **日本語見出しの焼き込みは opt-in (既定 OFF)** — ユーザー方針 (2026-09-08): フォントは「インストール済みのシステムフォント」と
    「自分で `app_data/fonts` に置いたもの」の両方から選ぶ。**アプリで焼くか手で焼くかは判断待ち**なので機構だけ用意し、比較材料の
    見本 1 枚を出す。フォントは同梱しない (Rust の ab_glyph でラスタライズ、名前は ttf-parser)。
    ~~v1 外~~ → 機構は入れたが既定 OFF。決定はユーザーのアンケート結果で。

## rev4 (2026-09-08、Phase E = 別リポジトリでの再現。RepoBrief の tree が生成物で埋まった)

18. **tree の出所を `git ls-files` にする** (契約 `RepoBrief.tree_source.primary`)。対象が git リポジトリなら
    追跡ファイルから深さ 3 以下の行を組み、祖先ディレクトリを補う。git でない / 失敗した時だけ従来の FS walk に落ち、
    `EXCLUDED_DIRS` / `EXCLUDED_EXT` はその fallback にだけ効く (`venv` を追加。点なしを取りこぼしていた)。
    根拠: 生成物かどうかを一番よく知っているのは除外語の一覧ではなくリポジトリ自身。gitignore 済みの
    `doxy/html` (227 行) と `vcpkg` (251 行) はこれだけで消える。
19. **追跡された生成物は機械生成名で畳む** (契約 `machine_named_elision`)。子の過半かつ 5 件以上が
    `[0-9a-f]{16,}` か UUID 断片なら `backend/.sqlx/ (49 entries, elided)` の 1 行にする。
    **件数による畳み込みは実装前に棄却** — Fuseforks `specs/` 52 件と outcast `.sqlx/` 49 件は 3 件差で分離不能
    (failures #9)。機械生成名は 7 リポジトリで完全分離した。
20. **鍵に見える名前は tree に載せない** (契約 `secret_names`)。`.env*` / `*.pem` / `*.p12` / `*_key` /
    `*key*.txt` / `*secret*` / `*credential*`。`cli_runner` は claude に `Read/Glob/Grep` + `--add-dir <repo>` を
    渡すので、brief がその名前を指すこと自体が鍵の在処を教える経路になる。README 本文の言及は落とさない。

**接地の限界**: git 優先は未コミットの作業を tree から落とす。Phase E の 7 本では消えたのは未追跡の
作業メモ・worktree だけだったが、実装が未コミットのリポジトリでは brief が薄くなる。
LLM を通した live 通し (analyze → plan → 参照画像) は本セッションでは未実施 — CLI の認証が
Claude デスクトップの子セッションでは継承されないため (failures #4 / #7)、ユーザー端末での実行が要る。

## rev5 (2026-09-08、Phase E の合成 live で背景と面のパースが噛み合わなかった)

21. **面の向きを契約に載せる**。`Scene.plate_tilt: Option<{yaw_degrees, pitch_degrees}>` (各 ±35 度、product のみ)。
    背景のアングルを書いた LLM が面の向きも書くので、両者が同じものを指す経路ができる。範囲外は
    `PlateTiltOutOfRange` で再生成に回す。
22. **貼り方を設定で選ぶ** (`PlateMode`、ユーザー判断 2026-09-08「1 でお願い。ただ 1,2 を両方、設定で切り替えられるのがいい」)。
    - `perspective` (既定): `image_gen::compose` が面を 3D で yaw/pitch 回転 → ピンホール投影 → 4 点から
      homography を解いて逆写像 + バイリニアで焼く。影も同じ quad で歪める。
    - `frontal`: 面は正対のまま。代わりに `image_prompt` のアングル語を `ProductBackdropAngled` で弾き、背景も正対に保つ。
    - **検査と本文がモードに依存する** = `validate_scene_plan` と `scene_prompt` が `PlateMode` を受け取る。
    - **傾き 0 は従来の overlay 経路をそのまま通す** (画素等価を PoC で固定)。既存 4 本の PoC を壊さない。
23. 経路は CLI (`--plate perspective|frontal`) と GUI 設定 (画像タブ「製品カットの画面の貼り方」) の両方から。

**live 検算 (2026-09-08、AppPromoVideo 自身 + 実 GUI スクショ)**: analyze 0.2903 USD / 48 s、
plan **attempts 2** (再生成の理由は既存の `ProductBackdropDrawsScreen` = 背景に `screen` が 3 シーン混じった) 0.4237 USD / 135 s、
images 3/3。LLM の傾き指定は意味と一致した — scene 2 `Top-down view` → yaw -10 / pitch -20、
scene 5 `three-quarter angle` → yaw +18 / pitch -12、正対の背景 (scene 4) は `plate_tilt` null。
合成結果は上辺が狭く下辺が広い、机に画面が寝た絵になった。

**接地の限界**: 傾けた絵を MiniMax の i2v に通した検証はまだ無い。①傾ける と ②正面固定 のどちらが動画として
良いかは未決で、だから両方を残した。傾けると画面内の文字は読みにくくなる — 許容範囲は未実測。
スクショ自身が持つ 1px の窓枠が、傾けるとエッジで階段状に見える (バイリニアの範囲では消えない)。
影は残っている (実測: 正対 76,476 px / 傾き 66,446 px) が、暗い背景では見えない。

## rev6 (2026-09-08、MiniMax 実測「判別しにくい文字は作り変えられる」)

24. **canvas をスクショに合わせて拡げる** (契約 `compose.canvas_for_snapshot`)。`1344x768` は固定値で、
    窓 (実測 1282x842) より縦が低い。`screen_ratio` を 1.0 まで上げても 0.912 倍にしかならず、
    **構造的に必ず縮んでいた** (0.78 では 0.711 倍 = 11px の UI 文字が 7.8px)。
    比率を保ったまま等倍で収まる大きさまで canvas を拡げ、長辺 `CANVAS_MAX_LONG_EDGE` (3840px) で頭打ちにする。
    実測の組み合わせでは `1344x768 → 1890x1080`、縮小率 1.000。
    **理由は rev3 と同じ**: 背景はモデルが描いた柔らかい絵なので拡大が効き、スクショは唯一の硬い情報。
    縮める対象を入れ替える。
25. **上限に当たって縮小が避けられない時は進捗に警告を出す** (`スクショを NN% に縮めます`)。黙って劣化させない。
26. **パッケージ内の寸法と形式を揃える** (`compose.fit_to_canvas`)。mood カットはプロバイダの出力をそのまま
    保存していたので **JPEG 1376x768 なのに拡張子は .png**、product カット (PNG 1344x768) と食い違っていた。
    動画は全フレームが同寸である必要がある。揃えられない時は素のまま保存して警告する (生成には金がかかっている)。

**接地の限界**: 「解像度を上げれば作り変えられない」はユーザーの MiniMax 実測に基づく方向であり、
**等倍にした版での再テストはまだ**。傾けた面の遠い側は等倍より小さくなるので、tilt と可読性は依然として
トレードオフの関係にある (角度と可読性の境目は未実測)。

## rev7 (2026-09-08、ユーザー指摘「再起動すると出力が揮発」→ 調べたらディスク上で破壊されていた)

**発見**: `package_dir_name` は `<app_name>_Promo_Package` だけで run の識別子を持たず、同じアプリに 2 回
実行すると `promo.json` / `scenes.md` / `scene_NN_ref_MM.png` が上書きされ、**過去の出力が消えていた**。
掃除もしないので、新しい run のシーン数が前回より少ないと前回の `scene_07` 等が残って混ざった。
症状は「アプリを再起動すると揮発」だったが、真因はアプリの状態ではなくファイルの側。

27. **run ごとに隔離する** (契約 `ExportPackage.run_isolation`)。`<export_dir>/<app>_Promo_Package/runs/<run_id>/`。
    `run_id` = `YYYYMMDD-HHMMSS` (**UTC**。ローカル時刻だと夏時間の切り替わりで順序が壊れる)、同一秒は `-2`, `-3`。
    **文字列比較がそのまま時系列順**になるので、一覧の並べ替えに日付解析が要らない。
    `promo images <run_dir>/promo.json` は promo.json の親に書くので入れ子でもそのまま動く。
    rev6 以前の `<pkg>/promo.json` は移動も削除もしない (放置)。
28. **索引はキャッシュ、正本はフォルダ** (契約 `RunIndex` / `RunRecord`、`app_data/runs.json`)。
    アプリは複数のリポジトリ・複数の出力先を横断して一覧する必要があるので走査だけでは足りない。
    索引が指す `run_dir` が消えていたら `missing` として出すだけで**索引からは消さない** (移動しただけかもしれない)。
    索引が壊れていても空で続行し、**file は残す** (人が直せる)。索引の書き込み失敗は本流を止めない
    (生成は成功しているので結果を捨てない)。
29. **比較が要件**。生成は「新しいものを過去と比べて選ぶ」作業なので、一覧・復元だけでなく
    **2 つの run の同じ scene を左右に並べる**ところまで作る (ユーザー判断 2026-09-08)。
    backend `list_runs` / `open_run` / `forget_run`、frontend は `RunsDialog`。
30. **`forget_run` は既定でファイルを消さない**。索引から外すだけ。`delete_files` を立てた時のみ削除し、
    そのときも `runs/<id>` の形でないフォルダは拒否する (誤爆よけ)。
31. **`copy_package` の名前を組み直す**。run dir をそのままコピーすると日時だけのフォルダ名になり
    アプリ名が消えるので、`<App>_Promo_Package_<run_id>` にする。

**live 確認 (2026-09-08、ユーザー実機)**: 同じアプリを 2 回実行して履歴に 2 行 (19:36 / 19:40、4 と 5 シーン、
どちらも gemini で画像あり、0.944 / 0.919 USD)。**上書きは起きず、左右比較も描画された。**
そのスクリーンショットから回帰を 1 件回収 — 「面」列が `Perspective` (Rust の識別子) で出ていた。
`RunRecord` の aspect / language / plate_mode は **契約の表記** (`16:9` / `ja` / `perspective`) を入れる。
既存の 2 行は表示専用なのでそのまま残す (解析しないので壊れない)。

**接地の限界**: rev6 以前の既存パッケージを履歴に取り込む移行は**やっていない** — 放置されるだけで
ファイルは無事だが、アプリの一覧には出ない。
**`RunRecord` は attempts と違反種別を持たない**ので、「このモードは再生成が起きやすい」を数えられない。
実測 2026-09-08: frontal 1.422 USD / perspective 0.919・0.944 USD (約 1.5 倍) — 原因は
`ProductBackdropAngled` による再生成だと**推測しているが確認できていない**。

## 検討した代案: Remotion (2026-09-08、採用しない)

React で動画をプログラム的に作る枠組み ([remotion-dev/remotion](https://github.com/remotion-dev/remotion))。
ユーザーが X の投稿を提示して検討した。

**筋は通っている**: rev3 → rev5 → rev6 の 3 つの罠はすべて「生成モデルに硬い情報を渡すと壊れる」の変奏で、
そのたびに保証を確率側から構造側へ移してきた。Remotion はその move の終点 — rev3 が静止画の画素を確率から
取り上げたのに対し、**動きまで取り上げる**。UI はネイティブ解像度のまま描かれ、傾きは CSS の 3D transform で
厳密、文字は文字として描かれるので、今日の 3 つの罠は原理的に発生しない。

**採用しない理由**: 北極星の「**動画そのものは作らない (スコープ外を守る)**」と正面衝突する。
これはスコープの拡張ではなく製品の定義変更で、ユーザー判断 (2026-09-08「今のところは 1 でいい」= スコープを守る)。

**記録しておく事実**:
- mood カット (情景の発明) は Remotion にはできない。置き換えではなく product = Remotion / mood = 生成画像 の併用になる。
- `motion_prompt` を書く仕事は消えず、英文からコードへ移るだけ。
- 公式 MCP (`@remotion/mcp`) は**非推奨**。理由は「導入が難しい / エージェントが確実に呼ばない / `/remotion-docs`
  スキルと重複」。代替は Agent Skills (`npx remotion skills add`)。有志の MCP サーバーは複数ある。
- ライセンスは個人・非営利・従業員 3 人以下の営利組織は無料。**Remotion の派生物を売る目的の改変は禁止**。
- 上流 (解析 → ScenePlan → 参照画像) は貼り先が MiniMax でも Remotion でも同じ資産。変わるのは出力の形だけ。

## 査読の反映 (2026-09-07、ユーザー査読 11 点)

| # | 査読 | 裁定 | 根拠 |
|---|---|---|---|
| 1 | aider の `--message` 例外は自己矛盾 | **採用**。`--message-file` + 一時ファイル | aider 公式 options: `--message-file` (`-f`) あり。`--yes` は無く `--yes-always` |
| 2 | 無キーは LLM だけ、UI で 2 層を明示 | **採用** (決定 10) | 訂正表の文言だけでは UI に届かない |
| 3 | 走査が CLI ごとに別物 / `--add-dir` 不変条件 | **採用 + 強化** (決定 4・9)。cwd を作業ディレクトリへ | 公式 headless: `-p` は cwd の hook / MCP を無確認で実行 = `--add-dir` 忘れより重い穴 |
| 4 | 検証基盤が未接地 / 救済が複数 `{}` で誤抽出 | **採用**。Red で誤抽出を再現 → 括弧バランス + 最後のフェンス | `structured_output` は公式文書で確認。stream-json の実データは fixture 手順を凍結 (scripts/) |
| 5 | `--ar` は Veo / Sora の文法ではない | **採用** (決定 7) | Midjourney 記法。Veo / Sora は UI で比率選択 |
| 6 | promo_core は純関数ではない | **採用** (決定 9)。収集は crates/pipeline へ | brief.rs は BriefInputs → RepoBrief → render() だけ |
| 7 | ComfyUI の待機と失敗が区別不能 | **採用** (決定 11) | Kataribe は 600s 待つだけだった |
| 8 | kill のゾンビ化 | **採用** (決定 5) | tokio kill = TerminateProcess。公式: claude は SIGTERM でプロセス木を止める |
| 9 | 参照画像の命名衝突と枚数上限 | **採用** (決定 6)。「OpenAI は 1 枚が安定」は**未検証** — API 上限 16 と Kataribe live 1 枚を別の列で持ち、既定 1 | `reference_limits` |
| 10 | image_gen.rs が Tauri 依存なら持ってこれない | **前提不成立**。grep で tauri 語 0 件、use は std / serde / serde_json のみ。ただし「入出力同型で検める」は採用 (Phase C = 同梱 PoC 15 本をそのまま持ち込む) | 2026-09-07 grep 実測 |
| 11 | VisualIdentity が弱い、手入力必須に | **採用 + 上乗せ** (決定 8)。手入力必須に加え、画素からの自動算出 (LLM 不要) を既定に | 査読案は「自動抽出 = LLM/README」を前提にしていたが、色は画素にある |

---

## 未決

- ~~claude stream-json の `result.structured_output` の実データ~~ → **2026-09-08 確定** (fixtures/claude_json_schema_ok.jsonl)。
- ユーザー端末での 401 の理由 (OAuth 期限切れは確定。User スコープの `ANTHROPIC_API_KEY` がその端末に載っていなかったかは未分離)。
- Read の到達範囲が cwd + `--add-dir` に閉じるか (`--add-dir` 外の既知ファイルを読ませて拒否を確認する live。Phase B では未実施、Phase E までに)。
- 1 run 約 1 USD (sonnet)。haiku での品質は未計測。GUI にはモデル欄と実測コストの表示が要る。
- aider の非対話モードでの stdout 形と `--message-file` の実挙動 (未導入。導入後に fixture 採取)。
- OpenAI `images/edits` の参照 2 枚以上の品質 (Kataribe live は 1 枚のみ)。
- ComfyUI のワークフロー同梱: Kataribe は汎用 1 本だけ同梱し、警告 3 本 (`%ref_1%` / `%seed%` / `%negative%`) で入れ忘れを見せる方針 (#83)。同じにする。

---

## Tasks

- [x] 台帳 3 点 (data_contract / spec 01 / CLAUDE.md)
- [x] workspace 骨格 (Cargo.toml / promo_core / cli_runner) — edition 2024
- [x] promo_core: 型 + schema PoC (schema の required と description / 検査 6 違反の全件収集 / 件数境界 / clipboard / JSON 往復 / fenced 救済 4 本) = 10 green
- [x] cli_runner: argv + stream-json 解析 PoC (本文が argv に載らない / 凍結既定と allowlist / extra_args と model / aider の例外 / custom / timeout 下限 / serde 既定 / **実測 fixture が Auth** / 成功封筒 / result 欠落 / 未知 type の素通し / 非認証 error は Shape) = 13 green
- [x] rev2 (査読反映): fenced 救済の Red 3 本 → Green (最後のフェンス / 複数オブジェクト / 文字列内括弧) + 5 本追加 / コピーのプロバイダ別 (Red→Green) / aider の `--message-file` 化と `--yes-always` / 不変条件 (Read ⇒ `--add-dir`、cwd ≠ repo、本文が argv に無い × 全 kind) / RepoBrief の純関数と上限 (最悪ケースで初案の総量が破綻 → Red → 文字数で統一) / fixture 採取スクリプト = **35 green・clippy clean**
- [x] 2026-09-08 採取スクリプトの罠 3 件 (failures #2〜#4) → api_retry の解析 + `retry_notice`。「認証再試行の初回で打ち切る」`early_abort` は成功 fixture (401 × 7 の後に成功) で反証され撤回、終端 `assistant.error` だけで止める
- [x] 成功 fixture 採取 (2026-09-08、User スコープの ANTHROPIC_API_KEY を適用して Neo 側で実行)。`structured_output` 確定・`StructuredOutput` tool_use の機序を記録・成功 run 内の非認証 api_retry と未知 type の素通しをテスト化
- [x] Phase A (2026-09-08): `runner.rs` + `tree_kill.rs` + `bin/fake_cli.rs` + `tests/runner.rs` 10 本。workspace 50 green・clippy clean。
      罠: windows-sys は引数型の feature (`Win32_Security`) が無いと関数宣言ごと消える (E0432 unresolved import で出る)
- [x] Phase B (2026-09-08): `crates/pipeline` (collect / task trait / stages / export / bin promo) + promo_core prompts・export。
      workspace 65 green・clippy clean。live 1 本 = Kataribe → `Outcasts_Lorekeel_Promo_Package/` (promo.json / scenes.md / snapshots)。
      再生成ループが実データで発火 (初回違反 1 → 2 回目通過)。
- [x] Phase C (2026-09-08): `crates/image_gen` (provider / generator / refs / comfy_wait / palette) + promo_core style + pipeline reference + `promo images`。
      workspace 93 green (+ live 5 ignored)・clippy clean。live: Gemini 2 枚 (`scene_01_ref_01.png` / `scene_02_ref_01.png`)。
      live から 2 点を直した: visual_identity を英語固定 / UI が映るシーンを最低 1 つ要求。
      罠: edition 2024 では `gen` が予約語 (モジュール名・変数名に使えない) / rev2 契約の「OpenAI 参照 16 枚」は未検証の主張だった (Kataribe の凍結値は 3、移植テストで判明 → failures #5)。
- [x] Phase D 実装 (2026-09-08): `app/` (Tauri 2 + Vue 3 + Pinia、CSS 変数テーマ、Tailwind なし)。backend 16 command (lib.rs) + settings_store / env_store (Kataribe の写し、接頭辞 apppromo.)。
      frontend: settings.ts (perProvider / toBackendCli / toBackendConfig)、settingsMirror.ts、store.ts、TitleBar / InputPane / ScenePanel / LogPanel / SettingsDialog。
      検証: vue-tsc + vite build green、vitest 6 green、src-tauri check / clippy / test 3 green。
      罠: Tauri async command は future に Send を要求 → trait に Send 境界 (contract DesktopUi.send_bounds) / State 参照を取る async command は Result 必須 / sccache が syn のビルドで 0xfffffffe (RUSTC_WRAPPER= で回避)。
- [x] Phase D 目視 FB 1 (2026-09-08): スナップショットのドロップ / Ctrl+V 貼り付け / 横並びサムネイル + クリック拡大 (参照画像も)。
      `dragDropEnabled: true` + `onDragDropEvent`、`save_clipboard_image` command、SnapshotStrip / Lightbox。vitest 9 green・build green・backend check/clippy green。
- [x] Phase D 目視 FB 2 (2026-09-08 04:19): GUI からの実行が 401 の再試行で止まる → 切り分け 7 通り (全部 Neo 側では成功、failures #7) → 有力仮説「起動元の端末の ANTHROPIC_API_KEY が別物で無効」(未確定)。
      処方: `cli_runner::env_scrub` (ホスト結合変数を子に渡さない) / 設定 LLM タブに認証の見え方 (鍵の有無・長さ・指紋、auth status、落とした変数) / 『OAuth ログインを使う』(鍵を子に渡さない) チェック。
      crates 95 green・vitest 9・backend check/clippy green。
- [x] rev3 (2026-09-08): CutKind / snapshot_index / motion_prompt、validate 5 種追加、scene_prompt にスナップショット一覧と product 規律、
      `image_gen::compose` (PoC 4)、pipeline reference の product 経路 (背景生成 → 合成、失敗時は単色) PoC 1、scenes.md と GUI を i2v 前提に、
      CopyTarget = minimax | generic、`promo compose` サブコマンド。crates 102 green・vitest 9・backend check/clippy green。
      目視 1 枚 (Gemini 背景 + Kataribe UI 合成) は忠実。
- [x] rev3 の live (ユーザー、2026-09-08): Fuseforks で GUI 実行 → product カットに実画面が合成 → MiniMax (Hailuo) i2v で「綺麗にできた」。
      mood 1 (光る網状ノード) + product 2 (暗い舞台 + 実 UI + 落ち影) のフレームを確認。**P1「貼れば動く」と P2「製品が映る」が初めて同時に成立。**
- [x] 見出しの焼き込み (opt-in、2026-09-08): `image_gen::fonts` (システム + app_data/fonts、TTC 対応、日本語グリフ判定) + `image_gen::caption` (ab_glyph、
      中央揃え・落ち影・自動縮小) + RefJob.caption + GUI 設定 (フォント一覧 / 高さ / 位置 / プレビュー / フォルダ) + `promo caption` / `promo fonts`。
      crates 107 green・vitest 10・backend check/clippy green。見本 1 枚 (BIZ UDゴシック B)。**既定 OFF、採否はユーザーのアンケート待ち**。
- [x] Phase E: 別リポジトリで再現 (rev4 = tree の出所を git に) + live 2 本 (Verificator 一発通過 / AppPromoVideo で合成カット) + rev5 (面の傾きを設定で切替)
- [ ] Phase F 候補: **等倍 (1890x1080) の対を MiniMax i2v で再テスト** (perspective / frontal、文字が作り変えられないかを含む) / 傾きと可読性の境目 / mood カットのモチーフ一貫性 / motion の粒度
- [ ] Phase E

## Notes

- 2026-09-07: リサーチターン (Kataribe grep / Memoria recall / 環境実測) → ユーザー決定 2 点 (Tauri+Vue / 走査委任) → 本 spec 起草 → Phase 0 着地 (workspace 23 green・clippy clean)。
  **接地の限界**: Phase 0 のテストは実装と同時に書いた (Red を観測していない)。実測由来は fixture 1 本 (認証失敗) のみで、**成功時の stream-json と `--json-schema` の構造化出力の置き場は未確認** — `structured_output` 読みは推定で、Phase B の live (ユーザー端末) で確定する。未コミット (ユーザーの手)。
