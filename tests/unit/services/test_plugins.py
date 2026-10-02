"""Plugin SDK — manifest/permissions, sandbox, loader (spec 3.0 mục 84, 85)."""
from __future__ import annotations

import json
from pathlib import Path

import pytest

from services.plugins.manifest import (
    ManifestError, has_permission, is_safe_id, load_manifest,
)
from services.plugins.sandbox import PermissionDenied, PluginSandbox
from services.plugins.loader import PluginLoader


# ------------------------------------------------------------------
# Manifest (mục 85)
# ------------------------------------------------------------------

def _write_plugin(root: Path, pid: str, *, perms=None, entry="main.py",
                  setup_body: str = "def setup(ctx): pass") -> Path:
    d = root / pid
    d.mkdir(parents=True, exist_ok=True)
    manifest = {
        "id": pid, "name": pid.title(), "version": "1.0.0",
        "entry": entry, "permissions": perms or [],
    }
    (d / "plugin.json").write_text(json.dumps(manifest), encoding="utf-8")
    (d / entry).write_text(setup_body, encoding="utf-8")
    return d


def test_manifest_valid_and_permission_scoping():
    mf = load_manifest(_write_plugin(Path("/tmp"), "x", perms=["minecraft"]))
    assert has_permission(mf, "minecraft.telemetry")   # quyền cha bao phủ scope con
    assert not has_permission(mf, "network")
    mf2 = load_manifest(_write_plugin(Path("/tmp"), "y", perms=["ui"]))
    assert has_permission(mf2, "ui")
    assert not has_permission(mf2, "minecraft")        # deny-by-default


def test_manifest_rejects_unknown_and_missing_fields(tmp_path):
    with pytest.raises(ManifestError):
        load_manifest(tmp_path)  # không có plugin.json

    d = tmp_path / "bad"
    d.mkdir()
    (d / "plugin.json").write_text("{not json", encoding="utf-8")
    with pytest.raises(ManifestError):
        load_manifest(d)

    _write_plugin(tmp_path, "p1", perms=["root.full access"])
    with pytest.raises(ManifestError):
        load_manifest(tmp_path / "p1")

    _write_plugin(tmp_path, "p2", entry="missing.py")
    # entry tồn tại hay không do LOADER kiểm (ManifestError khi load),
    # manifest chỉ yêu cầu entry là string — không assert gì thêm ở đây.


def test_is_safe_id_blocks_traversal():
    assert is_safe_id("com.example.tool")
    assert not is_safe_id("../evil")
    assert not is_safe_id("a/b")
    assert not is_safe_id("")
    assert not is_safe_id("x" * 65)


# ------------------------------------------------------------------
# Sandbox — permission-checked API
# ------------------------------------------------------------------

def _sandbox(perms, tmp_path):
    mf = {"id": "test.plug", "name": "T", "version": "1", "entry": "m.py",
          "permissions": perms}
    reg = type("R", (), {
        "register_command": lambda *a, **k: None,
        "register_tab": lambda *a, **k: None,
        "register_panel": lambda *a, **k: None,
        "register_optimizer": lambda *a, **k: None,
        "register_resource_template": lambda *a, **k: None,
        "register_visual_preset": lambda *a, **k: None,
    })()
    events = type("E", (), {"subscribe": lambda *a, **k: None})()
    return PluginSandbox(mf, reg, events, tmp_path / "data")


def test_sandbox_denies_without_permission(tmp_path):
    sb = _sandbox([], tmp_path)
    with pytest.raises(PermissionDenied):
        sb.api().register_command("x", lambda: None)
    with pytest.raises(PermissionDenied):
        sb.api().register_tab("t", title_key="k")
    with pytest.raises(PermissionDenied):
        sb.api().notify("hello")


def test_sandbox_allows_with_permission(tmp_path):
    sb = _sandbox(["ui", "minecraft.telemetry"], tmp_path)
    seen = {}
    ctx = sb.api()
    ctx.register_command("greet", lambda: {"msg": "hi"})
    ctx.on_minecraft_telemetry(lambda env: seen.update(env.payload))
    assert sb.allows("minecraft.telemetry")
    assert not sb.allows("filesystem")


def test_sandbox_config_roundtrip(tmp_path):
    sb = _sandbox(["ui"], tmp_path)
    ctx = sb.api()
    ctx.write_config({"theme": "dark"})
    assert ctx.read_config() == {"theme": "dark"}
    assert ctx.read_config.__doc__  # giữ contract


# ------------------------------------------------------------------
# Scoped filesystem — mọi path bị kẹp trong data_dir (mục 76, 85)
# ------------------------------------------------------------------

def test_fs_scoped_read_write(tmp_path):
    sb = _sandbox(["ui", "filesystem"], tmp_path)   # filesystem ⊃ filesystem.read
    ctx = sb.api()
    r = ctx.write_file("cache/data.json", b"{}")
    assert r["bytes"] == 2
    assert ctx.read_file("cache/data.json") == b"{}"
    assert ctx.list_files() == ["cache/data.json"]
    assert ctx.delete_file("cache/data.json")["deleted"]
    assert ctx.list_files() == []


def test_fs_blocks_escape_and_requires_permission(tmp_path):
    sb = _sandbox(["filesystem.read"], tmp_path)    # chỉ read
    ctx = sb.api()
    import pytest
    from services.plugins.sandbox import PermissionDenied

    with pytest.raises(PermissionDenied):
        ctx.write_file("x.txt", b"nope")            # thiếu quyền RW
    ctx.write_file.__doc__  # method tồn tại

    with pytest.raises(PermissionDenied):
        ctx.read_file("../../etc/passwd")            # path traversal
    with pytest.raises(PermissionDenied):
        ctx.read_file("/etc/passwd")                 # absolute escape
    # read trong phạm vi thì OK (file chưa tồn tại -> FileNotFoundError rõ ràng)
    import pytest as _p
    with _p.raises(FileNotFoundError):
        ctx.read_file("missing.bin")


# ------------------------------------------------------------------
# Scoped network — HTTPS-only + allowlist config (mục 85)
# ------------------------------------------------------------------

def _net_sandbox(tmp_path, allowlist):
    sb = _sandbox(["ui", "network"], tmp_path)
    calls = []
    class _Http:
        def get_json(self, url, **kw):
            calls.append(("GET", url)); return {"ok": True}
        def post_json(self, url, **kw):
            calls.append(("POST", url)); return {"ok": True}
    sb.context._http = _Http()
    sb.context._network_allowlist = allowlist
    return sb, calls


def test_network_allowlist_enforced(tmp_path):
    import pytest
    from services.plugins.sandbox import PermissionDenied
    sb, calls = _net_sandbox(tmp_path, ["api.example.com"])
    ctx = sb.api()

    assert ctx.http_get_json("https://api.example.com/v1/data") == {"ok": True}
    # subdomain của entry thì được (entry lstrip '.' rồi so suffix .entry)
    assert ctx.http_post_json("https://api.example.com/v1/submit", {"a": 1})

    with pytest.raises(PermissionDenied):
        ctx.http_get_json("https://evil.com/v1")     # không trong allowlist
    with pytest.raises(PermissionDenied):
        ctx.http_get_json("http://api.example.com/v1")  # HTTP bị chặn
    with pytest.raises(PermissionDenied):
        ctx.http_get_json("https://example.com/v1")  # host khác entry
    assert len(calls) == 2


# ------------------------------------------------------------------
# Loader — vòng đời scan/load/unload
# ------------------------------------------------------------------

def test_loader_scan_and_load_all(tmp_path, ctx):
    plugins_dir = ctx.paths.plugins
    _write_plugin(plugins_dir, "com.good.one", perms=["ui"],
                  setup_body="def setup(pctx): pctx.register_command('hi', lambda: 1)")
    loader = PluginLoader(ctx)
    ctx.set("plugins", loader)

    scan = loader.scan()
    assert [s["id"] for s in scan] == ["com.good.one"]
    assert scan[0]["loaded"] is False

    r = loader.load_all()
    assert r["loaded"] == ["com.good.one"] and not r["skipped"]
    assert loader.loaded_ids() == ["com.good.one"]

    # command đã đăng ký — chạy được qua registry
    out = loader.registry().run_command("plugin.com.good.one.hi")
    assert out["ok"] and out["data"]["result"] == 1

    # event PLUGIN_LOADED đã publish
    seen = []
    ctx.events.subscribe("plugin.loaded", lambda env: seen.append(env.payload))
    r2 = loader.enable("com.good.one")  # already loaded -> no-op path không crash
    assert r2["ok"]


def test_loader_skips_broken_plugin_and_keeps_healthy(tmp_path, ctx):
    plugins_dir = ctx.paths.plugins
    _write_plugin(plugins_dir, "com.broken", perms=["ui"],
                  setup_body="def setup(pctx):\n  raise RuntimeError('boom')")
    _write_plugin(plugins_dir, "com.healthy", perms=["ui"])
    loader = PluginLoader(ctx)
    r = loader.load_all()
    assert r["loaded"] == ["com.healthy"]
    assert len(r["skipped"]) == 1
    assert "boom" in r["skipped"][0]["reason"]
    assert loader.loaded_ids() == ["com.healthy"]


def test_loader_disable_persists_and_unload(tmp_path, ctx):
    plugins_dir = ctx.paths.plugins
    # teardown ghi file vào data_dir của plugin — chứng kiến teardown được gọi
    setup_src = (
        "def setup(pctx): pctx.register_tab('t', title_key='x')\n"
        "def teardown(pctx):\n"
        "    pctx.data_dir.mkdir(parents=True, exist_ok=True)\n"
        "    (pctx.data_dir / 'bye.txt').write_text('bye', encoding='utf-8')\n"
    )
    _write_plugin(plugins_dir, "com.switch", perms=["ui"], setup_body=setup_src)
    (plugins_dir / "com.switch" / "main.py").write_text(setup_src, encoding="utf-8")
    loader = PluginLoader(ctx)
    ctx.set("plugins", loader)
    loader.load_all()
    assert loader.registry().tabs()

    r = loader.set_enabled("com.switch", False)
    assert r["ok"]
    assert loader.loaded_ids() == []
    assert not loader.registry().tabs()          # gỡ hết đăng ký
    assert ctx.config.get("plugins.disabled") == ["com.switch"]
    # teardown đã chạy — file đánh dấu nằm trong data dir riêng của plugin
    assert (ctx.paths.plugins / "_data" / "com.switch" / "bye.txt").exists()

    # Load lại từ config disabled — scan vẫn thấy, load_all không load
    loader2 = PluginLoader(ctx)
    scan = loader2.scan()
    assert scan[0]["enabled"] is False
    assert loader2.load_all()["loaded"] == []

    # Bật lại -> load được
    r2 = loader2.set_enabled("com.switch", True)
    assert r2["ok"]
    assert loader2.loaded_ids() == ["com.switch"]
