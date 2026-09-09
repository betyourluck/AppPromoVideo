# AppPromoVideo

A desktop tool (Tauri 2 + Vue 3 + Rust) that takes an existing app's repository and UI snapshots to generate **scene-by-scene prompts** and **reference images** ready to paste directly into video generation AI (such as Veo, Sora, or MiniMax).

- LLMs are executed as local CLI subprocesses (`claude -p`, Aider, custom, etc.). It does not call HTTP APIs directly.
- Image generation is handled by OpenAI Images, Gemini, or ComfyUI. Snapshots are sent as reference images to align with the app's visual style.
- It does not generate the video itself. The output is a prompt and reference image package (`promo.json` + images + Markdown).

For specifications, see `specs/`; for terminology, `data_contract.yaml`; for working rules, `CLAUDE.md`; and for known pitfalls, `failures.md`.

## Architecture (Workspace)

Consists of a Rust workspace with 4 crates and a Tauri application (`app/`).

| Crate | Role |
|---|---|
| `promo_core` | A pure-function core library without process or HTTP I/O. Handles type definitions and JSON Schema validation for `RepoBrief` (repository compression) and `ScenePlan`, `fenced_json` (JSON recovery from LLM unstructured output), and text generation for prompts and exports. |
| `cli_runner` | A runner that securely invokes local LLM CLIs as subprocesses. Provides argv construction, NDJSON/`stream-json` parsing, authentication environment variable scrubbing, and reliable descendant process termination (e.g., Windows Job Object). |
| `image_gen` | Generates reference images and product shots (compositing backgrounds + real screenshots with rounded corners and drop shadows). Handles ComfyUI queue waiting, dominant color extraction (palette), font enumeration, and heading burn-in (captioning). |
| `pipeline` | An I/O harness connecting the above components. Executes file collection (`collect`), LLM dialogue / scene planning / inspection loop (`stages`), reference image generation instructions (`reference`), and output package export (`export`), providing the `promo` CLI binary. |
| `app/` | Tauri 2 + Vue 3 GUI. Invokes the above crates via `#[tauri::command]`. |

### `promo` CLI Subcommands

```
promo run <repo> ...      # Batch execution: analysis -> scene planning -> image generation -> package export
promo brief <repo> ...    # Outputs the RepoBrief for the repository (no LLM used)
promo images <promo.json> ... # Post-generates reference and scene images for an existing plan
promo caption <in.png> <font> <index> <text> <out.png>  # Heading burn-in (for visual verification, no LLM used)
promo fonts               # Lists available fonts
promo compose <backdrop.png> <screenshot.png> <out.png> [16:9|9:16|1:1]  # Backdrop and screenshot composition (for visual verification, no LLM used)
```

## Requirements

- Rust: edition `2024` / rust-version `1.85` or higher
- Node.js (for building the `app/` frontend; no specific version required, stable recommended)
- Local LLM CLI (`claude`, etc.) available in PATH and authenticated
- OS: Verified on Windows. Unix-like systems are uncompiled and unverified.

## Build & Run

```bash
# Test the Rust workspace
cargo test --workspace

# Run GUI in development mode
cd app
npm install
npm run tauri dev
# If build errors occur due to sccache, run with RUSTC_WRAPPER=

# Frontend unit tests, type check, and build
cd app
npx vitest run
npm run build

# Backend unit tests and lint
cd app/src-tauri
cargo test
cargo clippy
```

Key dependency versions: `tauri` v2, frontend uses `vue` ^3.5 / `pinia` ^3 / `@tauri-apps/api` ^2 (`@tauri-apps/cli` ^2) / `vite` ^6 / `typescript` ~5.6.

## GUI Usage

1. Upon startup, CLI connectivity check (`check_cli`) runs.
2. Select the target repository in the left pane (`InputPane`), and optionally add snapshot images (file selection / drag & drop / clipboard paste). Configure video concept, duration (15/30/60s), aspect ratio (16:9/9:16/1:1), and copy language.
3. Clicking "Analyze -> Scene Plan" runs `run_pipeline`, displaying real-time progress in the right pane (`LogPanel`).
4. The center pane (`ScenePanel`) displays the app summary, differentiation points, and scene list. Prompts can be copied to the clipboard per scene or all at once.
5. If needed, click "Generate Reference Images" (using OpenAI Images, Gemini, or ComfyUI).
6. Save the output package (`promo.json` + images + Markdown) using "Open Folder" or "Export to Another Folder".

Settings (LLM CLI type/model/timeout, image generation provider API keys, fonts) are configured via `SettingsDialog`. UI settings are saved to `settings.json` in the app data directory, and image generation API keys are saved to `.env` in the same directory (only the presence/absence (bool) of API keys is passed to the frontend; plaintext keys are never exposed to the WebView).

## Image Generation Provider Verification Status

- **ComfyUI**: Verified on real hardware via HTTP polling method (`/prompt` -> `/history` -> `/view`). Works without an API key.
- **Gemini**: Image generation verified on real hardware.
- **OpenAI (`images/edits`)**: Stable operation verified with 1 reference image. The "16 images" mentioned in the contract is an unverified value and not based on actual measurements.

## Known Limitations

- Running via the GUI may occasionally cause the LLM CLI to repeatedly return 401 (`authentication_failed`). Enabling "Use OAuth login" in the settings screen (which does not pass `ANTHROPIC_API_KEY` to subprocesses) resolves this.
- Other implementation pitfalls and workarounds are consolidated in `failures.md` (e.g., PowerShell redirect conflicts, `gen` keyword reservation in Rust 2024).

## License

MIT
