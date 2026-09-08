# AppPromoVideo

既存アプリのリポジトリと UI スナップショットから、動画生成 AI (Veo / Sora / MiniMax 等) にそのまま貼れる
**シーン別プロンプト**と**参照画像**を出すデスクトップツール (Tauri 2 + Vue 3 + Rust)。

- LLM はローカル CLI（`claude -p` / Aider / custom）をサブプロセスとして実行する。HTTP API を直接は呼ばない。
- 画像生成は OpenAI Images / Gemini / ComfyUI。スナップショットを参照画像として送り、アプリの見た目に寄せる。
- 動画そのものは作らない。成果物はプロンプトと参照画像のパッケージ（`promo.json` + 画像 + Markdown）。

仕様は `specs/`、名詞は `data_contract.yaml`、作業規律は `CLAUDE.md`、既知の罠は `failures.md` を参照。

## 構成（ワークスペース）

Rust ワークスペース 4 crate + Tauri アプリ（`app/`）から成る。

| crate | 役割 |
|---|---|
| `promo_core` | プロセス/HTTP通信を持たない純関数の中核ライブラリ。`RepoBrief`（リポジトリ圧縮）、`ScenePlan` の型定義とJSON Schema検証、`fenced_json`（LLM非構造化出力からのJSON救済）、プロンプト・エクスポート用テキスト生成を担当。 |
| `cli_runner` | ローカル LLM CLI をサブプロセスとして安全に呼び出すランナー。argv 構築、NDJSON/`stream-json` 解析、認証系環境変数のスクラブ、子孫プロセスの確実な kill（Windows Job Object 等）を提供。 |
| `image_gen` | 参照画像・製品カット（背景 + 実スクショの合成、丸角・落ち影）の生成。ComfyUI キュー待機、支配色抽出（palette）、フォント列挙・見出し焼き込み（caption）を担当。 |
| `pipeline` | 上記を結線する IO ハーネス。ファイル収集（`collect`）、LLM 対話・シーン構成・検査ループ（`stages`）、参照画像生成指示（`reference`）、成果物パッケージ書き出し（`export`）を実行し、CLI バイナリ `promo` を提供する。 |
| `app/` | Tauri 2 + Vue 3 の GUI。上記 crate を `#[tauri::command]` 経由で呼び出す。 |

### `promo` CLI サブコマンド

```
promo run <repo> ...      # 解析 → シーン構成 → 画像生成 → パッケージ出力を一括実行
promo brief <repo> ...    # リポジトリの RepoBrief を出力（LLM 不使用）
promo images <promo.json> ... # 既存プランに対し参照画像・シーン画像を後付け生成
promo caption <in.png> <font> <index> <text> <out.png>  # 見出し焼き込み（目視確認用、LLM不使用）
promo fonts               # 利用可能フォント一覧
promo compose <backdrop.png> <screenshot.png> <out.png> [16:9|9:16|1:1]  # 背景とスクショの合成（目視確認用、LLM不使用）
```

## 動作要件

- Rust: edition `2024` / rust-version `1.85` 以上
- Node.js（`app/` のフロントエンドビルド用。バージョン指定なし、安定版推奨）
- ローカル LLM CLI（`claude` 等）が PATH 上にあり、認証済みであること
- OS: Windows で実機動作確認済み。Unix 系は未コンパイル・未検証。

## ビルド・起動

```bash
# Rust ワークスペースのテスト
cargo test --workspace

# GUI 開発起動
cd app
npm install
npm run tauri dev
# sccache 由来のビルドエラーが出る場合は RUSTC_WRAPPER= を付けて実行

# フロントエンド単体テスト・型検査・ビルド
cd app
npx vitest run
npm run build

# バックエンド単体テスト・Lint
cd app/src-tauri
cargo test
cargo clippy
```

主な依存バージョン: `tauri` 2 系、フロントエンドは `vue` ^3.5 / `pinia` ^3 / `@tauri-apps/api` ^2（`@tauri-apps/cli` ^2）/ `vite` ^6 / `typescript` ~5.6。

## GUI の使い方

1. 起動時に CLI の疎通確認（`check_cli`）が走る。
2. 左ペイン（`InputPane`）で対象リポジトリを選択し、必要に応じてスナップショット画像を追加（ファイル選択 / ドラッグ&ドロップ / クリップボード貼り付け）。動画イメージ（コンセプト）・尺（15/30/60秒）・比率（16:9/9:16/1:1）・コピー言語を設定する。
3. 「解析 → シーン構成」を実行すると `run_pipeline` が走り、右ペイン（`LogPanel`）にリアルタイムで進捗が表示される。
4. 中央ペイン（`ScenePanel`）にアプリ要約・差別化ポイント・シーン一覧が表示される。プロンプトはシーン単位／全シーン一括でクリップボードにコピーできる。
5. 必要なら「参照画像を生成」を実行（OpenAI Images / Gemini / ComfyUI のいずれか）。
6. 「フォルダを開く」または「別フォルダへ書き出し」で成果物パッケージ（`promo.json` + 画像 + Markdown）を保存する。

設定（LLM CLI の種類・モデル・タイムアウト、画像生成プロバイダの API キー、フォント）は `SettingsDialog` から行う。UI 設定はアプリデータディレクトリの `settings.json` に、画像生成 API キーは同ディレクトリの `.env` に保存される（フロントエンドには API キーの有無 (bool) のみを渡し、平文は WebView に露出させない）。

## 画像生成プロバイダの検証状況

- **ComfyUI**: HTTPポーリング方式（`/prompt` → `/history` → `/view`）で実機確認済み。無キーで動作。
- **Gemini**: 実機での画像生成を確認済み。
- **OpenAI (`images/edits`)**: 安定稼働は参照画像 1 枚。契約書に記載のあった「16 枚」は未検証の値であり、実測に基づくものではない。

## 既知の制約

- GUI からの実行でのみ LLM CLI が 401 (`authentication_failed`) を繰り返すことがある。設定画面の「OAuth ログインを使う」（子プロセスに `ANTHROPIC_API_KEY` を渡さない）を有効にすると解消する。
- その他の実装上の罠・回避策は `failures.md` に集約している（PowerShell のリダイレクト競合、Rust 2024 での `gen` 予約語化など）。

## ライセンス

MIT
