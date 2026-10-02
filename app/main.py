"""Antares Launcher — entry point nhỏ (spec 1.3.1: main.py < 150 LOC).

Chỉ làm: startup -> bootstrap -> runtime -> run.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from app.bootstrap import bootstrap  # noqa: E402
from app.runtime import AppRuntime  # noqa: E402
from app.version import APP_NAME, APP_VERSION  # noqa: E402


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(prog=APP_NAME)
    parser.add_argument("--dev", action="store_true", help="dev mode: verbose log + devtools")
    parser.add_argument("--safe-mode", action="store_true", help="tắt animation/plugin để debug")
    parser.add_argument("--diagnose", action="store_true", help="in thông tin hệ thống rồi thoát")
    parser.add_argument("--headless", action="store_true", help="chạy không cửa sổ (test)")
    return parser.parse_args(argv)


def _ui_dir() -> Path:
    """Thư mục frontend — tuyệt đối, đúng cả dev lẫn PyInstaller (mục 49)."""
    if getattr(sys, "frozen", False):
        base = Path(getattr(sys, "_MEIPASS", Path(sys.executable).parent))
        return base / "frontend"
    return ROOT / "frontend"


def _ui_path() -> Path:
    """Đường dẫn tuyệt đối tới frontend/index.html (fallback/kiểm tra nhanh)."""
    return _ui_dir() / "index.html"


def diagnose() -> int:
    import platform

    print(f"App: {APP_NAME}")
    print(f"Python: {platform.python_version()} ({platform.python_implementation()})")
    print(f"OS: {platform.platform()}")
    try:
        import psutil

        vm = psutil.virtual_memory()
        print(f"RAM: total={vm.total >> 20}MB available={vm.available >> 20}MB")
    except Exception:
        print("RAM: psutil unavailable")
    try:
        import webview  # noqa: F401

        print("WebView: pywebview available")
    except ImportError:
        print("WebView: pywebview NOT installed (pip install pywebview)")
    return 0


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.diagnose:
        return diagnose()

    # --safe-mode = chế độ debug: bật verbose log (dev_mode) + frontend tự giảm hiệu ứng
    ctx = bootstrap(dev_mode=args.dev or args.safe_mode, root=ROOT)
    runtime = AppRuntime(ctx)

    if args.headless:
        runtime.run_headless()
        return 0

    try:
        import webview  # optional dependency

        from api.bridge.api import AntaresApi
        from app.ui_server import UIServer

        api = AntaresApi(ctx)

        # Phải serve qua http://127.0.0.1 — load file:// sẽ bị Chromium/WebView2
        # chặn ES module (origin null) -> JS không chạy -> splash nhấp nháy mãi.
        ui = UIServer(_ui_dir())
        ui_url = ui.start()

        window = webview.create_window(
            APP_NAME,
            ui_url,
            js_api=api,
            width=1280,
            height=800,
            min_size=(960, 640),
        )
        ctx.set("window", window)
        ctx.set("ui_server", ui)
        # Bật kênh đẩy event (thread flush) — UI không cần poll định kỳ.
        api.attach_window(window)
        runtime.start_background()
        try:
            webview.start()
        finally:
            ui.stop()
            runtime.request_shutdown()
    except ImportError:
        print("pywebview chưa cài — chạy GUI: pip install pywebview")
        print("Hoặc chạy headless: python -m app.main --headless")
        runtime.run_headless()
    except Exception as e:
        # Không có GUI backend (server không display, thiếu GTK/QT...) —
        # rơi về headless thay vì crash (spec mục 53: app không được chết trắng).
        print(f"Không mở được cửa sổ GUI ({e}) — chuyển sang headless.")
        runtime.run_headless()
    return 0


if __name__ == "__main__":
    sys.exit(main())
