import { describe, expect, it } from "vitest";
import { usesAnthropicAuth } from "./settings";

// rev52 (ユーザー 2026-09-14): 「OAuth ログインを使う — ANTHROPIC_API_KEY / ANTHROPIC_AUTH_TOKEN を子 CLI に渡さない」が
// agy でも出ていて、agy に鍵を送ると読めた。agy には鍵を常に渡さない (backend の env_remove_for) ので、画面にも出さない。
// rev53 (同日): aider / custom も同様に出さない。スイッチと Anthropic の診断は claude のためのもの。
describe("usesAnthropicAuth", () => {
  it("agy では Anthropic の鍵と OAuth の設定を出さない", () => {
    expect(usesAnthropicAuth("agy")).toBe(false);
  });

  it("aider / custom でも出さない", () => {
    expect(usesAnthropicAuth("aider")).toBe(false);
    expect(usesAnthropicAuth("custom")).toBe(false);
  });

  it("claude だけが出す", () => {
    expect(usesAnthropicAuth("claude")).toBe(true);
  });
});
