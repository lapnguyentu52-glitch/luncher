"""Frontend HTTP server — phục vụ UI qua 127.0.0.1 thay vì file://.

Vì sao cần (fix "nhấp nháy không vào"):
  WebView2/Chromium CHẶN `<script type="module">` khi trang load qua file://
  (origin null -> CORS chặn mọi import). Kết quả: JS không chạy, splash nhấp
  nháy mãi, app không bao giờ vào được. Serve qua http://127.0.0.1 giữ nguyên
  tính năng module + không mở ra ngoài mạng (bind loopback, port ephemeral).
"""
from __future__ import annotations

import threading
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

_MIME = {
    ".html": "text/html",
    ".js": "text/javascript",
    ".mjs": "text/javascript",
    ".css": "text/css",
    ".json": "application/json",
    ".svg": "image/svg+xml",
    ".png": "image/png",
    ".ico": "image/x-icon",
    ".woff2": "font/woff2",
}


class _Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, root: Path, **kwargs) -> None:
        super().__init__(*args, directory=str(root), **kwargs)

    def end_headers(self) -> None:
        # Không cache để dev/edit CSS-JS thấy ngay; directory listing tắt sẵn.
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def guess_type(self, path: str) -> str:  # noqa: D102
        ext = Path(path).suffix.lower()
        return _MIME.get(ext) or super().guess_type(path)

    def log_message(self, fmt, *args) -> None:  # im lặng — log của webview đủ rồi
        return


class UIServer:
    """HTTP server loopback phục vụ thư mục frontend."""

    def __init__(self, root: Path) -> None:
        self._root = Path(root)
        self._httpd: ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None
        self.url: str | None = None

    def start(self) -> str:
        handler = partial(_Handler, root=self._root)
        # port 0 = OS chọn port trống -> không xung đột, không mở ra LAN
        self._httpd = ThreadingHTTPServer(("127.0.0.1", 0), handler)
        port = self._httpd.server_address[1]
        self.url = f"http://127.0.0.1:{port}/index.html"
        self._thread = threading.Thread(target=self._httpd.serve_forever,
                                        name="antares-ui-server", daemon=True)
        self._thread.start()
        return self.url

    def stop(self) -> None:
        if self._httpd:
            self._httpd.shutdown()
            self._httpd.server_close()
        self._httpd = None
        self._thread = None
        self.url = None
