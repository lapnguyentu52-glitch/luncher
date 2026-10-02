"""VisualStudioService — Crosshair Studio facade (mục 8.3 output modes).

Output:
1. `preview`   — PNG bytes cho <img> preview (UI vẽ canvas song song).
2. `export`    — tạo/cập nhật Resource Studio project "visual-<tên>" với
   texture crosshair PNG + pack.mcmeta version-aware -> build ZIP -> install
   vào instance tuỳ chọn (re-dùng toàn bộ pipeline batch 5).
"""
from __future__ import annotations

import json
import time

from app.context import AppContext
from core.config.writer import write_json_atomic
from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger
from services.visuals import crosshair

logger = get_logger("visuals")


class VisualStudioService:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx
        self._resources = None

    @property
    def _rs(self):
        """Lazy — tránh circular import lúc bootstrap."""
        if self._resources is None:
            self._resources = self._ctx.get("resource_studio")
        return self._resources

    # ------------------------------------------------------------------
    # Presets / preview
    # ------------------------------------------------------------------

    def presets(self) -> dict:
        return {"presets": [
            {"id": pid, "spec": {**crosshair.DEFAULT_SPEC, **crosshair.PRESETS[pid]}}
            for pid in crosshair.PRESET_ORDER
        ], "default": crosshair.DEFAULT_SPEC}

    # ------------------------------------------------------------------
    # Totem (mục 9)
    # ------------------------------------------------------------------

    def totem_presets(self) -> dict:
        from services.visuals import totem
        return {"presets": [
            {"id": pid, "spec": {**totem.DEFAULT_SPEC, **totem.PRESETS[pid]}}
            for pid in totem.PRESET_ORDER
        ], "default": totem.DEFAULT_SPEC}

    def render_totem_b64(self, spec: dict) -> str:
        import base64
        png = totem.render_png(spec)
        return "data:image/png;base64," + base64.b64encode(png).decode("ascii")

    # ------------------------------------------------------------------
    # Totem 3D — draft model (Batch 4, mục 62: draft store backend-backed)
    # ------------------------------------------------------------------

    def totem_model_get(self) -> dict:
        """Draft voxel model hiện tại (spec v3 mục 5.3) hoặc None."""
        import json
        from services.visuals import model3d
        path = self._ctx.paths.data / "totem-3d" / "draft.json"
        if not path.is_file():
            return {"spec": None}
        try:
            spec = json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            logger.warning("Corrupt totem-3d draft — trả None để UI recovery")
            return {"spec": None, "corrupt": True}
        return {"spec": spec, "findings": model3d.validate(spec)}

    def totem_model_save(self, spec: dict) -> dict:
        """Lưu draft — validate chặn ERROR (mục 14), atomic write (mục 62)."""
        import json
        from services.visuals import model3d
        findings = model3d.validate(spec)
        if model3d.has_errors(findings):
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"Invalid model: {model3d.errors(findings)[0]['code']}")
        d = self._ctx.paths.data / "totem-3d"
        d.mkdir(parents=True, exist_ok=True)
        write_json_atomic(d / "draft.json", spec)
        return {"saved": True, "findings": findings}

    def render_totem_model_b64(self, spec: dict, size: int = 256) -> str:
        """Static render (mục 9) — fallback khi WebGL không khả dụng."""
        import base64
        from services.visuals import renderer
        png = renderer.render_png(spec, size=size)
        return "data:image/png;base64," + base64.b64encode(png).decode("ascii")

    def export_totem_pack(self, name: str, spec: dict, mc_version: str,
                          *, install_instance_id: str | None = None,
                          overwrite: bool = False) -> dict:
        """Totem -> RS project -> build -> install (khuôn mẫu crosshair).

        Batch 2: texture totem + model JSON theo version (mục 7):
        - definition (1.21.4+): items/totem_of_undying.json + models/item/
        - legacy: models/item/totem_of_undying.json (item/generated)
        Quick Totem output texture giữ nguyên — chỉ thêm model JSON.
        """
        from services.visuals import totem
        from services.resources import itemmodel
        norm = totem.normalize(spec)
        result = self._export_texture_pack(
            name, norm, totem.render_png(norm),
            legacy_rel="assets/minecraft/textures/item/totem_of_undying.png",
            sprite_rel="assets/minecraft/textures/item/totem_of_undying.png",
            mc_version=mc_version, install_instance_id=install_instance_id,
            overwrite=overwrite, kind="totem",
            extra_files=itemmodel.export_files(
                "totem_of_undying",
                "minecraft:item/totem_of_undying", mc_version))
        return result

    # ------------------------------------------------------------------
    # HUD (mục 11)
    # ------------------------------------------------------------------

    def hud_widgets(self) -> dict:
        from services.visuals import hud
        return {"widgets": list(hud.WIDGETS), "defaultLayout": hud.DEFAULT_LAYOUT}

    def save_hud_layout(self, project_id: str, layout) -> dict:
        """Lưu HUD layout vào visual project (sanitize + merge defaults)."""
        from services.visuals import hud
        rs = self._rs
        project = rs.get(project_id)
        errors = hud.validate(layout)
        if any("unknown" in e["error"] or "duplicate" in e["error"] for e in errors):
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"Invalid HUD layout: {errors}")
        visuals = {**project.get("visuals", {}), "hud": hud.merge_default(layout)}
        rs.projects.update(project_id, {"visuals": visuals})
        return {"layout": visuals["hud"]}

    def render_preview_b64(self, spec: dict) -> str:
        """PNG -> base64 data URI (nhỏ, 1 lần mỗi thay đổi — mục 79: redraw theo state)."""
        import base64
        png = crosshair.render_png(spec, size=16)
        return "data:image/png;base64," + base64.b64encode(png).decode("ascii")

    # ------------------------------------------------------------------
    # FX — Hit Effects + Particles (mục 52 subsections)
    # ------------------------------------------------------------------

    def fx_defaults(self) -> dict:
        """Spec mặc định + danh sách lựa chọn cho UI (hit + particle)."""
        from services.visuals import fx
        return {
            "hit": {"default": fx.DEFAULT_HIT, "kinds": list(fx.HIT_KINDS)},
            "particle": {"default": fx.DEFAULT_PARTICLE,
                         "shapes": list(fx.PARTICLE_SHAPES),
                         "maxFrames": fx.P_MAX_FRAMES},
        }

    def render_hit_b64(self, spec: dict) -> str:
        import base64
        from services.visuals import fx
        png = fx.render_hit_png(fx.normalize_hit(spec))
        return "data:image/png;base64," + base64.b64encode(png).decode("ascii")

    def render_particle_b64(self, spec: dict) -> str:
        """Frame đầu tiên của atlas (render frames=1) cho preview vuông."""
        import base64
        from services.visuals import fx
        s = fx.normalize_particle(spec)
        png = fx.render_particle_png({**s, "frames": 1})
        return "data:image/png;base64," + base64.b64encode(png).decode("ascii")

    def export_fx_pack(self, name: str, hit_spec: dict | None, particle_spec: dict | None,
                       mc_version: str, *, install_instance_id: str | None = None,
                       overwrite: bool = False) -> dict:
        """Hit overlay + particle atlas -> 1 RS project -> build -> install.
        Chỉ phần được cung cấp mới được ghi texture (cả hai None -> lỗi)."""
        from services.visuals import fx as fxmod
        if not hit_spec and not particle_spec:
            raise AntaresError(codes.VALIDATION_FAILED,
                               "Cần ít nhất hit_spec hoặc particle_spec")
        if not name or not name.strip():
            raise AntaresError(codes.VALIDATION_FAILED, "Visual name required")

        rs = self._rs
        project = rs.create(name.strip(), mc_version, template="blank")
        pdir = rs.projects.dir_of(project["id"])
        gen = pdir / "generated"
        visuals = {"crosshair": {}, "totem": {}, "hud": {}}

        if hit_spec:
            norm_hit = fxmod.normalize_hit(hit_spec)
            hit_png = fxmod.render_hit_png(norm_hit)
            rel = "assets/minecraft/textures/gui/sprites/hit.png"
            (gen / rel).parent.mkdir(parents=True, exist_ok=True)
            (gen / rel).write_bytes(hit_png)
            visuals["hit"] = norm_hit

        if particle_spec:
            norm_part = fxmod.normalize_particle(particle_spec)
            part_png = fxmod.render_particle_png(norm_part)
            prel = "assets/minecraft/textures/particle/glow.png"
            (gen / prel).parent.mkdir(parents=True, exist_ok=True)
            (gen / prel).write_bytes(part_png)
            (gen / (prel + ".mcmeta")).write_text(
                fxmod.particle_mcmeta(norm_part["frames"]), encoding="utf-8")
            visuals["particle"] = norm_part

        rs.projects.update(project["id"], {"visuals": visuals})
        built = rs.build(project["id"])
        result = {"projectId": project["id"], "build": built, "visuals": visuals}
        if install_instance_id:
            result["install"] = rs.install(project["id"], install_instance_id,
                                           overwrite=overwrite)
        try:
            from core.events import names as ev
            self._ctx.events.publish(ev.VISUAL_EXPORTED, {
                "kind": "fx", "projectId": project["id"],
                "installed": bool(install_instance_id),
            })
        except Exception:
            pass
        logger.info("Exported fx pack %s (installed=%s)", project["id"],
                    bool(install_instance_id))
        return result

    # ------------------------------------------------------------------
    # Export -> resource pack -> (tuỳ chọn) install
    # ------------------------------------------------------------------

    def export_pack(self, name: str, spec: dict, mc_version: str,
                    *, install_instance_id: str | None = None,
                    overwrite: bool = False) -> dict:
        """Crosshair spec -> resource pack project -> ZIP -> install tuỳ chọn."""
        if not name or not name.strip():
            raise AntaresError(codes.VALIDATION_FAILED, "Visual name required")
        norm = crosshair.normalize(spec)
        png = crosshair.render_png(norm, size=16)
        # Version path qua versioning (mục 6.1 — không hardcode ngưỡng rải rác)
        from services.resources.versioning import crosshair_path
        rel = crosshair_path(mc_version)
        return self._export_texture_pack(
            name, norm, png, legacy_rel=rel, sprite_rel=rel,
            mc_version=mc_version, install_instance_id=install_instance_id,
            overwrite=overwrite, kind="crosshair")

    def _export_texture_pack(self, name: str, norm: dict, png: bytes, *,
                             legacy_rel: str, sprite_rel: str,
                             mc_version: str, install_instance_id: str | None,
                             overwrite: bool, kind: str,
                             extra_files: dict | None = None) -> dict:

        rs = self._rs
        project = rs.create(name.strip(), mc_version, template="blank")

        pdir = rs.projects.dir_of(project["id"])
        target = pdir / "generated" / sprite_rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(png)

        # Model JSON version-aware (Batch 2 — mục 7): key str = JSON dict,
        # tuple ("png", "#hex") = PNG 1x1 màu palette
        for rel, payload in (extra_files or {}).items():
            dest = pdir / "generated" / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            if isinstance(payload, tuple) and payload[0] == "png":
                from services.resources import itemmodel
                dest.write_bytes(itemmodel.color_png_bytes(payload[1]))
            else:
                dest.write_text(json.dumps(payload, indent=2, ensure_ascii=False),
                                encoding="utf-8")

        # Lưu spec vào project visuals (mục 34: JSON chứ không binary)
        visuals = {"crosshair": {}, "totem": {}, "hud": {}}
        visuals[kind] = norm
        rs.projects.update(project["id"], {"visuals": visuals})

        built = rs.build(project["id"])
        result = {"projectId": project["id"], "build": built, "spec": norm}

        if install_instance_id:
            result["install"] = rs.install(project["id"], install_instance_id,
                                           overwrite=overwrite)
        try:
            from core.events import names as ev
            self._ctx.events.publish(ev.VISUAL_EXPORTED, {
                "kind": kind, "projectId": project["id"],
                "installed": bool(install_instance_id),
            })
        except Exception:
            pass
        logger.info("Exported %s pack %s (installed=%s)", kind, project["id"],
                    bool(install_instance_id))
        return result
