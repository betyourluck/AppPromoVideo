/**
 * 実行状態 (Pinia)。backend の event `promo-progress` を受けてログに積み、command の結果を持つ。
 * 状態の真実は backend / export フォルダにあり、ここは描画用のスナップショット。
 */
import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { BriefPreview, CliCheck, FontEntry, ImagesResult, OpenedRun, Progress, RunListItem, RunResult, SnapshotMeta } from "./types";
import { mergePaths } from "./snapshots";
import {
  KEYS,
  loadCliSettings,
  loadImageGenSettings,
  loadProjectSettings,
  pickCaptionFont,
  save,
  toBackendCaption,
  toBackendCli,
  toBackendConfig,
  type CliSettings,
  type ImageGenSettings,
  type ProjectSettings,
} from "./settings";

export interface LogLine {
  ts: number;
  stage: string;
  text: string;
}

const LOG_MAX = 2000;

export const useStore = defineStore("main", {
  state: () => ({
    cli: loadCliSettings() as CliSettings,
    image: loadImageGenSettings() as ImageGenSettings,
    project: loadProjectSettings() as ProjectSettings,
    running: false as boolean,
    imaging: false as boolean,
    log: [] as LogLine[],
    result: null as RunResult | null,
    images: null as ImagesResult | null,
    /** 参照画像の data URL (scene_id → url)。 */
    imageUrls: {} as Record<number, string>,
    /** スナップショットのサムネイル data URL (path → url)。 */
    snapshotUrls: {} as Record<string, string>,
    error: "" as string,
    toast: "" as string,
    brief: null as BriefPreview | null,
    cliCheck: null as CliCheck | null,
    unlisten: null as UnlistenFn | null,
    /** 過去の run (新しい順)。rev7: 実行のたびに前回が消えていたのを直したうえで一覧する。 */
    runs: [] as RunListItem[],
    /** 比較に選んだ run_dir (最大 2 つ)。 */
    compare: [] as string[],
    /** 比較用に読んだ run の画像 URL: run_dir → (scene_id → data URL)。 */
    compareUrls: {} as Record<string, Record<number, string>>,
  }),
  actions: {
    persist() {
      save(KEYS.cli, this.cli);
      save(KEYS.image, this.image);
      save(KEYS.project, this.project);
    },
    push(stage: string, text: string) {
      this.log.push({ ts: Date.now(), stage, text });
      if (this.log.length > LOG_MAX) this.log.splice(0, this.log.length - LOG_MAX);
    },
    async listenProgress() {
      if (this.unlisten) return;
      try {
        this.unlisten = await listen<Progress>("promo-progress", (e) => this.push(e.payload.stage, e.payload.text));
      } catch (e) {
        console.warn("[store] event listen unavailable:", e);
      }
    },
    showToast(text: string) {
      this.toast = text;
      setTimeout(() => {
        if (this.toast === text) this.toast = "";
      }, 4000);
    },
    // --- スナップショット (ドロップ / 貼り付け / ダイアログ) ---
    async loadSnapshotUrls() {
      for (const p of this.project.snapshots) {
        if (this.snapshotUrls[p]) continue;
        try {
          this.snapshotUrls[p] = await invoke<string>("image_data_url", { path: p });
        } catch (e) {
          this.push("error", `サムネイルを読めません ${p}: ${e}`);
        }
      }
    },
    /** パス列を検証して足す (ドロップとダイアログの共通経路)。画像以外・壊れたものはログに出して飛ばす。 */
    async addSnapshotPaths(paths: string[]) {
      const { next, skipped } = mergePaths(this.project.snapshots, paths);
      for (const s of skipped) this.push("error", `画像ではないので飛ばしました: ${s}`);
      const added = next.filter((p) => !this.project.snapshots.includes(p));
      for (const p of added) {
        try {
          await invoke<SnapshotMeta>("validate_snapshot", { path: p });
          this.project.snapshots.push(p);
        } catch (e) {
          this.push("error", String(e));
        }
      }
      this.persist();
      await this.loadSnapshotUrls();
      if (added.length) this.showToast(`スナップショット ${added.length} 枚を追加`);
    },
    async pickSnapshots() {
      const files = await invoke<string[]>("pick_images");
      await this.addSnapshotPaths(files);
    },
    /** クリップボードの画像 (data URL の base64) を backend に保存して足す。 */
    async addClipboardImage(base64: string, mime: string) {
      try {
        const meta = await invoke<SnapshotMeta>("save_clipboard_image", { base64, mime });
        await this.addSnapshotPaths([meta.path]);
      } catch (e) {
        this.push("error", `貼り付けに失敗: ${e}`);
      }
    },
    removeSnapshot(i: number) {
      const [p] = this.project.snapshots.splice(i, 1);
      if (p) delete this.snapshotUrls[p];
      this.persist();
    },
    async checkCli() {
      const b = toBackendCli(this.cli);
      try {
        this.cliCheck = await invoke<CliCheck>("check_cli", { executable: b.executable });
      } catch (e) {
        this.cliCheck = {
          found: false,
          version: "",
          error: String(e),
          auth: { api_key_present: false, api_key_len: 0, api_key_fingerprint: "", auth_token_present: false, base_url: "", oauth_logged_in: null, oauth_method: "", scrubbed: [] },
        };
      }
    },
    async previewBrief() {
      this.error = "";
      try {
        this.brief = await invoke<BriefPreview>("brief_preview", {
          projectPath: this.project.projectPath,
          snapshotPaths: this.project.snapshots,
        });
      } catch (e) {
        this.brief = null;
        this.error = String(e);
      }
    },
    async run() {
      if (this.running) return;
      this.error = "";
      this.result = null;
      this.images = null;
      this.imageUrls = {};
      this.persist();
      this.running = true;
      this.push("ui", "実行開始");
      try {
        const res = await invoke<RunResult>("run_pipeline", {
          req: {
            project_path: this.project.projectPath,
            snapshot_paths: this.project.snapshots,
            concept: this.project.concept,
            seconds: this.project.seconds,
            aspect: this.project.aspect,
            language: this.project.lang,
            export_dir: this.project.exportDir || ".",
            cli: toBackendCli(this.cli),
            plate_mode: this.image.plateMode,
          },
        });
        this.result = res;
        this.showToast(`完了: ${res.plan.attempts} 回目で通過、${(res.analyze.cost_usd + res.plan.cost_usd).toFixed(3)} USD`);
        if (this.image.enabled) await this.makeImages();
        await this.loadRuns();
      } catch (e) {
        this.error = String(e);
        this.push("error", String(e));
      } finally {
        this.running = false;
      }
    },
    async cancel() {
      try {
        const ok = await invoke<boolean>("cancel_run");
        this.push("ui", ok ? "中断を要求しました" : "実行中の処理はありません");
      } catch (e) {
        this.push("error", String(e));
      }
    },
    async loadRuns() {
      try {
        this.runs = await invoke<RunListItem[]>("list_runs");
      } catch (e) {
        this.push("error", `履歴を読めません: ${e}`);
      }
    },
    /** 過去の run を結果ペインに戻す。正本は run_dir/promo.json (索引ではない)。 */
    async openRun(runDir: string) {
      try {
        const r = await invoke<OpenedRun>("open_run", { runDir });
        this.result = {
          promo: r.promo,
          package_dir: r.package_dir,
          analyze: { attempts: 0, cost_usd: 0, duration_ms: 0, violations: [] },
          plan: { attempts: 0, cost_usd: 0, duration_ms: 0, violations: [] },
          brief_chars: 0,
        };
        this.images = { promo: r.promo, results: r.images, palette: [], anchor: "", truncated: null };
        this.imageUrls = {};
        for (const im of r.images) {
          if (im.ok && im.path) {
            try {
              this.imageUrls[im.scene_id] = await invoke<string>("image_data_url", { path: im.path });
            } catch {
              /* 1 枚読めなくても残りは出す */
            }
          }
        }
        this.showToast(`${r.promo.summary.app_name} の run を開きました`);
      } catch (e) {
        this.error = String(e);
        this.push("error", String(e));
      }
    },
    /** 比較の選択をトグルする (2 つまで。3 つ目を押したら古い方を落とす)。 */
    async toggleCompare(runDir: string) {
      const at = this.compare.indexOf(runDir);
      if (at >= 0) {
        this.compare.splice(at, 1);
        return;
      }
      this.compare.push(runDir);
      if (this.compare.length > 2) this.compare.shift();
      if (!this.compareUrls[runDir]) await this.loadCompareUrls(runDir);
    },
    async loadCompareUrls(runDir: string) {
      try {
        const r = await invoke<OpenedRun>("open_run", { runDir });
        const urls: Record<number, string> = {};
        for (const im of r.images) {
          if (im.ok && im.path) {
            try {
              urls[im.scene_id] = await invoke<string>("image_data_url", { path: im.path });
            } catch {
              /* 欠けは空欄で出す */
            }
          }
        }
        this.compareUrls[runDir] = urls;
      } catch (e) {
        this.push("error", `比較用に読めません: ${e}`);
      }
    },
    async forgetRun(runDir: string, deleteFiles: boolean) {
      try {
        await invoke<boolean>("forget_run", { runDir, deleteFiles });
        this.compare = this.compare.filter((d) => d !== runDir);
        delete this.compareUrls[runDir];
        await this.loadRuns();
        this.showToast(deleteFiles ? "run を削除しました" : "履歴から外しました");
      } catch (e) {
        this.error = String(e);
        this.push("error", String(e));
      }
    },
    /**
     * 見出しが ON なのにフォント未選択なら、日本語グリフを持つものを自動で選ぶ。
     * 既定 ON にした以上、「ON なのに何も焼かれない」を無言で起こさせない。
     * 選べなければ理由をログに出して**焼かずに進む** (画像は出す)。
     */
    async ensureCaptionFont() {
      const c = this.image.caption;
      if (!c.enabled || c.fontPath.trim()) return;
      try {
        const fonts = await invoke<FontEntry[]>("list_fonts");
        const pick = pickCaptionFont(fonts);
        if (!pick) {
          this.push("images", "日本語のフォントが見つからないので見出しは焼きません (設定 → 画像 で選べます)");
          return;
        }
        c.fontPath = pick.path;
        c.fontIndex = pick.index;
        this.persist();
        this.push("images", `見出しのフォントを自動選択: ${pick.family}`);
      } catch (e) {
        this.push("images", `フォント一覧を取れないので見出しは焼きません: ${e}`);
      }
    },
    /**
     * 1 シーンだけ見出しを焼き直す (rev9)。**生成 API は呼ばない** —
     * base/ に残した焼く前の合成から焼くので、何度やっても劣化せず、お金もかからない。
     * `spec` が null なら見出しを消す。
     */
    async reburnCaption(
      sceneId: number,
      spec: ReturnType<typeof toBackendCaption>,
      plate: Record<string, number> | null = null,
      newCopy: string | null = null,
    ) {
      const runDir = this.result?.package_dir;
      if (!runDir) return;
      try {
        const url = await invoke<string>("reburn_caption", { runDir, sceneId, spec, plate, newCopy });
        this.imageUrls[sceneId] = url;
        // コピー文を書き換えたら手元の plan も揃える (scenes.md / クリップボードが食い違わないように)。
        if (newCopy !== null && this.result) {
          const sc = this.result.promo.plan.scenes.find((s) => s.scene_id === sceneId);
          if (sc) sc.copy_text = newCopy;
        }
      } catch (e) {
        this.error = String(e);
        this.push("error", String(e));
      }
    },
    async makeImages() {
      if (!this.result || this.imaging) return;
      await this.ensureCaptionFont();
      this.imaging = true;
      this.error = "";
      this.persist();
      try {
        const res = await invoke<ImagesResult>("generate_images", {
          req: {
            promo: this.result.promo,
            package_dir: this.result.package_dir,
            image: toBackendConfig(this.image, this.result.promo.plan.aspect),
            snapshot_paths: this.project.snapshots,
            max_scenes: this.image.maxScenes > 0 ? this.image.maxScenes : null,
            requested_refs: this.image.requestedRefs > 0 ? this.image.requestedRefs : null,
            caption: toBackendCaption(this.image.caption),
            plate_mode: this.image.plateMode,
          },
        });
        this.images = res;
        this.result = { ...this.result, promo: res.promo };
        for (const r of res.results) {
          if (r.ok && r.path) {
            try {
              this.imageUrls[r.scene_id] = await invoke<string>("image_data_url", { path: r.path });
            } catch (e) {
              this.push("error", `表示できません ${r.path}: ${e}`);
            }
          }
        }
        const ok = res.results.filter((r) => r.ok).length;
        this.showToast(`参照画像 ${ok}/${res.results.length} 枚`);
      } catch (e) {
        this.error = String(e);
        this.push("error", String(e));
      } finally {
        this.imaging = false;
      }
    },
  },
});
