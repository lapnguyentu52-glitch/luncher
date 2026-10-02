"""ModSecurityService — orchestration cho scanner + quarantine + health."""
from __future__ import annotations

from pathlib import Path

from app.context import AppContext
from core.errors import codes
from core.errors.base import AntaresError
from core.tasks.manager import Task
from services.downloads.checksum import file_sha256
from services.mods.health import check_health, check_outdated, read_mod_info, ModInfo
from services.mods.scanner.quarantine import Quarantine
from services.mods.scanner.scanner import (
    scan_mod, VERDICT_DANGEROUS, VERDICT_SUSPICIOUS, ScanReport,
)


class ModSecurityService:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx
        self._quarantine = Quarantine(ctx.paths.cache / "quarantine")

    # ---------- scan ----------
    def scan_file(self, path: Path) -> dict:
        """Quét 1 jar bất kỳ (upload hoặc trong mods dir)."""
        if not path.exists():
            raise AntaresError(codes.INSTANCE_NOT_FOUND, f"File not found: {path.name}")
        sha = file_sha256(path)
        report: ScanReport = scan_mod(path, sha256=sha)
        result = report.to_dict()
        # auto-quarantine nếu DANGEROUS và file nằm trong mods dir của instance
        if report.verdict == VERDICT_DANGEROUS:
            result["quarantineSuggested"] = True
        return result

    def scan_uploaded(self, filename: str, content: bytes) -> dict:
        """Scan file upload — ghi vào cache tmp rồi scan như thường."""
        safe_name = Path(filename).name or "uploaded.jar"
        tmp = self._ctx.paths.cache / "uploads" / safe_name
        tmp.parent.mkdir(parents=True, exist_ok=True)
        tmp.write_bytes(content)
        result = self.scan_file(tmp)
        result["uploaded"] = True
        return result

    def scan_instance(self, instance_id: str, task: Task) -> dict:
        """Quét toàn bộ mods dir của instance."""
        mods_dir = self._ctx.paths.instances / instance_id / "game" / "mods"
        if not mods_dir.exists():
            return {"results": [], "dangerous": 0, "suspicious": 0, "safe": 0}

        jars = sorted(mods_dir.glob("*.jar"))
        results = []
        counts = {"dangerous": 0, "suspicious": 0, "safe": 0}
        for i, jar in enumerate(jars):
            if task.cancelled:
                raise AntaresError(codes.DOWNLOAD_CANCELLED, "Scan cancelled")
            task.message = f"Scanning {jar.name} ({i + 1}/{len(jars)})"
            task.progress = 100.0 * i / max(1, len(jars))
            sha = file_sha256(jar)
            report = scan_mod(jar, sha256=sha)
            data = report.to_dict()
            data["quarantined"] = False
            results.append(data)
            counts[report.verdict.lower()] = counts.get(report.verdict.lower(), 0) + 1

            # Tự động cô lập mod DANGEROUS (mục 368: move, không delete)
            if report.verdict == VERDICT_DANGEROUS:
                try:
                    self._quarantine.quarantine(
                        jar, instance_id=instance_id,
                        verdict=report.verdict, score=report.score,
                        findings=[{"ruleId": f.rule_id, "title": f.title,
                                   "severity": f.severity} for f in report.findings])
                    data["quarantined"] = True
                except AntaresError as e:
                    logger_msg = f"Quarantine failed: {e.message}"
                    data["quarantineError"] = e.message

        return {"results": results, **counts}

    # ---------- health ----------
    def health_check(self, instance_id: str, *, loader: str,
                     mc_version: str, include_outdated: bool = False) -> dict:
        mods_dir = self._ctx.paths.instances / instance_id / "game" / "mods"
        mods: list[ModInfo] = []
        if mods_dir.exists():
            for f in sorted(mods_dir.glob("*.jar")):
                mods.append(read_mod_info(f))

        issues = check_health(mods, loader=loader, mc_version=mc_version)
        outdated = []
        if include_outdated:
            http = self._ctx.get("http_client")
            outdated = check_outdated(mods, loader=loader, mc_version=mc_version,
                                      http=http)

        return {
            "mods": [
                {"filename": m.filename, "modId": m.mod_id, "name": m.name,
                 "version": m.version, "loader": m.loader,
                 "depends": m.depends, "readable": m.readable}
                for m in mods
            ],
            "issues": [
                {"mod": i.mod, "kind": i.kind, "detail": i.detail,
                 "fixable": i.fixable, "suggestion": i.suggestion}
                for i in issues
            ],
            "outdated": [
                {"mod": o.mod, "detail": o.detail, "suggestion": o.suggestion}
                for o in outdated
            ],
        }

    # ---------- auto-fix ----------
    def auto_fix(self, instance_id: str, *, loader: str, mc_version: str,
                 task: Task) -> dict:
        from services.mods.auto_fix import auto_fix_instance
        return auto_fix_instance(self._ctx, instance_id, loader=loader,
                                 mc_version=mc_version, task=task)

    # ---------- quarantine ----------
    def quarantine_list(self) -> list[dict]:
        return self._quarantine.list()

    def quarantine_restore(self, quarantine_file: str) -> dict:
        return self._quarantine.restore(quarantine_file)

    def quarantine_delete(self, quarantine_file: str) -> bool:
        return self._quarantine.delete(quarantine_file)
