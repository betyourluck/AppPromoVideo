import { describe, expect, it } from "vitest";
import { usesAnthropicAuth } from "./settings";

// rev52 (ユーザー 2026-09-14): 「OAuth ログインを使う — ANTHROPIC_API_KEY / ANTHROPIC_AUTH_TOKEN を子 CLI に渡さない」が
// agy でも出ていて、agy に鍵を送ると読めた。agy には鍵を常に渡さない (backend の env_remove_for) ので、画面にも出さない。
describe("usesAnthropicAuth", () => {
  it("agy では Anthropic の鍵と OAuth の設定を出さない", () => {
    expect(usesAnthropicAuth("agy")).toBe(false);
  });

  it("claude / aider / custom ではスイッチが効くので出す", () => {
    for (const k of ["claude", "aider", "custom"] as const) expect(usesAnthropicAuth(k)).toBe(true);
  });
});
