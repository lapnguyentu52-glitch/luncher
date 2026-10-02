"""LogAnalyzer — phân tích log & crash report, trả insights + khuyến nghị (mục 41).

Nguồn: launcher ring buffer, latest.log của instance, crash-reports.
Nhận diện bằng blueprint (pattern + khuyến nghị) — KHÔNG LLM (mục 24).
"""
from __future__ import annotations

import re
import time

from app.context import AppContext
from core.logging.setup import get_logger

logger = get_logger("diagnostics.log_analyzer")

#: Thứ tự cố định để preview ổn định (mục 10.5).
_LOG_SOURCES = ("launcher", "minecraft", "crash")

#: Blueprint nhận diện lỗi quen thuộc: pattern -> id/seed/khuyến nghị i18n.
#: Match theo từng dòng, dùng cho launcher + minecraft; crash report có path riêng.
_BLUEPRINTS: tuple[dict, ...] = (
    {
        "id": "java_out_of_memory",
        "pattern": re.compile(r"OutOfMemoryError|java\.lang\.OutOfMemoryError", re.IGNORECASE),
        "seed": "oom",
        "recommend": "profiles.ramMax",
        "action": {"kind": "route", "target": "gameOptimization"},
    },
    {
        "id": "java_class_version",
        "pattern": re.compile(
            r"UnsupportedClassVersionError|bad major version|class file version", re.IGNORECASE),
        "seed": "javaVersion",
        "recommend": "nav.diagnostics",
        "action": {"kind": "route", "target": "diagnostics"},
    },
    {
        "id": "auth_failed",
        "pattern": re.compile(r"Invalid session|Invalid token|Failed to verify|"
                              r"AUTH_FAILED|authserver\.ely\.by", re.IGNORECASE),
        "seed": "auth",
        "recommend": "nav.accounts",
        "action": {"kind": "route", "target": "accounts"},
    },
    {
        "id": "java_missing",
        "pattern": re.compile(r"JAVA_NOT_FOUND|Java not found|No Java installation", re.IGNORECASE),
        "seed": "javaMissing",
        "recommend": "nav.diagnostics",
        "action": {"kind": "route", "target": "diagnostics"},
    },
    {
        "id": "mod_conflict",
        "pattern": re.compile(r"DuplicateModsFoundException|ModResolutionException|"
                              r"LoaderExceptionModCrash|FabricException", re.IGNORECASE),
        "seed": "modConflict",
        "recommend": "nav.mods",
        "action": {"kind": "route", "target": "mods"},
    },
    {
        "id": "connection_refused",
        "pattern": re.compile(r"Connection refused|ConnectionRefusedError|"
                              r"Failed to connect to the server", re.IGNORECASE),
        "seed": "connRefused",
        "recommend": "profiles.server",
        "action": {"kind": "net", "target": "tcp"},
    },
    {
        "id": "connection_timeout",
        "pattern": re.compile(r"Connection timed out|connect timed out|"
                              r"SocketTimeoutException", re.IGNORECASE),
        "seed": "connTimeout",
        "recommend": "profiles.server",
        "action": {"kind": "net", "target": "tcp"},
    },
    {
        "id": "disk_full",
        "pattern": re.compile(r"No space left on device|IOException.*space", re.IGNORECASE),
        "seed": "diskFull",
        "recommend": "nav.systemOptimization",
        "action": {"kind": "route", "target": "systemOptimization"},
    },
    {
        "id": "gpu_driver",
        "pattern": re.compile(
            r"GL_INVALID|OpenGL (error|context)|dxgi|WGL|EGL_NOT_INITIALIZED|"
            r"Failed to create OpenGL|Unable to initialize OpenGL", re.IGNORECASE),
        "seed": "gpuDriver",
        "recommend": "nav.gameOptimization",
        "action": {"kind": "route", "target": "gameOptimization"},
    },
    {
        "id": "asset_corrupt",
        "pattern": re.compile(
            r"Invalid pack\.mcmeta|invalid pack\.mcmeta|zipfile\.BadZipFile|"
            r"Bad CRC-32|checksum mismatch|DOWNLOAD_CHECKSUM_MISMATCH", re.IGNORECASE),
        "seed": "assetCorrupt",
        "recommend": "nav.resourceStudio",
        "action": {"kind": "route", "target": "resourceStudio"},
    },
    {
        "id": "version_json_bad",
        "pattern": re.compile(r"Failed to (parse|load) version|"
                              r"MojangAPI.*404|version manifest", re.IGNORECASE),
        "seed": "versionBad",
        "recommend": "nav.repair",
        "action": {"kind": "route", "target": "repair"},
    },
)

#: Blueprint đã chuẩn hoá (không key lạ).
_BLUEPRINTS = tuple(b for b in _BLUEPRINTS if b.get("id"))


class LogAnalyzer:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def sources(self, instance_id: str | None = None) -> list[dict]:
        """Danh sách nguồn log + trạng thái tồn tại + số byte."""
        return [self._source_info(src, instance_id) for src in _LOG_SOURCES]

    def analyze(self, instance_id: str | None = None,
                source: str | None = None) -> dict:
        """Quét các nguồn (source=None = cả 3) -> insights gộp theo blueprint.

        Mỗi insight: id, count, severity, seed (i18n), recommend, action,
        firstSeen/lastSeen, sources[], lines[0..5] (dòng gốc đã cắt 240 ký tự).
        """
        started = time.time()
        insights: dict[str, dict] = {}
        totals: dict[str, int] = {}

        for src in _LOG_SOURCES:
            if source and src != source:
                continue
            lines = self._read_source(src, instance_id)
            totals[src] = len(lines)
            for line_no, line in lines:
                bp = self._match(line)
                if not bp:
                    continue
                ins = insights.setdefault(bp["id"], {
                    "id": bp["id"], "count": 0, "severity": self._severity(bp["id"]),
                    "seed": bp["seed"], "recommend": bp["recommend"],
                    "action": bp["action"], "firstSeen": None, "lastSeen": None,
                    "sources": [], "lines": [],
                })
                ins["count"] += 1
                ins["sources"] = sorted(set(ins["sources"]) | {src})
                if ins["firstSeen"] is None:
                    ins["firstSeen"] = self._extract_ts(line) or ins["firstSeen"]
                ins["lastSeen"] = self._extract_ts(line) or ins["lastSeen"]
                if len(ins["lines"]) < 5:
                    ins["lines"].append({"source": src, "line": line_no,
                                         "text": line[:240]})

        items = sorted(insights.values(),
                       key=lambda i: (-i["count"], -self._severity_rank(i["id"])))
        return {
            "instanceId": instance_id,
            "sources": {k: v for k, v in totals.items()},
            "insights": items,
            "errorCount": sum(i["count"] for i in items if i["severity"] == "error"),
            "warnCount": sum(i["count"] for i in items if i["severity"] == "warning"),
            "durationMs": int((time.time() - started) * 1000),
        }

    def read(self, source: str, instance_id: str | None = None,
             limit: int = 5000) -> dict:
        """Đọc 1 nguồn -> {lines, truncated} cho console ảo hoá (mục 16)."""
        if source not in _LOG_SOURCES:
            from core.errors.base import AntaresError
            from core.errors import codes
            raise AntaresError(codes.VALIDATION_FAILED, f"Unknown log source: {source}")
        pairs = self._read_source(source, instance_id)
        truncated = len(pairs) > limit
        return {
            "source": source,
            "instanceId": instance_id,
            "lines": [{"n": n, "text": t[:400]} for n, t in pairs[-limit:]],
            "truncated": truncated,
        }

    # ------------------------------------------------------------------
    # Đọc từng nguồn
    # ------------------------------------------------------------------

    def _read_source(self, source: str, instance_id: str | None) -> list[tuple[int, str]]:
        try:
            if source == "launcher":
                ring = None
                try:
                    from core.logging.setup import get_ring
                    ring = get_ring()
                except Exception:
                    ring = None
                if ring is None:
                    return []
                snap = ring.snapshot(5000)
                return [(i + 1, s) for i, s in enumerate(snap)]
            if source == "minecraft":
                path = self._minecraft_log(instance_id)
                if path is None or not path.is_file():
                    return []
                return self._read_file_pairs(path, 5000)
            if source == "crash":
                out: list[tuple[int, str]] = []
                for path in self._crash_reports(instance_id):
                    pairs = self._read_file_pairs(path, 600)
                    out.extend(pairs)
                return out
        except Exception:
            logger.warning("read source %s failed", source, exc_info=True)
        return []

    def _minecraft_log(self, instance_id: str | None):
        iid = instance_id or self._ctx.config.get("selectedInstance")
        if not iid:
            return None
        return self._ctx.paths.instances / iid / "game" / "logs" / "latest.log"

    def _crash_reports(self, instance_id: str | None) -> list:
        iid = instance_id or self._ctx.config.get("selectedInstance")
        if not iid:
            return []
        d = self._ctx.paths.instances / iid / "game" / "crash-reports"
        if not d.is_dir():
            return []
        try:
            files = [p for p in d.iterdir() if p.is_file() and p.suffix == ".txt"]
            return sorted(files, key=lambda p: p.stat().st_mtime, reverse=True)[:3]
        except Exception:
            return []

    def _read_file_pairs(self, path, limit: int) -> list[tuple[int, str]]:
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except Exception:
            return []
        lines = text.splitlines()[-limit:]
        start = max(1, len(text.splitlines()) - len(lines) + 1)
        return [(start + i, s) for i, s in enumerate(lines)]

    # ------------------------------------------------------------------
    # Match + helpers
    # ------------------------------------------------------------------

    def _match(self, line: str) -> dict | None:
        for bp in _BLUEPRINTS:
            if bp["pattern"].search(line):
                return bp
        return None

    def _severity(self, bp_id: str) -> str:
        if bp_id in ("java_out_of_memory", "java_missing", "disk_full"):
            return "error"
        if bp_id in ("mod_conflict", "asset_corrupt", "version_json_bad",
                     "gpu_driver", "java_class_version"):
            return "error"
        return "warning"

    def _severity_rank(self, bp_id: str) -> int:
        return 1 if self._severity(bp_id) == "error" else 0

    def _extract_ts(self, line: str):
        """Timestamp đầu dòng '2026-09-26 12:00:00' (launcher fmt) nếu có."""
        m = re.match(r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})", line)
        return m.group(1) if m else None

    def _source_info(self, source: str, instance_id: str | None) -> dict:
        info = {"id": source, "available": False, "bytes": 0, "lines": 0}
        try:
            if source == "launcher":
                ring = None
                try:
                    from core.logging.setup import get_ring
                    ring = get_ring()
                except Exception:
                    ring = None
                if ring is not None:
                    snap = ring.snapshot(5000)
                    info["available"] = bool(snap)
                    info["lines"] = len(snap)
            elif source == "minecraft":
                path = self._minecraft_log(instance_id)
                if path and path.is_file():
                    info["available"] = True
                    info["bytes"] = path.stat().st_size
            elif source == "crash":
                reports = self._crash_reports(instance_id)
                info["available"] = bool(reports)
                info["bytes"] = sum(p.stat().st_size for p in reports)
                info["count"] = len(reports)
        except Exception:
            pass
        return info
