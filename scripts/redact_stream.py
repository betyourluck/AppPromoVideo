"""stream-json の生ログから、環境依存の値を伏せて fixture にする (hook 行は落とす)。"""
import json, sys
src, dst = sys.argv[1], sys.argv[2]
out = []
for line in open(src, encoding="utf-8"):
    line = line.strip()
    if not line:
        continue
    o = json.loads(line)
    # hook 行は環境依存なので落とす。init と api_retry (再試行の実データ) は残す。
    if o.get("type") == "system" and o.get("subtype") not in ("init", "api_retry"):
        continue
    if o.get("type") == "system":
        o["tools"] = o.get("tools", [])[:3]
        o["cwd"] = "<cwd>"
        for k in ("mcp_servers", "plugins", "slash_commands", "skills", "agents"):
            o.pop(k, None)
    out.append(json.dumps(o, ensure_ascii=False))
open(dst, "w", encoding="utf-8").write("\n".join(out) + "\n")
print(f"{len(out)} lines -> {dst}")
