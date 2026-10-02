"""Tests cho B9 sidecar visual.* handlers (§129–§135 — Visual Studio).

Handlers bọc VisualStudioService thật — mock `_visual_studio`, không đụng
filesystem thật. Render handlers trả data URI (mock phía service).
"""

import sys
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent
sys.path.insert(0, str(REPO_ROOT / "legacy" / "python"))

import sidecar  # noqa: E402

_DATA_URI = "data:image/png;base64,AAAA"


def _ctx_with_instance(instance: dict | None) -> mock.MagicMock:
    ctx = mock.MagicMock()
    inst_svc = mock.MagicMock()
    inst_svc.get.return_value = instance
    services = {"instances": inst_svc}
    ctx.get.side_effect = lambda name: services[name]
    return ctx


def _call(method: str, params: dict, *, ctx=None, studio=None):
    ctx = ctx if ctx is not None else mock.MagicMock()
    with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx), \
         mock.patch.object(sidecar, "_visual_studio",
                           return_value=studio or mock.MagicMock()):
        response = sidecar.handle_request({"id": "t", "method": method, "params": params})
    return response


class TestRegistry:
    def test_all_visual_methods_registered(self):
        expected = [
            "visual.presets", "visual.render_preview", "visual.export_pack",
            "visual.totem_presets", "visual.render_totem",
            "visual.totem_model.get", "visual.totem_model.save",
            "visual.render_totem_model", "visual.export_totem_pack",
            "visual.hud_widgets", "visual.save_hud_layout",
            "visual.fx_defaults", "visual.render_hit", "visual.render_particle",
            "visual.export_fx_pack",
        ]
        for method in expected:
            assert method in sidecar.HANDLERS, f"missing handler: {method}"


class TestCrosshair:
    def test_presets(self):
        studio = mock.MagicMock()
        studio.presets.return_value = {
            "presets": [{"id": "minimal", "spec": {"shape": "cross"}}],
            "default": {"shape": "cross", "size": 16},
        }
        response = _call("visual.presets", {}, studio=studio)
        assert response["ok"] is True
        assert response["data"]["presets"][0]["id"] == "minimal"
        assert response["data"]["default"]["shape"] == "cross"

    def test_render_preview(self):
        studio = mock.MagicMock()
        studio.render_preview_b64.return_value = _DATA_URI
        response = _call("visual.render_preview",
                         {"spec": {"shape": "dot", "color": "#ff4655"}}, studio=studio)
        assert response["ok"] is True
        assert response["data"]["preview"].startswith("data:image/png")
        studio.render_preview_b64.assert_called_once_with(
            {"shape": "dot", "color": "#ff4655"})

    def test_render_preview_requires_spec(self):
        response = _call("visual.render_preview", {"spec": "nope"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_export_ok(self):
        ctx = _ctx_with_instance({"id": "i1"})
        studio = mock.MagicMock()
        studio.export_pack.return_value = {"projectId": "p1", "build": {"file": "x.zip"}}
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("visual.export_pack",
                             {"name": "My Cross", "mcVersion": "1.21.4",
                              "spec": {"shape": "cross"}, "installInstanceId": "i1"},
                             ctx=ctx, studio=studio)
        assert response["ok"] is True
        assert response["data"]["export"]["projectId"] == "p1"
        studio.export_pack.assert_called_once_with(
            "My Cross", {"shape": "cross"}, "1.21.4",
            install_instance_id="i1", overwrite=False)
        n.assert_called_once()

    def test_export_requires_name_and_version(self):
        response = _call("visual.export_pack", {"name": "", "mcVersion": ""})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_export_instance_not_found(self):
        ctx = _ctx_with_instance(None)
        response = _call("visual.export_pack",
                         {"name": "X", "mcVersion": "1.21", "spec": {},
                          "installInstanceId": "ghost"}, ctx=ctx)
        assert response["ok"] is False
        assert response["error"]["code"] == "INSTANCE_NOT_FOUND"

    def test_export_service_error_kept(self):
        from core.errors.base import AntaresError

        studio = mock.MagicMock()
        studio.export_pack.side_effect = AntaresError("VALIDATION_FAILED", "bad spec")
        response = _call("visual.export_pack",
                         {"name": "X", "mcVersion": "1.21", "spec": {}}, studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"


class TestTotem:
    def test_presets(self):
        studio = mock.MagicMock()
        studio.totem_presets.return_value = {
            "presets": [{"id": "classic", "spec": {"base": "#e8b23a"}}],
            "default": {"base": "#e8b23a"},
        }
        response = _call("visual.totem_presets", {}, studio=studio)
        assert response["ok"] is True
        assert response["data"]["presets"][0]["id"] == "classic"

    def test_render(self):
        studio = mock.MagicMock()
        studio.render_totem_b64.return_value = _DATA_URI
        response = _call("visual.render_totem", {"spec": {"base": "#e8b23a"}},
                         studio=studio)
        assert response["ok"] is True
        assert response["data"]["preview"] == _DATA_URI

    def test_model_get(self):
        studio = mock.MagicMock()
        studio.totem_model_get.return_value = {"spec": None}
        response = _call("visual.totem_model.get", {}, studio=studio)
        assert response["ok"] is True
        assert response["data"]["spec"] is None

    def test_model_save(self):
        studio = mock.MagicMock()
        studio.totem_model_save.return_value = {"saved": True, "findings": []}
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("visual.totem_model.save", {"spec": {"version": 1}},
                             studio=studio)
        assert response["ok"] is True
        assert response["data"]["saved"] is True
        studio.totem_model_save.assert_called_once_with({"version": 1})
        n.assert_called_once()

    def test_model_save_requires_spec(self):
        response = _call("visual.totem_model.save", {})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_render_model(self):
        studio = mock.MagicMock()
        studio.render_totem_model_b64.return_value = _DATA_URI
        response = _call("visual.render_totem_model", {"spec": {"version": 1}}, studio=studio)
        assert response["ok"] is True
        studio.render_totem_model_b64.assert_called_once_with(
            {"version": 1}, size=256)

    def test_render_model_size_bounds(self):
        response = _call("visual.render_totem_model",
                         {"spec": {}, "size": 9999})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_export_totem_ok(self):
        ctx = _ctx_with_instance({"id": "i1"})
        studio = mock.MagicMock()
        studio.export_totem_pack.return_value = {"projectId": "p2", "build": {"file": "t.zip"}}
        with mock.patch.object(sidecar, "notify"):
            response = _call("visual.export_totem_pack",
                             {"name": "My Totem", "mcVersion": "1.21.4",
                              "spec": {"base": "#e8b23a"}, "installInstanceId": "i1",
                              "overwrite": True},
                             ctx=ctx, studio=studio)
        assert response["ok"] is True
        assert response["data"]["export"]["projectId"] == "p2"
        studio.export_totem_pack.assert_called_once_with(
            "My Totem", {"base": "#e8b23a"}, "1.21.4",
            install_instance_id="i1", overwrite=True)

    def test_export_totem_requires_name(self):
        response = _call("visual.export_totem_pack",
                         {"name": " ", "mcVersion": "1.21", "spec": {}})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"


class TestHud:
    def test_widgets(self):
        studio = mock.MagicMock()
        studio.hud_widgets.return_value = {
            "widgets": ["fps", "cps", "coordinates"],
            "defaultLayout": [{"id": "fps", "x": 4, "y": 4}],
        }
        response = _call("visual.hud_widgets", {}, studio=studio)
        assert response["ok"] is True
        assert response["data"]["widgets"] == ["fps", "cps", "coordinates"]

    def test_save_layout_ok(self):
        studio = mock.MagicMock()
        studio.save_hud_layout.return_value = {"layout": [{"id": "fps", "x": 4, "y": 4,
                                                           "scale": 1.0, "visible": True}]}
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("visual.save_hud_layout",
                             {"projectId": "p1",
                              "layout": [{"id": "fps", "x": 4, "y": 4,
                                          "scale": 1.0, "visible": True}]},
                             studio=studio)
        assert response["ok"] is True
        assert response["data"]["layout"][0]["id"] == "fps"
        n.assert_called_once()

    def test_save_layout_requires_list(self):
        response = _call("visual.save_hud_layout", {"projectId": "p1", "layout": "fps"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_save_layout_service_validation_kept(self):
        from core.errors.base import AntaresError

        studio = mock.MagicMock()
        studio.save_hud_layout.side_effect = AntaresError(
            "VALIDATION_FAILED", "Invalid HUD layout: unknown widget")
        response = _call("visual.save_hud_layout",
                         {"projectId": "p1", "layout": [{"id": "nope"}]}, studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"


class TestFx:
    def test_defaults(self):
        studio = mock.MagicMock()
        studio.fx_defaults.return_value = {
            "hit": {"default": {"kind": "flash"}, "kinds": ["none", "flash"]},
            "particle": {"default": {"shape": "orb"}, "shapes": ["orb", "spark"],
                         "maxFrames": 8},
        }
        response = _call("visual.fx_defaults", {}, studio=studio)
        assert response["ok"] is True
        assert response["data"]["hit"]["kinds"] == ["none", "flash"]
        assert response["data"]["particle"]["maxFrames"] == 8

    def test_render_hit(self):
        studio = mock.MagicMock()
        studio.render_hit_b64.return_value = _DATA_URI
        response = _call("visual.render_hit", {"spec": {"kind": "flash"}}, studio=studio)
        assert response["ok"] is True
        studio.render_hit_b64.assert_called_once_with({"kind": "flash"})

    def test_render_particle(self):
        studio = mock.MagicMock()
        studio.render_particle_b64.return_value = _DATA_URI
        response = _call("visual.render_particle", {"spec": {"shape": "orb"}},
                         studio=studio)
        assert response["ok"] is True

    def test_render_hit_requires_spec(self):
        response = _call("visual.render_hit", {})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_export_fx_ok(self):
        ctx = _ctx_with_instance({"id": "i1"})
        studio = mock.MagicMock()
        studio.export_fx_pack.return_value = {"projectId": "p3", "build": {"file": "f.zip"}}
        with mock.patch.object(sidecar, "notify"):
            response = _call("visual.export_fx_pack",
                             {"name": "My FX", "mcVersion": "1.21.4",
                              "hitSpec": {"kind": "flash"},
                              "particleSpec": {"shape": "orb"},
                              "installInstanceId": "i1"},
                             ctx=ctx, studio=studio)
        assert response["ok"] is True
        assert response["data"]["export"]["projectId"] == "p3"
        studio.export_fx_pack.assert_called_once_with(
            "My FX", {"kind": "flash"}, {"shape": "orb"}, "1.21.4",
            install_instance_id="i1", overwrite=False)

    def test_export_fx_requires_one_spec(self):
        response = _call("visual.export_fx_pack",
                         {"name": "X", "mcVersion": "1.21"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_export_fx_service_error_kept(self):
        from core.errors.base import AntaresError

        studio = mock.MagicMock()
        studio.export_fx_pack.side_effect = AntaresError("VALIDATION_FAILED", "bad fx")
        response = _call("visual.export_fx_pack",
                         {"name": "X", "mcVersion": "1.21", "hitSpec": {"kind": "flash"}},
                         studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"
