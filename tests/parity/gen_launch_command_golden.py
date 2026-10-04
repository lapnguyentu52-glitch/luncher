"""A/B parity goldens — dựng lệnh launch qua minecraft-launcher-lib THẬT (8.0).

Chạy: python tests/parity/gen_launch_command_golden.py
Output: crates/antares-launch/fixtures/mll_command_golden.json
Integration test: crates/antares-launch/tests/mll_parity.rs (Linux — golden tạo
trên Linux; test cfg(target_os = "linux") để CI Windows không lệch separator).

Game dir dùng path TƯƠNG ĐỐI ("fixtures/game") + cwd = crate → golden không
dính đường dẫn máy.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
CRATE = REPO / "crates" / "antares-launch"
GAME_DIR = "fixtures/game"

# Options đồng bộ 1-1 với CommandOptions trong test Rust (legacy._build_options
# truyền đúng các key này: username/uuid/token/executablePath/jvmArguments/gameDirectory).
OPTIONS = {
    "username": "Steve",
    "uuid": "00000000000000000000000000000001",
    "token": "tok",
    "executablePath": "java",
    "jvmArguments": ["-Xms512M", "-Xmx2048M"],
    "gameDirectory": GAME_DIR,
}


def build(version: str) -> list[str]:
    import minecraft_launcher_lib.command as mll_command

    cwd = Path.cwd()
    try:
        os.chdir(CRATE)
        return mll_command.get_minecraft_command(version, GAME_DIR, OPTIONS)
    finally:
        os.chdir(cwd)


def main() -> int:
    try:
        import minecraft_launcher_lib  # noqa: F401
    except ImportError:
        print("minecraft-launcher-lib chưa cài (pip install -r requirements.txt)", file=sys.stderr)
        return 1

    golden = {"modern": build("1.21.11"), "legacy": build("1.8.9")}
    out = CRATE / "fixtures" / "mll_command_golden.json"
    out.write_text(json.dumps(golden, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out} — " + ", ".join(f"{k}: {len(v)} args" for k, v in golden.items()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
