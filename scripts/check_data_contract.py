#!/usr/bin/env python3
"""data_contract.yaml が YAML として読めるか、重複したキーが無いかを検める (2026-09-12)。

台帳は人が読む正本で、プログラムからは読まれていない。そのため構文が壊れても誰も気づかない —
rev11 でブロックを別の mapping の途中に挿し込んだ時、後ろの 9 キーが親 (ImageGenConfig) から
切り離されたまま残り、Rust の擬似記法の行も 5 ブロックで YAML として読めなかった。
PyYAML は重複したキーを黙って後勝ちにするので、ここでは重複もエラーにする。

使い方: python scripts/check_data_contract.py [path]   (PyYAML が要る)
"""
import sys

import yaml


class UniqueKeyLoader(yaml.SafeLoader):
    pass


def _mapping(loader, node, deep=False):
    seen = set()
    for key_node, _ in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in seen:
            raise yaml.constructor.ConstructorError(None, None, f"重複したキー {key!r}", key_node.start_mark)
        seen.add(key)
    return loader.construct_mapping(node, deep)


UniqueKeyLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _mapping)


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    path = sys.argv[1] if len(sys.argv) > 1 else "data_contract.yaml"
    with open(path, encoding="utf-8") as f:
        text = f.read()
    try:
        doc = yaml.load(text, Loader=UniqueKeyLoader)
    except yaml.YAMLError as e:
        print(f"NG {path}: {e}")
        return 1
    print(f"OK {path}: トップレベル {len(doc)} ブロック")
    return 0


if __name__ == "__main__":
    sys.exit(main())
