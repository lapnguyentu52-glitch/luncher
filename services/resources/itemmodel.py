"""Item model pipeline (master plan v3 mục 7) — VoxelSpec -> model JSON.

Shared geometry compiler sinh JSON cho 2 branch (mục 7.1), switch theo
versioning.item_model_mode():

- legacy (< 1.21.4):  assets/<ns>/models/item/<item>.json
  {"parent": "item/generated", "textures": {"layer0": "minecraft:item/..."}}
- definition (>= 1.21.4): assets/<ns>/items/<item>.json
  {"model": {"type": "minecraft:model", "model": "minecraft:item/..."}}
  + models/item/<item>.json (elements geometry hoặc parent item/generated)

Schema definition xác minh 2026-09-26: datapack.wiki/guide/adding-new-features/
custom-items/models (review cho MC 26.2), minecraft.wiki Items model definition
— format không đổi từ 1.21.4 đến 26.2: {"model": {"type": "minecraft:model",
"model": "<ref>"}}.

VoxelSpec (mục 5.3, 8): {"grid": 16, "cubes": [{"id", "from", "to", "faces":
{"north"/"south"/"east"/"west"/"up"/"down": {"texture": <key-or-hex>,
"uv": [x1, y1, x2, y2]}}}], "textures": {<key>: "#hex" | "assets/...png"}}.

Giới hạn v1 (mục 35): không shade, không rotation, không tint, không display
transform. Mỗi cube = 6 element faces; UV theo mặt chiếu chuẩn MC, đơn vị
pixel 0..16 (1 block = 16). Texture màu -> PNG 1x1 màu trong assets/antares/.
"""
from __future__ import annotations

import re
from typing import Any

from services.resources import versioning
from services.visuals import model3d

#: 1 block = 16 pixel; totem dùng grid 16 (mục 8.1)
GRID = 16

#: Namespace nội bộ cho texture màu sinh ra
COLOR_NS = "antares"

_FACES = ("north", "south", "east", "west", "up", "down")

_HEX_RE = re.compile(r"^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$")
_ASSET_RE = re.compile(r"^assets/([^/]+)/textures/(.+)\.png$")


def _is_hex(value: str) -> bool:
    return bool(_HEX_RE.match(value.strip()))


def _asset_rel_to_ref(rel: str) -> str | None:
    """'assets/minecraft/textures/item/x.png' -> 'minecraft:item/x'."""
    m = _ASSET_RE.match(rel.replace("\\", "/").strip())
    return f"{m.group(1)}:{m.group(2)}" if m else None


# ----------------------------------------------------------------------
# VoxelSpec -> elements (mục 7.1 shared geometry compiler)
# ----------------------------------------------------------------------

def _default_uv(size: tuple[float, float, float], face: str) -> list[float]:
    """UV full-face theo mặt chiếu chuẩn Minecraft (pixel 0..16).

    north/south chiếu (x, y); east/west chiếu (z, y); up/down chiếu (x, z).
    """
    sx, sy, sz = size
    if face in ("north", "south"):
        return [0.0, 0.0, sx, sy]
    if face in ("east", "west"):
        return [0.0, 0.0, sz, sy]
    return [0.0, 0.0, sx, sz]      # up/down


def compile_elements(spec: dict) -> dict:
    """VoxelSpec -> model dict + danh sách texture màu cần sinh PNG.

    Return:
    {
      "model": {"textures": {"tex0": "antares:item/tex0", ...},
                "elements": [{"from", "to", "faces": {face: {texture, uv}}}]},
      "colorTextures": {"#e8b23a": "assets/antares/textures/item/tex0.png"},
      "assetTextures": ["minecraft:item/apple", ...]   # ref asset được tham chiếu
    }

    Texture resolution per face (mục 8.3):
      1. "#hex" trực tiếp trong face.texture
      2. textures[key] = "#hex"  (màu đặt tên trong spec.textures)
      3. textures[key] = "assets/...png" (asset trong pack)
      4. face.texture = "assets/...png" trực tiếp
      5. fallback: FALLBACK màu vàng totem (validator WARN khi gặp)
    """
    cubes = spec.get("cubes") or []
    spec_textures = spec.get("textures") or {}
    if not isinstance(spec_textures, dict):
        spec_textures = {}

    color_index: dict[str, str] = {}     # "#hex" -> slot "tex0"
    color_pngs: dict[str, str] = {}      # "#hex" -> rel path PNG 1x1
    asset_refs: list[str] = []

    def resolve(value: Any) -> tuple[str | None, str | None]:
        """-> (hex | None, ref | None). ref = namespaced texture ref."""
        if not isinstance(value, str) or not value.strip():
            return None, None
        v = value.strip()
        if _is_hex(v):
            return v.lower(), None
        ref = _asset_rel_to_ref(v)
        if ref:
            return None, ref
        return None, None

    def slot_for_color(hexv: str) -> str:
        if hexv not in color_index:
            n = len(color_index)
            slot = f"tex{n}"
            color_index[hexv] = slot
            color_pngs[hexv] = f"assets/{COLOR_NS}/textures/item/{slot}.png"
        return color_index[hexv]

    def face_material(face_spec: Any, default_key: str) -> tuple[str | None, str | None]:
        """Resolve material 1 face -> (hex | None, ref | None)."""
        if isinstance(face_spec, dict):
            raw = face_spec.get("texture", default_key)
            hexv, ref = resolve(raw)
            if hexv or ref:
                return hexv, ref
            # key lookup trong spec.textures
            if isinstance(raw, str):
                return resolve(spec_textures.get(raw))
            return None, None
        return resolve(face_spec if isinstance(face_spec, str) else None)

    textures: dict[str, str] = {}
    elements: list[dict] = []

    for cube in cubes:
        if not isinstance(cube, dict):
            continue
        try:
            frm = [float(x) for x in (cube.get("from") or [0, 0, 0])]
            to = [float(x) for x in (cube.get("to") or [0, 0, 0])]
        except (TypeError, ValueError):
            continue
        if any(to[i] == frm[i] for i in range(3)):
            continue        # cube zero-size bỏ qua — validator ở Batch 3 sẽ bắt
        size = (abs(to[0] - frm[0]), abs(to[1] - frm[1]), abs(to[2] - frm[2]))
        lo = [min(frm[i], to[i]) for i in range(3)]

        spec_faces = cube.get("faces") if isinstance(cube.get("faces"), dict) else {}
        faces_out: dict[str, dict] = {}
        for face in _FACES:
            hexv, ref = face_material(spec_faces.get(face), "base")
            if hexv:
                slot = slot_for_color(hexv)
                textures.setdefault(slot, f"{COLOR_NS}:item/{slot}")
                ref_out = f"#{slot}"
            elif ref:
                # slot riêng cho asset để 2 face không đè nhau
                slot = f"tex{len(textures)}"
                textures[slot] = ref
                ref_out = f"#{slot}"
                if ref not in asset_refs:
                    asset_refs.append(ref)
            else:
                slot = slot_for_color("#e8b23a")
                textures.setdefault(slot, f"{COLOR_NS}:item/{slot}")
                ref_out = f"#{slot}"

            uv_in = spec_faces.get(face, {}).get("uv") \
                if isinstance(spec_faces.get(face), dict) else None
            uv = ([float(u) for u in uv_in]
                  if isinstance(uv_in, (list, tuple)) and len(uv_in) == 4
                  else _default_uv(size, face))
            faces_out[face] = {"texture": ref_out, "uv": uv}

        elements.append({"from": lo,
                         "to": [lo[i] + size[i] for i in range(3)],
                         "faces": faces_out})

    return {
        "model": {"textures": textures, "elements": elements},
        "colorTextures": color_pngs,
        "assetTextures": asset_refs,
    }


def color_png_bytes(hex_color: str) -> bytes:
    """PNG 1x1 RGBA từ hex — cho texture palette màu (mục 8.4 normalize)."""
    from services.resources.templates import encode_png
    c = hex_color.lstrip("#")
    if len(c) == 3:
        c = "".join(ch * 2 for ch in c)
    rgb = bytes.fromhex(c)
    return encode_png(1, 1, rgb + b"\xff")


# ----------------------------------------------------------------------
# Generators — legacy (7.2) và definition (7.3)
# ----------------------------------------------------------------------

def flat_model(texture_ref: str) -> dict:
    """Model 'item/generated' cho texture phẳng (2 branch dùng chung)."""
    return {"parent": "item/generated", "textures": {"layer0": texture_ref}}


def legacy_item_model(texture_ref: str) -> dict:
    """Branch legacy (< 1.21.4, mục 7.2): chỉ models/item/<item>.json."""
    return flat_model(texture_ref)


def definition_item_files(item: str, texture_ref: str) -> dict[str, dict]:
    """Branch modern (>= 1.21.4, mục 7.3): items/<item>.json + models/item/.

    Schema xác minh: datapack.wiki + minecraft.wiki Items model definition.
    """
    definition = {"model": {"type": "minecraft:model",
                            "model": f"minecraft:item/{item}"}}
    return {
        f"assets/minecraft/items/{item}.json": definition,
        f"assets/minecraft/models/item/{item}.json": flat_model(texture_ref),
    }


def voxel_item_files(item: str, spec: dict) -> dict[str, Any]:
    """VoxelSpec -> files geometry elements.

    Return {rel_path: json_dict | ("png", hex)}:
    - model JSON tại assets/minecraft/models/item/<item>.json
    - item definition tại assets/minecraft/items/<item>.json (loại branch
      legacy ở export_files — geometry luôn sinh đầy đủ rồi lọc)
    - PNG màu dạng tuple ("png", "#hex") — caller encode + ghi file
    """
    compiled = compile_elements(spec)
    files: dict[str, Any] = {
        f"assets/minecraft/models/item/{item}.json": compiled["model"],
        f"assets/minecraft/items/{item}.json": {
            "model": {"type": "minecraft:model", "model": f"minecraft:item/{item}"}},
    }
    for hexv, rel in compiled["colorTextures"].items():
        files[rel] = ("png", hexv)
    return files


def export_files(item: str, texture_ref: str, mc_version: str,
                 spec: dict | None = None) -> dict[str, Any]:
    """API chính (mục 7): dispatcher theo versioning.item_model_mode().

    - spec None, mode legacy:     {models/item/<item>.json: item/generated}
    - spec None, mode definition: {items/<item>.json, models/item/<item>.json}
    - spec dict (có cubes):       model elements; branch legacy bỏ file
      items/<item>.json (game cũ không đọc — ghi ra gây wrong_path warning).
    """
    if isinstance(spec, dict) and spec.get("cubes"):
        findings = model3d.validate(spec)
        if model3d.has_errors(findings):
            from core.errors import codes
            from core.errors.base import AntaresError
            first = model3d.errors(findings)[0]
            raise AntaresError(
                codes.VALIDATION_FAILED,
                f"Model invalid: {first['code']} at {first['path']} "
                f"({first['message']})")
        files = voxel_item_files(item, spec)
        if versioning.item_model_mode(mc_version) == "definition":
            return files
        return {k: v for k, v in files.items() if not k.startswith(
            f"assets/minecraft/items/")}
    if versioning.item_model_mode(mc_version) == "definition":
        return definition_item_files(item, texture_ref)
    return {f"assets/minecraft/models/item/{item}.json":
            legacy_item_model(texture_ref)}
