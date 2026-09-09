# 依頼: README.md の英訳（README_en.md 作成）

## 目的
リポジトリ直下の `README.md`（日本語）を英語に翻訳し、`README_en.md` として新規作成する。

## 入力
- 翻訳元: `README.md`（作業フォルダ直下。file の read で全文を読むこと）

## 合格基準
- `README_en.md` が作業フォルダ直下に新規作成されていること（`file` の write、overwrite不要）。
- 見出し構成・箇条書き・表（crate一覧の表、コマンドのコードブロック）を原文と同じ構造で保持すること。
- コード部分（```bash や ``` のコマンド例、`promo run <repo> ...` 等のCLIサブコマンド一覧）は**翻訳せず原文のまま**。コマンド名・ファイル名・ライブラリ名（`promo_core`, `cli_runner`, `image_gen`, `pipeline`, `app/`, `tauri`, `vue`, `pinia` 等の固有名詞）も翻訳しない。
- 技術用語（RepoBrief, ScenePlan, fenced_json, ComfyUI, NDJSON, Job Object 等）は英語のまま（原文がすでに英語表記のため変更不要）。
- 自然な技術文書の英語にすること（直訳調は避ける）。
- 全文が欠けなく訳されていること（1回の write で収まらない場合は append で継ぎ足す）。

## 出力先
- 新規ファイル `README_en.md`（作業フォルダ直下）として保存する。
- 完了したら、作成したファイルのパスと概要（見出し数など）を返信で報告すること。
