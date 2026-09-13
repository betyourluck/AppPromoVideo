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
  許可ツールは Read / Glob / Grep のみで `--add-dir <repo>` とセット (**claude のみ。agy にこのフラグは無い** —
  契約 `IsolationGuarantee`)。**cwd は app の作業ディレクトリ**であって
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
| `LICENSE` | **MIT**。公開リポジトリ (2026-09-13) | 配布・引用の条件を見るとき |
| `.github/workflows/build.yml` | **リリースビルド**。`v*.*` タグの push で 3 OS を建てて Release を draft で作る (Lorekeel / Fuseforks からの移植) | 配布物を出すとき |

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

## 現状 (2026-09-12)

- **公開した (2026-09-13)。** MIT (`LICENSE`)、`origin` は public。`v0.1.0` のタグで CI が**初回から 3 OS とも green**、
  Release に installer 7 点 (exe / msi / deb / rpm / AppImage / dmg / app.tar.gz)。
  **配布物の性質**: ①**`v0.1.0` の macOS は未署名・未公証**だったので、その Release (draft) は削除した (2026-09-13 ユーザー。
  タグは残す — タグは履歴、Release は配布物)。**公開されている版は `v0.1.1` のみ**。
  ②**macOS は aarch64 のみ** (`macos-latest` が arm ランナー。Intel Mac は対象外)。後から「なぜ動かない」と聞かれる種類なので、
  Release の説明に書いておくこと。③**`Upload artifacts` は置かない** — Release Assets が正 (Fuseforks と同じ。理由は `build.yml` のコメント)。
- **`v0.1.1` (2026-09-13) = macOS の署名と公証が通った最初の版。** Release は draft、7 点。CI ログで
  `Signing with identity "Developer ID Application: …"` → `Notarizing Finished with status Accepted` → `Stapling app...` を確認。
  **アプリのコードは 1 行も変わっていない** (版番号 3 か所と workflow だけ) — Fuseforks v0.1.9 と同じ「配布物の性質だけが変わった版」。
  Apple の秘密 5 つは **Fuseforks で取った Developer ID (Team `6XU323VJZN`、証明書は 2031-08 まで) の流用** —
  証明書はアカウント単位でアプリごとには要らない。材料は `~/.apple-signing/`、GitHub の secret は読み戻せないので手元から入れ直した。
  `verify-notary.yml` (手動、Fuseforks から移植) で公証の資格情報だけを 1 分で検証できる。署名した配布物には実名が載る (Fuseforks で承知済み)。
  **タグは `X.Y.Z` の形で打つ** — `v0.1.1-` (末尾ハイフン) は semver でなく、以前は cargo test を終えた後の build で 3 OS 一斉に落ちた。
  今は Extract の段で拒む (failures.md 2026-09-13)。
- **Homebrew で入る (2026-09-13)**: `brew install --cask betyourluck/tap/apppromovideo` (完全修飾名は省略できない)。
  cask は `D:/Github/homebrew-tap/Casks/apppromovideo.rb` (v0.1.1 から。v0.1.0 は未署名なので載せない)。
  tap の `verify-cask.yml` を実機 (macos-latest) で回し、**`spctl -a -vv` が `accepted / source=Notarized Developer ID`** —
  これだけが「Gatekeeper が実際に受け入れる」の証拠 (codesign と stapler は構造物の主張)。
  **新版を出したら cask の `version` と `sha256` (Release API の `digest`) を更新する** — Fuseforks / Lorekeel と同じ手順で、
  `auto_updates` は付けていないので更新は `brew upgrade` が拾う。公式 homebrew-cask は注目度の門 (225 stars) で対象外。
- **winget は提出済み・マージ待ち (2026-09-13)**: `Outcasts.AppPromoVideo` 0.1.1 を [winget-pkgs PR #434054](https://github.com/microsoft/winget-pkgs/pull/434054)
  で New-Package 提出 (Fuseforks は 12 日でマージ、Lorekeel の PR #427244 は 2026-09-01 提出で未マージ)。**MSI のみ・`InstallerLocale` を書かない・
  `Scope: machine`** (Fuseforks / Lorekeel と同じ 3 判断。`InstallerLocale: en-US` を書くと日本語環境の `winget upgrade` が落ちる)。
  `ProductCode` は版ごとに変わるので**次の版は必ず新しい MSI から読み直す** (PowerShell の `WindowsInstaller.Installer` COM で読める)。
  マニフェストの正本は winget-pkgs 側、手元の `manifests/` は `.gitignore`。**マージされるまで README に winget を書かない**
  (書いてあるのに入らない、を避ける)。次の版は `wingetcreate update Outcasts.AppPromoVideo --version <V> --urls <MSI URL>` →
  ローカル生成 → `InstallerLocale` が混ざっていたら消す → `wingetcreate submit --token "$(gh auth token)"`。提出前に
  `gh repo sync betyourluck/winget-pkgs --source microsoft/winget-pkgs --branch master`。
- **spec 01 は Phase 0〜E 着地、rev47 まで反映済み (rev27〜29 は試行)。** crates 188 green / vitest 118 / backend 25 green・clippy clean。
- **agy でも通しが成功** (2026-09-12 21:13、ユーザー実機)。解析 → 構成 (1 回目で通過) → 参照画像 → 合成 → 見出しの焼き込み。
  費用の chip は出ない (agy は費用を返さないので描かない)。
- コミットは Initial `a75c3bc` の上に積んでいる。**本数も push 済みの範囲もここに書かない** — 書くたび 1 手遅れて嘘になる (実際に 2026-09-12、`rev42 まで push 済み` と書いた行の中に 「ここに書くと 1 手遅れる」と併記する矛盾を作った)。数えるなら `git rev-list --count a75c3bc..HEAD` と `git log --oneline origin/main..HEAD`。**公開リポジトリ** (MIT、`LICENSE`)。
- 通し (解析 → 構成 → 参照画像 → 合成) は **4 リポジトリで live 成功** (Kataribe / Verificator /
  AppPromoVideo 自身 / Fuseforks)。**出口まで到達** — MiniMax i2v の 15 秒が X に投稿された (2026-09-09)。
- CLI の認証は現行 `claude` なら子セッションからも通る。落ちる時は GUI 設定「OAuth ログインを使う」ON。
- **CLI の「種類」と実行ファイルは別の設定**。食い違うと別系統の argv が飛ぶ (今日の障害の根)。
  rev40 から `--version` の名乗りと突き合わせる (`CliKindCheck`)。設定画面で警告し、**rev41 から run の開始時に止める** —
  警告を設定画面にだけ置いた rev40 では誰も見ず、同じ事故が 3 回続いた。**警告は人が見に行く場所ではなく、必ず通る場所に置く。**
- **`agy` は隔離の保証が弱い** (rev39)。CLI 側に許可リストが無く、`write_to_file` は**拒否されない** (実測: ファイルが実際に作られた)。
  止まるのは `run_command` だけ。アプリの見張り (`AGY_ALLOWED_TOOLS`) は**検出であって予防ではない** — 契約 `IsolationGuarantee`。
  既定は `claude` のまま。agy を選ぶと設定画面に警告が常時出る。
  **`--add-dir` は agy の読み取りを縛らない** (実測)。縛るのは探索ツールの探索範囲だけで、そこから外れると
  agy はシェルに逃げる → rev42 でスナップショットの置き場を `--add-dir` に足した (**逃げ道でなく逃げる理由を消す**)。
- **費用は `Option`** (rev42)。agy は費用を返さないので 0 で埋めない。**片方の段でも不明なら合計は不明**。
- **CLI が落ちたら生ログを読む** (rev38)。`<app の作業ディレクトリ>/cli-logs/<UTC>-<pid>.jsonl` に stdout の行がそのまま残り、
  兄弟の `.invocation.json` に**何を起動したか** (program / args / cwd) が残る。最新 20 本、成功した run も。エラーの文言と進捗ログにも出る。
  **2026-09-12 の障害 2 件の真因は同じ** — 起動していたのが `claude` ではなく **`agy`** だった (`.invocation.json` で確定)。
  agy の `-p` は値を取るので `--output-format` を本文として飲む。rev39 で `CliKind::Agy` を足して対応済み。
- **GUI は再ビルドしてから触る**。rev13 で dev プロファイルを変えた (debug の画像処理が 71 倍遅かった)。

**編集できるもの (rev9〜43。見出しとはめ込みは各シーンの「見出し / はめ込み…」ボタン → ダイアログ、プロンプトは鉛筆)**

| | |
|---|---|
| コピー文 | 書き換え + 「最初の文に戻す」(`original_copy` に LLM の初出を控えてある) |
| 見出し | フォント / 大きさ / 色 / **縦位置を数値で** (`y_ratio`)。上下の選択は rev18 で撤去 (縦位置が上位互換。面が避ける側は `effective_position` が `y_ratio` から導く) |
| プロンプト | **鉛筆で編集モードに入ってから** `motion_prompt` / `video_prompt` を書き換える (rev37)。保存すると backend が `promo.json` と `scenes.md` を書き直し、書き換え後の promo を返す。**空は拒む** (検査と食い違わせない)。破棄は入る前の値に戻す |
| シーンの構成 | **並び替え / 複製 / 削除** (rev43)。`scene_id` は必ず位置 + 1 に振り直され、**手編集 (見出し・はめ込み・最初の文) と参照画像が一緒に動く** — 付け替えは `SceneRemap` 1 つが決める。追加は**複製** (検査が空欄を弾くので白紙は作れない)。3〜8 シーン、唯一の product カットは消せない。**尺は直さない** — 合計のずれを出すだけ |
| はめ込み | **傾き (yaw・pitch) — つまみは実効値を指す** (`0° (正面)` / `18° (LLM)` / `18°`。rev21) / 大きさ / 横位置 / 縦位置 / **使うスナップショットの選び直し** — 一覧には左の入力ペインに**後から足した画像も出る** (選ぶとその run に写す、rev20)。取り込み口は入力ペイン 1 つ。**同じパスで撮り直したものも出る** — 同じ名前が 2 行並び、下が今のファイル (rev23)。**mood にも足せる** — 既定は「はめ込みなし (絵のまま)」で、選ぶと面が乗る。`cut_kind` は書き換えない (rev24) |

やり直しは `<run>/base/` の**素材**(product は背景 / mood は絵) から**合成ごと**行う。
**生成 API は呼ばない** = 無料・無劣化。焼き直しは「適用」を押した時だけ (実測 debug 0.64 s / release 0.25 s)。
つまみと枠は**実効値**を指す (`tilt_of` / `caption_layout` / `plate_quad` — 合成が使う関数から取る)。
いじっている間は予定位置を枠で重ねる — **見出し** (行ごと) と **面** (台形)。
どちらも合成・焼き込みと同じ関数から座標を取る。
**編集はダイアログ** (rev16、左が絵・右がつまみ)。絵を大きく見るためで、mood カットにも出る。
未適用のまま閉じようとすると確認が出る。
**初回起動のナビゲーション** (rev44、`FirstRunTour.vue`。Lorekeel の移植)。**手順を教えるだけ**で、案内の中から実行はさせない。
対象は `data-tour` を付けた実物の要素で、1 歩目だけ印が 2 つ (実行ボタン以外を束ねて囲む)。初回だけ出す — 使った痕跡があれば印だけ立てる。**一度使った環境では出ないので、設定画面の「使い方を見る」が唯一の入口** (rev45)。
**確認はアプリ内のメッセージボックス** (`dialog.ts` の `ask` + `MessageBox.vue`、rev26)。
**配布ビルドでは右クリック / F5・Ctrl+R / 文字の選択を締める** (rev47、契約 `DesktopGuards`)。**dev では締めない**(検証と再読み込みは開発の道具)。
選べるままにするのは入力欄・プロンプト本文・エラー文・**進捗ログ** — ここを塞ぐと報告のためにログを貼れなくなる。
**ブラウザ標準の confirm / alert / prompt は使わない** — 見出しに `localhost:1421 の内容` と出る。`noBrowserDialogs.test.ts` が網。
**履歴は全画面** (rev31、`RunsScreen.vue`。3 ペインと入れ替え、メインは `v-show`)。「開く」は**読めた時だけ**メイン画面へ戻る (`openRun` が成否を返す)。
比較中は比較対象を A → B で表の先頭に出し、表は比較中だけ 30vh で中をスクロール、画像の側を縮める (rev32)。
**見た目の変更は画面を測ってから報告する** — vite の dev サーバーをブラウザのペインで開き、ストアにダミーを入れて位置を数値で取れる (rev32 で初実施)。
**測る前に、今のコードがページに届いているかを確かめる** (今回の変更でしか存在しない要素・規則の有無)。開いたページはサーバーが止まっても古いまま動く (rev33)。
チェックボックスとラジオは文字入力欄の寸法を持たない、表のセル (td) は flex にしない (rev33)。
**設定の入り切りはスイッチ** (`Switch.vue`、中身は checkbox のままでつまみは実体のある要素。rev34)。
**遷移のある見た目は遷移を止めてから測る** (`--trans-fast` を 0s に。描画が止まったページでは遷移が進まず、時刻 0 の値が返る。rev34)。
**設定も全画面** (rev35、`SettingsScreen.vue`。3 列 = LLM CLI / 画像生成 / 面と見出し。長い説明・認証の診断・ComfyUI の詳細は `<details>` で畳む)。
**横に並べる項目は下端で揃える** (`.grid-row`、ラベルは 1〜3 行に折り返す)。**「収まる」は窓の大きさとセットで測る** — ユーザーの窓で測る (rev35)。
**結果ペインの見出しの数値は正本 (`promo.json` の `run_stats`) から** — 記録が無ければ 0 ではなく chip ごと出さない (rev36)。
**タイトルバーの履歴 / 設定はトグル** — 開いている画面のアイコンをもう一度押すと戻る。開いている間はアクセント色 (rev36)。

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
**ブランドの標「Outcasts」は `main.css` の `.brand-line` / `.outcasts` が唯一の定義** (rev46)。色は `--brand-mark` (oklch の成分)。タイトルバーと初回案内が共有し、**大きさだけ**置き場ごとに決める。
**書体は同梱する** (rev30、`@fontsource-variable` の Inter / Noto Sans JP を main.ts で import)。宣言名は `Inter Variable` / `Noto Sans JP Variable`。
**外から読まない** — CSP が通さない (`fonts.test.ts` が網)。
**画面の文言は `i18n.ts` の辞書を通す** — ja が正本、en / zh-CN は `Record<MessageKey, string>` で型で縛る。強調・等幅は文言に `<b>` / `<mono>` と書いて
`Rich.vue` で出す (`v-html` は使わない)。コードに日本語を直書きしない (`noHardcodedJapanese.test.ts`。対象外は settings.ts のフォント名と `console.*`)。
backend 由来のログ・エラーの文言は日本語のまま。
**試行の未決 (2026-09-11 rev30 時点)**: ①rev27〜35 の採否 — **2026-09-12 にユーザーが GUI を目視 OK** (設定の 1 画面は英語表示でも溢れない。スイッチの色はアクセントで確定)。
**rev36・37 は Tauri の GUI で未目視** — 特に**プロンプトの保存を GUI から通していない** (backend のテストでは固定済み)
②en / zh-CN の訳は私が書いた (未査読) ③`.muted` (47 か所) に巻き込まれた表の列・ラベルを戻すか ④ログの「medium」を太さと解釈した件
⑤タイトルバーの「実行中」chip の大きさ ⑥既存の文言の不具合 2 点を直すか (設定の説明が `**全シーンの既定**` のまま / 「既定は OFF」が実際の ON と違う)
⑦**閉じた (rev38)**: 未使用の i18n キー 13 個を 3 言語から撤去 (270 → 257)。すべて改名の取り残しで置き換え先が現役だった。`i18nUnusedKeys.test.ts` が網。
旧 ②Plex の同梱 と旧 ③WebView2 がユーザーごとのフォントを拾うか は、rev30 で書体を同梱にしたので閉じた。
現状の形は 3 ペイン + 全画面 2 つ (履歴 rev31 / 設定 rev35) + ダイアログ 1 種 (シーン編集) で、rev16〜21 で
シーン編集の中身が増えた。**2026-09-11 に初めてメイン画面のスクリーンショットを見た** — 観察 3 点は
`history.md` の同日エントリ (特に「`画像: off` と `参照画像を生成` が別ペインに離れている」は見本が無くても直せる)。
シーン編集ダイアログも同日に見た (rev23 / rev24 のユーザー目視。観察 2 点は `history.md`)。

**次の候補**: GUI からのプロンプト保存の確認 (promo.json と scenes.md が揃うか) / プロンプトに「最初の文に戻す」を付けるか /
傾きと可読性の境目 /
mood カットのモチーフ一貫性 / `motion_prompt` の粒度 / ComfyUI と OpenAI の live /
Unix の `tree_kill` / `--add-dir` 外 Read の拒否確認。

**未整理**: `README_en.md` と `briefs/readme_en_translation.md` (会話の外で作られたもの) を
rev14 のコミット `5929f93` に巻き込んだ。実害は無いが粒度が嘘になっている。分離するかは未判断。

## 再開の手順

1. `cargo test --workspace` (162 green) と `cd app && npx vitest run` (75 green) で足場を確認。
2. `git log --oneline -5` で直前の着地を見る。詳しい経緯は `history.md`。
3. 上の「開いている判断」に答えが来ていないか確認してから着手する。
