"""Performance benchmarks — spec 3.0 mục 47.

"Không được chỉ test chạy được" — mỗi benchmark có **ngưỡng thời gian** rõ;
vượt ngưỡng = FAIL (exit 1). Ngưỡng đo trên máy dev, khoan dung cho CI
(2 vCPU): đủ bắt regression lớn, không flaky vì noise.

Chạy:  python tests/perf/benchmarks.py            (từ antares-src/)
       python tests/perf/benchmarks.py --quick    (bỏ dataset lớn)
Thoát: exit 0 = mọi ngưỡng pass, 1 = có benchmark vượt ngưỡng.
"""
from __future__ import annotations

import argparse
import io
import json
import shutil
import sys
import tempfile
import time
import threading
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]          # antares-src/
sys.path.insert(0, str(ROOT))

from app.context import AppPaths, AppContext        # noqa: E402
from core.config.manager import ConfigManager       # noqa: E402
from core.events import names as ev                 # noqa: E402
from core.events.bus import EventBus                # noqa: E402
from core.tasks.manager import TaskManager          # noqa: E402

RESULTS: list[dict] = []
FAILURES: list[str] = []


def bench(name: str, threshold_s: float, fn) -> None:
    """Chạy fn() 1 lần, đo wall time, so ngưỡng."""
    t0 = time.perf_counter()
    fn()
    dt = time.perf_counter() - t0
    ok = dt <= threshold_s
    RESULTS.append({"name": name, "seconds": round(dt, 4),
                    "threshold": threshold_s, "ok": ok})
    if not ok:
        FAILURES.append(f"{name}: {dt:.3f}s > {threshold_s}s")


def make_ctx(tmp: Path) -> AppContext:
    paths = AppPaths(root=tmp, data_dir=tmp / "data")
    paths.ensure_all()
    return AppContext(paths=paths, config=ConfigManager(paths.config / "settings.json"),
                      events=EventBus(), tasks=TaskManager())


# ------------------------------------------------------------------
# 1. Cold boot — bootstrap + wire mọi service headless (mục 47, 67)
# ------------------------------------------------------------------

def bench_cold_boot(tmp: Path) -> None:
    from api.bridge.api import AntaresApi

    def run() -> None:
        ctx = make_ctx(tmp / "boot")
        AntaresApi(ctx)                     # _wire_services: mọi service + loader
        runtime_bits = (ctx.get("instances"), ctx.get("mods"),
                        ctx.get("notifications"), ctx.get("plugins"))
        assert all(runtime_bits)

    bench("cold_boot_all_services", 2.0, run)


# ------------------------------------------------------------------
# 2. Realtime events — 500 evt/s pipeline qua EventBridge (mục 47/48)
# ------------------------------------------------------------------

def bench_events_500eps(tmp: Path) -> None:
    from api.events.bridge import EventBridge

    def run() -> None:
        ctx = make_ctx(tmp / "events")
        bridge = EventBridge(ctx.events, log_reader=lambda c: ([], c), log_cursor=0)
        n = 1500                            # 500/s x 3s tương đương khối lượng
        for i in range(n):
            ctx.events.publish(ev.PERFORMANCE_TELEMETRY, {"cpu": i % 100})
            if i % 25 == 0:
                ctx.events.publish(ev.TASK_UPDATED, {"id": f"t-{i}"},
                                   task_id=f"t-{i}")
        batch = bridge.flush()
        assert batch                        # coalesced batch không rỗng
        bridge.close()

    bench("events_1500_coalesced", 1.5, run)


# ------------------------------------------------------------------
# 3. Logs — 10k entries qua ring buffer + bridge đọc (mục 47)
# ------------------------------------------------------------------

def bench_logs_10k(tmp: Path) -> None:
    from core.logging.setup import (
        setup_logging, get_logger, get_ring_since, get_ring_cursor,
    )

    def run() -> None:
        setup_logging(tmp / "logs10k", dev_mode=False)
        # logger phải nằm trong hierarchy 'antares' — ring gắn trên logger đó
        log = get_logger("bench")
        for i in range(10_000):
            log.info("benchmark log line %d with some payload %d", i, i * 7)
        lines, cursor = get_ring_since(0)
        # ring capacity = 5000 — viết 10k thì giữ 5k dòng MỚI nhất (drop stale)
        assert len(lines) == 5000
        assert "9999" in lines[-1]
        get_ring_cursor()

    bench("logs_10k_ring", 2.0, run)


# ------------------------------------------------------------------
# 4. Notification burst — 20/s x 5s với dedupe + history cap (mục 47/80)
# ------------------------------------------------------------------

def bench_notification_burst(tmp: Path) -> None:
    from services.notifications.service import NotificationService

    def run() -> None:
        ctx = make_ctx(tmp / "notif")
        svc = NotificationService(ctx.events)
        for i in range(100):
            svc.push(severity="WARNING", category="bench",
                     title=f"Retry {i % 10}", message=f"attempt {i}")
        assert len(svc.list(limit=200)) <= 100
        svc.close()

    bench("notification_burst_100", 0.5, run)


# ------------------------------------------------------------------
# 5. Mod list — 1k / 10k file .jar (mục 47)
# ------------------------------------------------------------------

def bench_mod_list(tmp: Path, count: int, name: str, threshold: float) -> None:
    from services.mods.service import ModService

    mods_dir = tmp / "mods" / "inst" / "game" / "mods"
    mods_dir.mkdir(parents=True, exist_ok=True)
    for i in range(count):
        (mods_dir / f"mod-{i:05d}.jar").write_bytes(b"PK\x03\x04")  # stub jar

    # đo list_installed thuần FS — paths.instances là THƯ MỤC CHA chứa
    # subdir theo instance id (như AppPaths.instances)
    svc = ModService.__new__(ModService)
    svc._ctx = type("C", (), {"paths": type("P", (), {
        "instances": tmp / "mods",
    })()})()

    def run() -> None:
        names = svc.list_installed("inst")
        assert len(names) == count

    bench(name, threshold, run)


# ------------------------------------------------------------------
# 6. Task churn — create/progress/complete 1000 task (mục 47)
# ------------------------------------------------------------------

def bench_task_churn(tmp: Path) -> None:
    def run() -> None:
        tm = TaskManager()
        seen = []
        tm.set_update_listener(lambda task: seen.append(1))
        for i in range(1000):
            t = tm.create("bench", owner="perf")
            tm.start(t)
            tm.progress(t, (i % 100))
            tm.complete(t, {"i": i})
        assert len(seen) == 4000            # 4 event mỗi task

    bench("task_churn_1000", 1.0, run)


# ------------------------------------------------------------------
# 7. RS build pipeline — 10 lần create->generate->validate->build
#    (RS v2 Batch 9 — mục 27 performance budget "Build pack")
# ------------------------------------------------------------------

def bench_rs_build(tmp: Path) -> None:
    from services.resources import ResourceStudioService

    ctx = make_ctx(tmp / "rsbuild")
    ctx.set("instances", _FakeInstanceService())
    rs = ResourceStudioService(ctx)

    def run() -> None:
        for i in range(10):
            proj = rs.create(f"Bench Pack {i}", "1.21.11", template="pvp")
            pid = proj["id"]
            assert rs.validate(pid)["ok"] is True
            built = rs.build(pid)
            assert built["sha256"] and built["files"] >= 3

    bench("rs_build_x10", 3.0, run)


class _FakeInstanceService:
    """Stub tối thiểu cho installer path (không cần instance thật)."""

    def get(self, _iid):
        return {"id": "x", "name": "x"}


# ------------------------------------------------------------------
# 8. Asset Library — import 100 PNG + 10k metadata records search/sort
#    (RS v2 Batch 9 — mục 27 stress "10.000 asset metadata")
# ------------------------------------------------------------------

def bench_asset_library(tmp: Path) -> None:
    from services.resources.assets import AssetStore
    from services.resources.templates import encode_png

    ctx = make_ctx(tmp / "assets")
    store = AssetStore(ctx)
    png16 = encode_png(16, 16, bytes((232, 178, 58, 255)) * 256)

    def import_100() -> None:
        for i in range(100):
            store.import_png(png16 + bytes([i % 256]) + png16[41:],
                             name=f"asset-{i:03d}", category="item")
        assert len(store.list()) == 100

    bench("asset_import_100", 2.0, import_100)

    # 10k metadata records — catalog query path (không decode PNG)
    catalog = store._load_catalog()
    base = catalog["assets"][0]
    import time as _t
    for i in range(10_000 - 100):
        catalog["assets"].append({**base, "id": f"synth-{i:06d}",
                                  "name": f"synth-{i:06d}",
                                  "sha256": f"{'0' * 63}{i % 10}",
                                  "createdAt": _t.time() - i})
    store._save_catalog(catalog)

    def query_10k() -> None:
        # prefix search quét toàn bộ 10k records (mục 27: search debounce path)
        out = store.list(query="synth-009", category="item", sort="newest")
        assert out and len(store.list()) == 10_000

    bench("asset_query_10k", 1.0, query_10k)


# ------------------------------------------------------------------
# 9. Versioning hot path — 100k pack_meta/info calls (Batch 1 API chạy
#    trong mọi build/validate — mục 27 "startup cost")
# ------------------------------------------------------------------

def bench_versioning_hot_path(tmp: Path) -> None:
    from services.resources import versioning

    def run() -> None:
        for i in range(100_000):
            v = versioning.parse("1.21.11")
            _ = versioning.pack_format("1.21.11")
            _ = versioning.item_model_mode("1.21.4")
            if i % 20_000 == 0:
                _ = versioning.pack_meta("1.21.11")
        assert v == (1, 21, 11)

    bench("versioning_100k", 1.0, run)


# ------------------------------------------------------------------
# 10. Model3D — validator 64 cubes + static renderer 64 cubes
#     (RS v2 Batch 9 — mục 27 stress "64 cubes warning/perf test")
# ------------------------------------------------------------------

def bench_model3d(tmp: Path) -> None:
    from services.visuals import model3d, renderer

    spec = {"grid": 16, "cubes": [
        {"id": f"c{i}", "from": [i % 8 * 2, i // 8 * 2, 0],
         "to": [i % 8 * 2 + 1, i // 8 * 2 + 1, 1],
         "faces": {"south": {"texture": "#e8b23a"}}}
        for i in range(65)]}         # 65 > WARN_CUBES 64 -> warning
    findings0 = model3d.validate(spec)
    assert not model3d.has_errors(findings0)
    assert any(f["code"] == "MODEL_CUBE_LIMIT" for f in findings0)

    def run() -> None:
        findings = model3d.validate(spec)
        assert any(f["code"] == "MODEL_CUBE_LIMIT" for f in findings)
        size, rgba = renderer.render_rgba(spec, size=128)
        assert len(rgba) == size * size * 4

    bench("model3d_64cubes_render", 1.0, run)


# ------------------------------------------------------------------
# 11. ZIP importer — inspect + import 100-entry pack (Batch 6 hot path)
# ------------------------------------------------------------------

def bench_zip_import(tmp: Path) -> None:
    import io
    import zipfile
    from services.resources.importer import inspect_zip
    from services.resources import ResourceStudioService

    ctx = make_ctx(tmp / "zipimp")
    ctx.set("instances", _FakeInstanceService())
    rs = ResourceStudioService(ctx)
    ctx.set("resource_studio", rs)      # importer lấy rs qua ctx.get
    proj = rs.create("ZipBench", "1.21.11", template="blank")
    pid = proj["id"]

    png = (b"\x89PNG\r\n\x1a\n" + struct_pack(13) + b"IHDR"
           + struct_pack_data(4, 4) + b"\x00" * 4)
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_STORED) as zf:
        for i in range(100):
            zf.writestr(f"assets/minecraft/textures/item/f{i:03d}.png", png)
    zip_path = tmp / "zipimp" / "bench.zip"
    zip_path.write_bytes(buf.getvalue())

    def run() -> None:
        with zipfile.ZipFile(zip_path) as zf:
            report = inspect_zip(zf)
            assert report["ok"] is True and len(report["entries"]) == 100
        out = rs.zip_import(pid, zip_path)
        assert len(out["imported"]) == 100

    bench("zip_import_100_entries", 2.0, run)


def struct_pack(size: int) -> bytes:
    import struct
    return struct.pack(">I", size)


def struct_pack_data(w: int, h: int) -> bytes:
    import struct
    return struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)


# ------------------------------------------------------------------
# 12. Layering — effective resolution với 20 packs x 200 paths
#     (RS v2 Batch 9 — mục 25 gate "deterministic output" + perf)
# ------------------------------------------------------------------

def bench_layering(tmp: Path) -> None:
    from services.resources.layering import LayerStore

    ctx = make_ctx(tmp / "layers")
    ctx.set("instances", _FakeInstanceService())
    # _FakeInstanceService.get -> {'id': 'x'}; LayerStore chỉ validate tồn tại.
    inst = "x"
    rp = ctx.paths.instances / inst / "game" / "resourcepacks"
    rp.mkdir(parents=True, exist_ok=True)

    png = (b"\x89PNG\r\n\x1a\n" + struct_pack(13) + b"IHDR"
           + struct_pack_data(4, 4) + b"\x00" * 4)
    names = []
    for p in range(20):
        name = f"pack-{p:02d}.zip"
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_STORED) as zf:
            for i in range(200):
                zf.writestr(f"assets/minecraft/textures/item/s{i:03d}.png", png)
        (rp / name).write_bytes(buf.getvalue())
        names.append(name)

    layers = LayerStore(ctx)
    layers.set_order(inst, names)

    def run() -> None:
        # 1 query full preview (4000 path tổng) + 50 effective queries
        preview = layers.preview_effective(inst)
        assert len(preview["assets"]) == 200
        for i in range(50):
            eff = layers.effective_asset(
                inst, f"assets/minecraft/textures/item/s{i % 200:03d}.png")
            assert eff is not None

    bench("layering_20packs_preview", 2.0, run)


# ------------------------------------------------------------------
# Main
# ------------------------------------------------------------------

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--quick", action="store_true",
                        help="bỏ dataset lớn (10k logs/mods)")
    parser.add_argument("--json", action="store_true", help="in kết quả JSON")
    args = parser.parse_args()

    tmp = Path(tempfile.mkdtemp(prefix="antares-bench-"))
    try:
        bench_cold_boot(tmp)
        bench_events_500eps(tmp)
        if not args.quick:
            bench_logs_10k(tmp)
        bench_notification_burst(tmp)
        bench_mod_list(tmp, 1000, "mod_list_1k", 1.0)
        if not args.quick:
            bench_mod_list(tmp, 10_000, "mod_list_10k", 3.0)
        bench_task_churn(tmp)
        if not args.quick:
            bench_rs_build(tmp)
            bench_asset_library(tmp)
            bench_versioning_hot_path(tmp)
            bench_model3d(tmp)
            bench_zip_import(tmp)
            bench_layering(tmp)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    if args.json:
        print(json.dumps({"results": RESULTS, "failures": FAILURES}, indent=2))
    else:
        width = max(len(r["name"]) for r in RESULTS)
        print(f"{'BENCHMARK'.ljust(width)}  {'TIME':>8}  {'THRESHOLD':>9}  STATUS")
        for r in RESULTS:
            print(f"{r['name'].ljust(width)}  {r['seconds']:>7.3f}s  {r['threshold']:>8.1f}s  "
                  f"{'PASS' if r['ok'] else 'FAIL'}")
        print(f"\n{len(RESULTS) - len(FAILURES)}/{len(RESULTS)} benchmarks passed")

    if FAILURES:
        print("REGRESSIONS:", *FAILURES, sep="\n  - ")
        return 1
    print("BENCH_OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
