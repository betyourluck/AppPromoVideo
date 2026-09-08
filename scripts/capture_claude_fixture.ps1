# claude -p --json-schema の成功ログを fixture として採取する (spec 01 rev2 決定 3 / 査読 4)。
# Neo のセッション内では OAuth が継承されず採取できない → ユーザーの通常端末で実行する。
#   pwsh scripts/capture_claude_fixture.ps1
# 出力: crates/cli_runner/fixtures/claude_json_schema_ok.jsonl (cwd / tools / hook 出力を伏せた版)
#
# 2026-09-08: パイプライン内の native コマンドに `2> file` を付けると PowerShell 7 が同じファイルを
# 二重に開き "being used by another process" で落ちた。リダイレクトは Start-Process に任せる。
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$scratch = Join-Path $env:TEMP "apppromo_fixture"
New-Item -ItemType Directory -Force $scratch | Out-Null

$promptFile = Join-Path $scratch "prompt.txt"
$raw = Join-Path $scratch "raw.jsonl"
$err = Join-Path $scratch "stderr.txt"
Remove-Item -Force -ErrorAction SilentlyContinue $raw, $err
Set-Content -Path $promptFile -Value 'Reply with ok=true and note="fixture".' -Encoding utf8 -NoNewline

$schema = '{"type":"object","properties":{"ok":{"type":"boolean"},"note":{"type":"string"}},"required":["ok","note"]}'
$claude = (Get-Command claude).Source
# Start-Process は引数を空白で連結するだけで引用符を付けない → Windows CRT の規則で `"` を `\"` に逃がし、
# 全体を `"` で囲む (2026-09-08 実測: 素で渡すと "--json-schema is not valid JSON: Expected '}'")。
$schemaArg = '"' + ($schema -replace '"', '\"') + '"'
$args = @("-p", "--output-format", "stream-json", "--verbose", "--max-turns", "1", "--model", "haiku", "--json-schema", $schemaArg)

$p = Start-Process -FilePath $claude -ArgumentList $args -WorkingDirectory $scratch `
  -RedirectStandardInput $promptFile -RedirectStandardOutput $raw -RedirectStandardError $err `
  -NoNewWindow -Wait -PassThru
Write-Host "claude exit=$($p.ExitCode)  raw=$raw"
if (Test-Path $err) { Get-Content $err | Select-Object -First 5 }

$dst = Join-Path $root "crates/cli_runner/fixtures/claude_json_schema_ok.jsonl"
python (Join-Path $PSScriptRoot "redact_stream.py") $raw $dst
# 成功 (result.is_error == false かつ structured_output あり) の時だけ fixture として残す。
$ok = (Test-Path $dst) -and ((Get-Content $dst -Raw) -match '"is_error":\s*false') -and ((Get-Content $dst -Raw) -match 'structured_output')
if (-not $ok) {
  Write-Host "NOT a success run (auth / schema / exit error) -> fixture removed. raw kept at $raw"
  Remove-Item -Force -ErrorAction SilentlyContinue $dst
  exit 1
}
Write-Host "SUCCESS fixture -> $dst"
