# failures.md — AppPromoVideo 罠台帳

事故と罠の記録。**症状 → 真因 → 処方 → 一般化 → 接地の限界** の順で書く。番号は通し。

## crates/promo_core (2026-09-07 spec 01 rev2)

### 1. 上限を別の単位で決めると、総量が部分の和より小さくなる

**症状**: RepoBrief の初案は tree 400 行・README 8,000 字・manifest 5×2,000 字・**総量 32 KB**。最悪ケースの
PoC (`worst_case_render_fits_total_budget`) が落ちた。

**真因**: 部分の上限は文字数、総量はバイト。しかも tree の 1 行に上限が無く、部分の和 (36 KB+、多バイトなら 3 倍)
が総量を超えていた。総量は「守れない約束」だった。

**処方**: 単位を文字数で統一 (`RENDER_MAX_CHARS = 64_000`)、tree の 1 行にも上限 (100 字)。最悪 ≈ 59.5k 字を
多バイト文字で埋めた PoC で固定。

**一般化**: 上限を複数持つ契約は、**最悪ケースを 1 本のテストで組み立てて総量を検算**する。単位が違う上限は
検算しないと矛盾に気づけない。

## scripts (2026-09-08 fixture 採取)

### 2. PowerShell 7 の `2> file` はパイプライン内の native コマンドで同じファイルを二重に開く

**症状** (ユーザー実行): `'…' | claude -p … 2> stderr.txt | Set-Content raw.jsonl` が
`Out-File: The process cannot access the file 'stderr.txt' because it is being used by another process`。
さらに claude.exe が **stdin 待ちのまま残留** (PID 33524、`--max-turns 1` で 2 分以上生存) し、その後の
再実行も同じファイルで衝突し続けた。

**真因**: 2 つ重なっていた。①PowerShell が `2>` を Out-File で実装するため、パイプラインの Out-File と競合する。
②失敗した起動が claude.exe を残し、それが stderr.txt を掴んでいた。②のせいで「①を直しても同じエラー」に見えた。

**処方**: (a) リダイレクトを PowerShell に任せず `Start-Process -RedirectStandardInput/Output/Error` (本文は
prompt.txt から stdin へ)。(b) 「同じファイルで衝突」が続いたら `Get-CimInstance Win32_Process` でコマンドラインから
残留プロセスを特定して止める (`json-schema|apppromo_fixture` で grep)。

**一般化**: 「直したのに同じエラー」は**前回の失敗の残骸が原因を隠している**ことがある。処方を変える前に、
ファイルを掴んでいるプロセスを見る。cli_runner の Phase A で「timeout 後に子孫が残らない」を PoC にする理由が
ここでも出た (残留プロセスは次の実行を壊す)。

### 3. `Start-Process -ArgumentList` は引数を空白で連結するだけで引用符を付けない

**症状**: `--json-schema {"type":…}` が `Error: --json-schema is not valid JSON: JSON Parse error: Expected '}'`。

**真因**: Start-Process は配列を空白連結してコマンドラインにするだけで、要素の `"` を保護しない。JSON の `"` が
そのまま CRT の引数分割に食われた。

**処方**: Windows CRT の規則で `"` を `\"` に逃がし全体を `"` で囲む (`'"' + ($schema -replace '"','\"') + '"'`)。
**cli_runner (Rust) には無関係** — `tokio::process::Command::arg` は要素ごとに正しく引用する。この罠は採取
スクリプトだけのもの。

**一般化**: JSON を引数に載せる経路は、引用の責任が誰にあるかを確認する。Rust 側で argv を組む本番経路と、
PowerShell で手作りする採取経路は責任の所在が違う。

**接地の限界**: 修正後の実行は本セッション内なので認証で落ちる (`authentication_failed`)。schema が受理された
こと (エラーが schema → 認証に変わった) までは確認済み。成功 fixture はユーザー端末での実行待ち。

## crates/cli_runner (2026-09-08 fixture 採取 2 回目)

### 4. 「採取できた」は成功ではなかった — 401 は 10 回・約 2.5 分再試行してから落ちる

**症状**: ユーザーが「fixture 採取できた」と報告。しかし `fixtures/claude_json_schema_ok.jsonl` は存在せず、
temp の raw.jsonl は `system/api_retry` (401 `authentication_failed`, attempt 1..10, 遅延 0.5s→38s) が
並び、最後に assistant(error) + result(is_error)。ユーザーの claude.exe (PID 19856) は報告時点でまだ
再試行中だった。

**真因**: ①採取スクリプトは成功時だけ fixture を残す設計 (前回の修正) なので、失敗時に「3 lines -> …」の
行が出ても最後に消える — 途中の出力を成功と読んだ。②claude CLI は 401 でも 10 回再試行する。
再試行しても直らない種別 (認証) に 2.5 分を使い、その間ユーザーには何も見えない。

**処方**: (a) 失敗ログをそのまま `claude_auth_retry_loop.jsonl` として fixture 化 (api_retry を redact で
落とさないよう修正)。(b) `ParsedLine::ApiRetry` を追加し、`early_abort()` で `authentication_failed` /
`oauth_org_not_allowed` の初回再試行で runner がプロセスを止めて `CliError::Auth` を返す (rate_limit 等は
CLI に任せる)。(c) 採取スクリプトの最終行を `SUCCESS` / `NOT a success run` の 2 値に固定済み — 報告は
その行で判断する。

**一般化**: **失敗を試みる側 (CLI) の再試行方針と、直る見込みのない失敗の種別は別**。呼び出し側は
「何回再試行したか」でなく「再試行で直る種別か」で打ち切る。また、採取・計測の報告は「ファイルが在るか」
で確認する (人の報告は途中経過を読み違える)。

**分離済み (同日 0:13)**: 3 回目の実行 (PID 2860) も 401 ループ。親チェーンは pwsh → **WindowsTerminal.exe →
explorer.exe** で、Claude デスクトップは経路に無い。よって「子セッションの環境変数継承」は棄却、
**この機体の claude CLI の OAuth 自体が期限切れ**で確定。デスクトップアプリのセッションが動いていることは
CLI の資格情報が生きている証拠にならない (別の資格情報ストア)。処方 = CLI 側で再ログインしてから採取。

**設計への含意 (初案、同日撤回)**: 「`early_abort` で初回 401 を即座に『再ログインしてください』に変換する」
と書いたが、下の成功 fixture で反証された。

**決着 (同日 0:20)**: `claude auth status --text` = `Login: Expired`。ユーザーも対話で承認要求が出ることを確認
(= OAuth 期限切れ)。一方、Windows の **User スコープに `ANTHROPIC_API_KEY` が設定されており**、それを
プロセス環境に適用して採取スクリプトを回すと**成功** (`claude_json_schema_ok.jsonl`)。`-p` は OAuth が
切れていても API キーがあれば動く。**`auth status` は OAuth しか見ない** (API キーで動く状態でも
`loggedIn: false`) ので、起動前検査に使うなら「loggedIn=false かつ ANTHROPIC_API_KEY 未設定」の組だけを
『認証なし』と判定する。ユーザーの端末で 401 になった理由 (その端末のプロセスに User スコープの変数が
載っていなかった可能性) は未分離。

**反証 (同日 0:25)**: 成功 run の `api_retry` 7 回は**すべて 401 `authentication_failed`** だった
(78.5 s のうち約 70 s)。つまり「認証失敗の再試行は直らない」は誤りで、期限切れ OAuth で数回失敗した後に
API キーで通る経路が実在する (切り替えの機序は未確認)。書いたばかりの `early_abort` (初回 401 で kill) は
**この成功 run を殺していた**。テストが Red になったので撤回し、止めるのは終端の `assistant.error` だけ、
再試行は `retry_notice` で進捗に出してユーザーに待つ/中断を委ねる形にした。**「直らない種別」を自分の
推論で決めない — 直った実データが 1 本あれば推論は負ける。** `rate_limit_event` / `user` (tool_result) と
いう未知 type も流れる。`--json-schema` の機序は **`StructuredOutput` という tool_use**
(assistant → user の tool_result → result.structured_output)。--allowedTools に書かなくても動いた。

## crates/image_gen (2026-09-08 Phase C 移植)

### 5. 契約に「公式上限」を書いたが、それは検証していない値だった

**症状**: rev2 の `reference_limits.openai.api_max: 16` (「images/edits の image[] は最大 16 (公式)」) を前提に
`select_refs` のテストを書いたら落ちた。移植した Kataribe の `max_refs` は **全プロバイダ 3**。

**真因**: 査読 9 への回答を急ぎ、OpenAI の API 仕様の記憶を「公式」と書いて契約に載せた。この workspace で
16 を裏づける実測も文書引用も無い。一方 Kataribe の 3 は live で通っている値。

**処方**: 契約を Kataribe の凍結値 3 に戻し、16 は「未検証なので採らない」と明記。テストは 3 で固定。

**一般化**: **契約に書く数値は、出典 (実測 or 文書の引用) を同じ行に書けないなら書かない。** 「公式」と
いう語は出典ではない。§1 (上限の単位) と同族で、上限は最悪ケースの検算か出典のどちらかで裏づける。

### 6. edition 2024 では `gen` が予約語

**症状**: `pub mod gen;` と `let gen = …` が `expected identifier, found reserved keyword` で落ちた。

**処方**: モジュールは `generator`、変数は `generator` に改名。Kataribe (edition 2021) からの移植や
既存コードの写経で `gen` を使っている箇所は、2024 edition の crate に持ち込む時に全部引っかかる。

## app (2026-09-08 GUI 実測 — 実行が 401 の再試行で止まる)

### 7. GUI からの実行だけ 401 を繰り返す — 未再現、有力仮説は「端末の ANTHROPIC_API_KEY が別物」

**症状** (ユーザー実機、04:19): GUI から解析を実行すると CLI は起動する (pid 表示) が、stderr に
`claude.ai connectors are disabled because ANTHROPIC_API_KEY or another auth source is set and takes precedence
over your claude.ai login` が出た直後から `api_retry authentication_failed (HTTP 401)` を繰り返す。

**切り分け (全部 Neo 側で実測、いずれも 0 回の再試行で成功 = 原因ではない)**:
(A) User スコープの `ANTHROPIC_API_KEY` を単独で使用 → 成功。(B) OAuth のみ (`claude auth login` 済み、Claude Max) → 成功。
(C)(D) デスクトップ子セッションの環境変数 (`CLAUDE_CODE_CHILD_SESSION` / `SDK_HAS_HOST_AUTH_REFRESH` /
`CLAUDECODE` / `MESSAGING_SOCKET`) を残したまま → 成功。(E1) 既定モデル `claude-opus-5[1m]` → 成功。
(E2) `--add-dir D:\Github\Fuseforks` + allowlist → 成功。(F) `MESSAGING_SOCKET` を死んだパスに差し替え → 成功。
アプリと同じ引数 (`--json-schema --add-dir --allowedTools --verbose stream-json`)・同じ cwd (temp) も含む。
`.env` (dotenvy が拾う範囲: repo / 親 / Kataribe / Fuseforks) と PowerShell プロファイル・Windows Terminal 設定・
Machine スコープに別の鍵は無い。CLI のデバッグログには 401 の本文が残っていない。

**全事実と整合する仮説**: **アプリを起こした端末の `ANTHROPIC_API_KEY` が User スコープの値と別物で無効**。
根拠: 同じ 00:05 にユーザー端末の fixture 採取が 10 × 401 で落ち、Neo が User スコープの鍵で回した採取は成功した。
ユーザー端末の対話 CLI は「API Usage Billing」表示 = 鍵が載っている。アプリは起動元の端末の環境を継承する。
警告文も「鍵が優先される」と言っている。**未確定** — 端末側で鍵の長さと指紋を比べるまで仮説のまま。

**処方 (確定を待たずに入れたもの)**: (a) `cli_runner::env_scrub` — `CLAUDE_CODE_*` / `CLAUDECODE` / `CLAUDE_PID` /
`CLAUDE_EFFORT` / `CLAUDE_PREVIEW_*` / `CLAUDE_AGENT_SDK_VERSION` を子に渡さない (子 CLI はホストに結びつかない
独立プロセスであるべき。**原因の証明ではなく衛生**)。fake_cli `env` モードで end-to-end 固定。
(b) `check_cli` が認証の見え方 (鍵の有無・長さ・指紋 / AUTH_TOKEN / base_url / `claude auth status` / 落とした変数) を
返し、設定 → LLM タブに表示。実行開始時にも同じ 1 行を進捗ログへ出す。**鍵の値は画面にも台帳にも出さない**。

**一般化**: 「自分の環境で通る」は「ユーザーの環境で通る」の証明にならない。特に**プロセス環境の継承**が絡む機能は、
何を継承したかを製品自身が見せないと、切り分けがユーザーとの往復になる (#4 の「報告でなくファイルで確認」と同族)。

**接地の限界**: 04:19 の失敗ログは進捗ログの写し (スクリーンショット) のみで、当該プロセスの環境は取れていない。

**決着 (同日、ユーザー実機)**: 設定『OAuth ログインを使う』(= 子に ANTHROPIC_API_KEY を渡さない) を ON にしたら通った。
鍵を外すだけで直ったので、原因は**起動元端末の ANTHROPIC_API_KEY が無効な値**でほぼ確定 (端末側の長さ・指紋の比較は未実施)。

**決着 (同日、ユーザー実機)**: rev3 で GUI から実行 → product カットに Fuseforks の実画面が背景に合成され、その画像 +
motion_prompt を MiniMax (Hailuo) の i2v に渡した動画が「綺麗にできた」(フレーム 3 枚: mood 1 = 光る網状ノードの情景、
product 2 = 暗い舞台に実 UI が落ち影つきで浮く)。**合成 = 忠実さの構造保証**が実運用で効いた最初の 1 本。
接地の限界: n=1 (Fuseforks)。日本語見出しは未焼き込み。角度つき (斜め置き) の合成は未実装で、正面配置のみ。

## image_gen / pipeline (2026-09-08 GUI 実測 — 参照画像が「スクショに全く従わない、ありえない画面」)

### 8. 実スクショを「参考」で渡すと、画像モデルは画面を発明する — 忠実さはプロンプトでは保証できない

**症状** (ユーザー実機、Gemini): UI スナップショットを参照画像として添付し、palette のアンカーを付けて
生成した製品カットが、実際のアプリ画面と無関係な UI を描いた。ありえない画面が並ぶ。

**真因**: 2 つ重なった。①経路が「参照 = 見た目を寄せる素材」(Kataribe spec 25 のキャラ設定画集の流儀) で、
画素の忠実な再現を頼む経路ではない。②しかも #85 の規律「参照を指す語を書かない」を UI スクショにも
適用していたので、プロンプトは「暖色の机」を描けとだけ言い、画面の中身は完全にモデルの創作になる。
キャラの「見た目を寄せる」と UI の「そのまま出す」は要求が逆で、同じ規律を当てたのが誤り。

**処方 (rev3)**: 製品カットは**モデルに背景だけ描かせ、実スクショの画素を Rust が貼る**
(`image_gen::compose::composite_product_cut`: cover で敷いた背景 + 等比縮小・角丸・落ち影のスクショを中央へ)。
ScenePlan に `cut_kind: product|mood` / `snapshot_index` / `motion_prompt` を追加し、product の `image_prompt` は
背景のみ (画面・端末・文字の語を validate で弾く)。背景生成が失敗しても palette の単色で合成する。
参照画像を「寄せる」目的で送るのは mood カットだけ。「LLM は書き、Rust は検める」の画像版 =
**モデルは舞台を描き、Rust が画面を貼る**。

**一般化**: 出力の一部が**入力と同一でなければならない**なら、その部分は生成に通さず合成する。
プロンプトの忠実さは確率で、合成の忠実さは構造。#85 (入力を指す語) と本件は同じ「入力の扱い」の問題だが、
向きが逆 — キャラは寄せる (指すな)、UI は写す (生成させるな)。

**接地の限界**: 合成の目視は Neo 側 (Gemini 背景 + Kataribe UI で 1 枚)。実 run (LLM が product カットを
選び、背景だけ描く) の live は未実施。日本語見出しの焼き込みは v1 では持たない (フォント同梱が要る)。
