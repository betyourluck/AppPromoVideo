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
  **出力は run ごとに隔離** (`<pkg>/runs/<run_id>/`)。以前は上書きして過去の生成物を消していた (failures #12)。
- **`app/`**: Tauri 2 + Vue 3。HTTP とプロセスは全部 backend。進捗は Tauri event で push。
  identifier は `jp.outcasts.apppromovideo` — **app_data と WebView プロファイルの場所を決めるので、
  公開後は変えられない** (契約 `AppIdentity`)。localStorage の接頭辞 `apppromo.` はこれとは別物。
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
| `data_contract.yaml` | **名詞と契約**。型・上限・enum・不変条件 | 実装の前。ここが正本。触ったら `scripts/check_data_contract.py`。新しいブロックはトップレベルの切れ目に足す (mapping の途中に挿すと後ろのキーの親が変わる) |
| `specs/NN_*.md` | **決定と Phase**。rev ごとの判断と理由、接地の限界 | なぜそうなっているかを知りたいとき |
| `failures.md` | **罠台帳**。症状 → 真因 → 処方 → 一般化 | 同じ形の問題に当たったとき |
| `history.md` | **作業ログ**。いつ何が起きて何が分かったか | 経緯をたどるとき。**読まなくても現状は分かる** |
| `CLAUDE.md` | この file。北極星 / アーキテクチャ / 掟 / 現状 / 台帳の地図 | 毎回 |

## 主要コマンド

```bash
cargo test --workspace                    # PoC (promo_core + cli_runner + pipeline + image_gen)
cargo clippy --workspace --all-targets    # lint
python scripts/check_data_contract.py     # data_contract.yaml の構文と重複キー (PyYAML が要る。台帳は機械が読まないので壊れても何も落ちない)
cargo run -q -p pipeline --bin promo -- brief <repo>   # RepoBrief を見る (LLM ゼロ)
cargo run -q -p pipeline --bin promo -- run <repo> --concept "..." --snapshot <png> --model sonnet \
  --plate perspective|frontal --images gemini --out <dir>          # live 通し
cargo run -q -p pipeline --bin promo -- images <pkg>/promo.json --images gemini --snapshot <png>  # 参照画像だけ
cd app && RUSTC_WRAPPER= npm run tauri dev   # GUI
cd app && npx vitest run && npm run build    # frontend の単体テストと型検査
cd app/src-tauri && cargo test && cargo clippy   # backend (独立 workspace)
```

## 現状 (2026-09-11)

- **spec 01 は Phase 0〜E 着地、rev33 まで反映済み (rev27〜29 は試行)。** crates 159 green / vitest 65 / backend 15 green・clippy clean。
- コミットは Initial `a75c3bc` の上に 36 本。`origin/main` には rev26 まで push 済み — **rev27〜29 (UI の試行、一時コミット) / rev30 (デザインの修正・多言語化) / rev31〜33 (履歴の全画面・比較の並び・チェックボックス、一時コミット) / data_contract.yaml の構文修正 は未 push** (private。**週末に public 予定**)。
- 通し (解析 → 構成 → 参照画像 → 合成) は **4 リポジトリで live 成功** (Kataribe / Verificator /
  AppPromoVideo 自身 / Fuseforks)。**出口まで到達** — MiniMax i2v の 15 秒が X に投稿された (2026-09-09)。
- CLI の認証は現行 `claude` なら子セッションからも通る。落ちる時は GUI 設定「OAuth ログインを使う」ON。
- **GUI は再ビルドしてから触る**。rev13 で dev プロファイルを変えた (debug の画像処理が 71 倍遅かった)。

**編集できるもの (rev9〜21、各シーンの「見出し / はめ込み…」ボタン → ダイアログ)**

| | |
|---|---|
| コピー文 | 書き換え + 「最初の文に戻す」(`original_copy` に LLM の初出を控えてある) |
| 見出し | フォント / 大きさ / 色 / **縦位置を数値で** (`y_ratio`)。上下の選択は rev18 で撤去 (縦位置が上位互換。面が避ける側は `effective_position` が `y_ratio` から導く) |
| はめ込み | **傾き (yaw・pitch) — つまみは実効値を指す** (`0° (正面)` / `18° (LLM)` / `18°`。rev21) / 大きさ / 横位置 / 縦位置 / **使うスナップショットの選び直し** — 一覧には左の入力ペインに**後から足した画像も出る** (選ぶとその run に写す、rev20)。取り込み口は入力ペイン 1 つ。**同じパスで撮り直したものも出る** — 同じ名前が 2 行並び、下が今のファイル (rev23)。**mood にも足せる** — 既定は「はめ込みなし (絵のまま)」で、選ぶと面が乗る。`cut_kind` は書き換えない (rev24) |

やり直しは `<run>/base/` の**素材**(product は背景 / mood は絵) から**合成ごと**行う。
**生成 API は呼ばない** = 無料・無劣化。焼き直しは「適用」を押した時だけ (実測 debug 0.64 s / release 0.25 s)。
つまみと枠は**実効値**を指す (`tilt_of` / `caption_layout` / `plate_quad` — 合成が使う関数から取る)。
いじっている間は予定位置を枠で重ねる — **見出し** (行ごと) と **面** (台形)。
どちらも合成・焼き込みと同じ関数から座標を取る。
**編集はダイアログ** (rev16、左が絵・右がつまみ)。絵を大きく見るためで、mood カットにも出る。
未適用のまま閉じようとすると確認が出る。
**確認はアプリ内のメッセージボックス** (`dialog.ts` の `ask` + `MessageBox.vue`、rev26)。
**ブラウザ標準の confirm / alert / prompt は使わない** — 見出しに `localhost:1421 の内容` と出る。`noBrowserDialogs.test.ts` が網。
**履歴は全画面** (rev31、`RunsScreen.vue`。3 ペインと入れ替え、メインは `v-show`)。「開く」は**読めた時だけ**メイン画面へ戻る (`openRun` が成否を返す)。
比較中は比較対象を A → B で表の先頭に出し、表は比較中だけ 30vh で中をスクロール、画像の側を縮める (rev32)。
**見た目の変更は画面を測ってから報告する** — vite の dev サーバーをブラウザのペインで開き、ストアにダミーを入れて位置を数値で取れる (rev32 で初実施)。
**測る前に、今のコードがページに届いているかを確かめる** (今回の変更でしか存在しない要素・規則の有無)。開いたページはサーバーが止まっても古いまま動く (rev33)。
チェックボックスとラジオは文字入力欄の寸法を持たない、表のセル (td) は flex にしない (rev33)。

**開いている判断**

1. **frontal が高い理由** — rev15 で再生成の回数と種別を残すようにした (`RunStats`、正本は
   `<run>/promo.json`。索引と GUI の「再生成」列はその写し)。**live の記録はまだ 0 件**なので、
   1.5 倍の説明は依然として推測。数えるには同一リポジトリ・同一スナップショットで
   perspective / frontal を各数本走らせる必要がある (LLM 費用がかかる)。
   過去の 3 行は遡って埋められない — 当時どこにも残していないため。
   **rev25 から `RunStats.models` に実際のモデル名 (解決後) も残る**ので、比べる run が同じモデルかを
   promo.json で確かめられる。モデル欄は再現性のため正式名 (`claude-sonnet-5` 等) を勧める。

**決着 (2026-09-09)**: `PlateMode` の既定は **frontal**。MiniMax i2v の実機観測で「斜めにすると
動画が動かしすぎる」(ユーザー)。傾ける経路は残す (設定 / `--plate perspective`)。
**保存済みの設定は上書きしないので、既に perspective の環境は設定で 1 度切り替える必要がある。**

**次の主題 (2026-09-09 ユーザー)**: **UI をもっと簡単にする。** 機能は出口 (MiniMax の 15 秒) まで
到達したので、次は使い勝手。ユーザーが X で今どきの UI デザインを探して持ち込む予定 —
**参照が来てから着手する** (こちらで先に作り込まない)。
**2026-09-11、ユーザーが最初の手を出した**: 文字を少し大きく (大きさをトークン化して +1px) と、3 つの列の枠を外す (rev27、試行中)。
続けて書体を IBM Plex Sans JP の Bold に、文字を 1.5 倍に (rev28、試行中)。
さらに見出し 1.5 倍・項目 1.25 倍に分け、説明文 (`.muted`) とログを Medium に (rev29、試行中)。
**同日、ユーザーがデザインを修正し UI を多言語化した** (会話の外。書体 Inter + Noto Sans JP / 太さ 500・600・700 / 形のトークン / アイコン / ja・en・zh-CN)。
こちらは差分を検めて回収した (rev30、spec 01 の 122〜127): 消えた `--fs-h-xl` / 書体の同梱 / 多言語化の残り / `<b>`・`<mono>` / キーの型 / `t()` の置換。
**文字は `main.css` のトークンを参照する** — 大きさ 項目 `--fs-*` (倍率 `--fs-scale`) / 見出し `--fs-h-*` (倍率 `--fs-scale-heading`) / 太さ `--fw-*` / 書体 `--font-ui`。部品に直書きしない。
フォールバックの無い `var(--x)` には定義が要る (`cssTokens.test.ts`)。
**タイトルバーのアプリ名は倍率の対象外** — `--font-chrome` / `--fs-chrome` / `--fw-chrome` (rev29 で試行前の値に固定、rev30 のデザイン修正で Inter / 14px / 600)。
**書体は同梱する** (rev30、`@fontsource-variable` の Inter / Noto Sans JP を main.ts で import)。宣言名は `Inter Variable` / `Noto Sans JP Variable`。
**外から読まない** — CSP が通さない (`fonts.test.ts` が網)。
**画面の文言は `i18n.ts` の辞書を通す** — ja が正本、en / zh-CN は `Record<MessageKey, string>` で型で縛る。強調・等幅は文言に `<b>` / `<mono>` と書いて
`Rich.vue` で出す (`v-html` は使わない)。コードに日本語を直書きしない (`noHardcodedJapanese.test.ts`。対象外は settings.ts のフォント名と `console.*`)。
backend 由来のログ・エラーの文言は日本語のまま。
**試行の未決 (2026-09-11 rev30 時点)**: ①rev27〜33 の採否 (rev30〜33 は Tauri の GUI で未目視 — 同梱の書体で出るか、英語で溢れないか、履歴の全画面・戻り方・比較の並び・チェックボックス)
②en / zh-CN の訳は私が書いた (未査読) ③`.muted` (47 か所) に巻き込まれた表の列・ラベルを戻すか ④ログの「medium」を太さと解釈した件
⑤タイトルバーの「実行中」chip の大きさ ⑥既存の文言の不具合 2 点を直すか (設定の説明が `**全シーンの既定**` のまま / 「既定は OFF」が実際の ON と違う)
⑦ユーザーが先に作った未使用キー 17 個を消すか。
旧 ②Plex の同梱 と旧 ③WebView2 がユーザーごとのフォントを拾うか は、rev30 で書体を同梱にしたので閉じた。
現状の形は 3 ペイン + 全画面の履歴 (rev31) + ダイアログ 2 種 (設定 / シーン編集) で、rev16〜21 で
シーン編集の中身が増えた。**2026-09-11 に初めてメイン画面のスクリーンショットを見た** — 観察 3 点は
`history.md` の同日エントリ (特に「`画像: off` と `参照画像を生成` が別ペインに離れている」は見本が無くても直せる)。
シーン編集ダイアログも同日に見た (rev23 / rev24 のユーザー目視。観察 2 点は `history.md`)。

**次の候補**: 傾きと可読性の境目 /
mood カットのモチーフ一貫性 / `motion_prompt` の粒度 / ComfyUI と OpenAI の live /
Unix の `tree_kill` / `--add-dir` 外 Read の拒否確認。

**未整理**: `README_en.md` と `briefs/readme_en_translation.md` (会話の外で作られたもの) を
rev14 のコミット `5929f93` に巻き込んだ。実害は無いが粒度が嘘になっている。分離するかは未判断。

## 再開の手順

1. `cargo test --workspace` (159 green) と `cd app && npx vitest run` (65 green) で足場を確認。
2. `git log --oneline -5` で直前の着地を見る。詳しい経緯は `history.md`。
3. 上の「開いている判断」に答えが来ていないか確認してから着手する。
