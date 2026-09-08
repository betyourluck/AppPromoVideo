"""stream-json の生ログから、環境依存の値を伏せて fixture にする (hook 行は落とす)。

伏せるもの: cwd / hook 行 / mcp_servers 等の一覧 / **home path** / **UUID**。
home path と UUID は入れ子のどこに現れるか分からないので、行を JSON に直した後で正規表現で潰す。
UUID は空文字にしない — `session_id` が空でないことを前提にしたテストがあるため `<uuid>` に置き換える。
"""
import json, re, sys

HOME = re.compile(r"[A-Za-z]:\\\\Users\\\\[^\\\\\"]+|/home/[^/\"]+|/Users/[^/\"]+")
UUID = re.compile(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")


def redact(line: str) -> str:
    line = HOME.sub("<home>", line)
    return UUID.sub("<uuid>", line)

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
        for k in ("mcp_servers", "plugins", "slash_commands", "skills", "agents", "memory_paths", "terminal_slash_commands"):
            o.pop(k, None)
    out.append(redact(json.dumps(o, ensure_ascii=False)))
open(dst, "w", encoding="utf-8").write("\n".join(out) + "\n")
print(f"{len(out)} lines -> {dst}")
