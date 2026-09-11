/**
 * 辞書の文言の**強調と等幅**を解析する (rev30)。表示は `components/Rich.vue`。
 *
 * 認める印は `<b>…</b>` と `<mono>…</mono>` だけ。それ以外の山括弧は文字のまま残す —
 * `v-html` を使わずに済ませ、辞書にも差し込む値にも HTML を注入させないため。
 * 閉じ忘れは文末まで効かせ、対の無い閉じ印は無視する (どちらも文字を落とさない)。
 */
export interface RichSegment {
  text: string;
  b: boolean;
  mono: boolean;
}

const MARK = /<(\/?)(b|mono)>/g;

export function parseRich(text: string): RichSegment[] {
  const out: RichSegment[] = [];
  let b = 0;
  let mono = 0;
  const pushText = (s: string) => {
    if (!s) return;
    const prev = out[out.length - 1];
    if (prev && prev.b === b > 0 && prev.mono === mono > 0) prev.text += s;
    else out.push({ text: s, b: b > 0, mono: mono > 0 });
  };
  let last = 0;
  for (const m of text.matchAll(MARK)) {
    pushText(text.slice(last, m.index));
    last = (m.index ?? 0) + m[0].length;
    const closing = m[1] === "/";
    if (m[2] === "b") b = closing ? Math.max(0, b - 1) : b + 1;
    else mono = closing ? Math.max(0, mono - 1) : mono + 1;
  }
  pushText(text.slice(last));
  return out;
}
