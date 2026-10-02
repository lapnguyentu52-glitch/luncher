"""Totem 3D draft model service (Batch 4 — master plan v3 mục 62, 70).

Draft nằm backend store (mục 38: KHÔNG localStorage), atomic write,
validate chặn ERROR; corrupt draft -> trả {spec: None, corrupt: True}
để UI recovery (mục 62: crash recovery).
"""
from __future__ import annotations

import base64
import json

from core.errors.base import AntaresError
from services.visuals import VisualStudioService
from services.visuals import model3d, renderer


def _svc(ctx) -> VisualStudioService:
    return VisualStudioService(ctx)


def test_draft_roundtrip(ctx):
    svc = _svc(ctx)
    assert svc.totem_model_get()["spec"] is None     # chưa có draft

    spec = {"grid": 16, "cubes": [
        {"id": "body", "from": [4, 0, 4], "to": [12, 10, 12]},
    ], "textures": {"base": "#e8b23a"}}
    out = svc.totem_model_save(spec)
    assert out["saved"] is True and out["findings"] == []

    got = svc.totem_model_get()
    assert got["spec"] == spec
    assert got["findings"] == []


def test_draft_save_blocks_errors(ctx):
    svc = _svc(ctx)
    bad = {"grid": 16, "cubes": [
        {"id": "flat", "from": [4, 4, 4], "to": [4, 8, 8]},   # zero-size -> ERROR
    ]}
    try:
        svc.totem_model_save(bad)
        assert False, "phải chặn ERROR"
    except AntaresError as e:
        assert e.code == "VALIDATION_FAILED"


def test_draft_corrupt_recovered(ctx):
    svc = _svc(ctx)
    d = ctx.paths.data / "totem-3d"
    d.mkdir(parents=True, exist_ok=True)
    (d / "draft.json").write_text("{not json", encoding="utf-8")
    got = svc.totem_model_get()
    assert got["spec"] is None and got.get("corrupt") is True
    # Ghi đè draft hợp lệ sau corrupt vẫn hoạt động (atomic write)
    spec = {"grid": 16, "cubes": [
        {"id": "a", "from": [0, 0, 0], "to": [2, 2, 2]}]}
    assert svc.totem_model_save(spec)["saved"] is True
    assert svc.totem_model_get()["spec"] == spec


def test_static_render_b64(ctx):
    svc = _svc(ctx)
    spec = {"grid": 16, "cubes": [
        {"id": "a", "from": [0, 0, 0], "to": [8, 8, 8],
         "faces": {"south": {"texture": "#ff0000"}}}]}
    uri = svc.render_totem_model_b64(spec, size=64)
    assert uri.startswith("data:image/png;base64,")
    raw = base64.b64decode(uri.split(",", 1)[1])
    assert raw.startswith(b"\x89PNG\r\n\x1a\n")
    # khớp render trực tiếp (cùng pipeline — mục 9)
    assert raw == renderer.render_png(spec, size=64)


def test_voxel_spec_validator_integration(ctx):
    # totem.voxel_spec() phải qua validator sạch (Quick -> Advanced migration)
    from services.visuals import totem
    spec = totem.voxel_spec()
    assert model3d.validate(spec) == []
