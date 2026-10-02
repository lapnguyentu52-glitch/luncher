"""Model3D backend — VoxelSpec validation (master plan v3 mục 8, 15).

Nguồn sự thật cho Totem 3D (Batch 4 UI) và item model pipeline (Batch 2 gọi
validate trước khi compile — mục 14: ERROR block build, WARNING cho qua).

Geometry rules (mục 8.1):
- grid 16; tọa độ cho phép float (decision Batch 0 — model MC chuẩn nhận).
- Element bounds Minecraft: from/to trong [-16, 32] từng trục.
- from < to từng trục; không NaN/Infinity.
- Mỗi cube ID duy nhất; face key chỉ 6 mặt hợp lệ.
- UV: [x1, y1, x2, y2] với x1 < x2, y1 < y2 (đảo = ERROR fixable — mục 94);
  ngoài [0, 16] = WARNING (MC tự tile).

Severity: INFO/WARNING/ERROR/FATAL (mục 15.1). Finding shape mục 15.2:
{severity, code, path, message, fixable}. Error codes thuộc taxonomy mục 33.
"""
from __future__ import annotations

import math
from typing import Any

GRID = 16

#: Giới hạn element Minecraft (block coords): [-16, 32]
COORD_MIN = -16.0
COORD_MAX = 32.0

#: Policy số cube (mục 35): 32 khuyến nghị, 64 warning, 256 hard cap
RECOMMENDED_CUBES = 32
WARN_CUBES = 64
MAX_CUBES = 256

_FACES = ("north", "south", "east", "west", "up", "down")

_HEX_PREFIX = "#"


def _is_hex(value: str) -> bool:
    v = value.strip()
    if not v.startswith(_HEX_PREFIX) or len(v) not in (4, 7):
        return False
    try:
        int(v[1:], 16)
        return True
    except ValueError:
        return False


def _is_asset_path(value: str) -> bool:
    v = value.strip().replace("\\", "/")
    return (v.startswith("assets/") and v.endswith(".png")
            and "/" in v[len("assets/"):-len(".png")])


def finding(severity: str, code: str, path: str, message: str,
            fixable: bool = False) -> dict:
    """Finding chuẩn mục 15.2."""
    return {"severity": severity, "code": code, "path": path,
            "message": message, "fixable": fixable}


def has_errors(findings: list[dict]) -> bool:
    return any(f["severity"] in ("ERROR", "FATAL") for f in findings)


def errors(findings: list[dict]) -> list[dict]:
    return [f for f in findings if f["severity"] in ("ERROR", "FATAL")]


def warnings(findings: list[dict]) -> list[dict]:
    return [f for f in findings if f["severity"] == "WARNING"]


# ----------------------------------------------------------------------
# Validate
# ----------------------------------------------------------------------

def validate(spec: Any) -> list[dict]:
    """VoxelSpec -> list findings. [] = hợp lệ hoàn toàn."""
    out: list[dict] = []
    if not isinstance(spec, dict):
        out.append(finding("FATAL", "MODEL_SPEC_INVALID", "model3d",
                           "Model spec phải là object"))
        return out

    grid = spec.get("grid", GRID)
    if not isinstance(grid, int) or grid <= 0:
        out.append(finding("ERROR", "MODEL_SPEC_INVALID", "model3d.grid",
                           f"grid phải là int dương, thấy {grid!r}"))

    cubes = spec.get("cubes")
    if not isinstance(cubes, list):
        out.append(finding("FATAL", "MODEL_SPEC_INVALID", "model3d.cubes",
                           "cubes phải là array"))
        return out
    if not cubes:
        out.append(finding("ERROR", "MODEL_NO_CUBES", "model3d.cubes",
                           "Model không có cube nào", fixable=True))
        return out

    if len(cubes) > MAX_CUBES:
        out.append(finding("FATAL", "MODEL_CUBE_LIMIT", "model3d.cubes",
                           f"{len(cubes)} cubes > hard cap {MAX_CUBES}"))
    elif len(cubes) > WARN_CUBES:
        out.append(finding("WARNING", "MODEL_CUBE_LIMIT", "model3d.cubes",
                           f"{len(cubes)} cubes > {WARN_CUBES} — có thể chậm"))

    textures = spec.get("textures")
    if textures is not None and not isinstance(textures, dict):
        out.append(finding("ERROR", "MODEL_TEXTURE_INVALID", "model3d.textures",
                           "textures phải là object"))

    seen_ids: dict[str, int] = {}
    for i, cube in enumerate(cubes):
        out.extend(_validate_cube(i, cube, textures if isinstance(textures, dict) else {},
                                  seen_ids))
    return out


def _validate_cube(i: int, cube: Any, textures: dict,
                   seen_ids: dict[str, int]) -> list[dict]:
    out: list[dict] = []
    path = f"model3d.cubes[{i}]"
    if not isinstance(cube, dict):
        out.append(finding("ERROR", "MODEL_SPEC_INVALID", path,
                           "cube phải là object"))
        return out

    # ID — duplicate chỉ warning (normalize có thể rename)
    cid = cube.get("id")
    if isinstance(cid, str) and cid:
        if cid in seen_ids:
            out.append(finding("WARNING", "MODEL_ID_DUPLICATE", f"{path}.id",
                               f"ID '{cid}' trùng cubes[{seen_ids[cid]}]"))
        else:
            seen_ids[cid] = i
    else:
        out.append(finding("ERROR", "MODEL_SPEC_INVALID", f"{path}.id",
                           "cube.id phải là string không rỗng", fixable=True))

    # from/to — số hợp lệ + bounds
    coords: dict[str, list[float]] = {}
    for key in ("from", "to"):
        raw = cube.get(key)
        if not isinstance(raw, (list, tuple)) or len(raw) != 3:
            out.append(finding("ERROR", "MODEL_COORD_INVALID", f"{path}.{key}",
                               f"{key} phải là array 3 số", fixable=True))
            coords[key] = [0.0, 0.0, 0.0]
            continue
        vals: list[float] = []
        for j, v in enumerate(raw):
            if isinstance(v, bool) or not isinstance(v, (int, float)) \
                    or isinstance(v, complex) or not math.isfinite(float(v)):
                out.append(finding("ERROR", "MODEL_COORD_INVALID",
                                   f"{path}.{key}[{j}]",
                                   f"giá trị không phải số hữu hạn: {v!r}",
                                   fixable=True))
                vals.append(0.0)
            else:
                fv = float(v)
                if not (COORD_MIN <= fv <= COORD_MAX):
                    out.append(finding("ERROR", "MODEL_COORD_INVALID",
                                       f"{path}.{key}[{j}]",
                                       f"{fv} ngoài giới hạn element "
                                       f"[{COORD_MIN:g}, {COORD_MAX:g}]"))
                vals.append(fv)
        coords[key] = vals

    # Zero/negative size
    frm, to = coords["from"], coords["to"]
    for axis, a in enumerate("xyz"):
        size = to[axis] - frm[axis]
        if size <= 0:
            out.append(finding("ERROR", "MODEL_CUBE_EMPTY", path,
                               f"Cube có kích thước {'không dương' if size == 0 else 'âm'} "
                               f"trục {a}", fixable=True))

    # Faces
    faces = cube.get("faces")
    if faces is not None and not isinstance(faces, dict):
        out.append(finding("ERROR", "MODEL_FACE_INVALID", f"{path}.faces",
                           "faces phải là object"))
        faces = {}
    for fname, fs in (faces or {}).items():
        if fname not in _FACES:
            out.append(finding("ERROR", "MODEL_FACE_INVALID", f"{path}.faces.{fname}",
                               f"'{fname}' không phải mặt hợp lệ "
                               f"({', '.join(_FACES)})", fixable=True))
            continue
        fpath = f"{path}.faces.{fname}"
        if fs is None:
            continue
        if not isinstance(fs, dict):
            out.append(finding("ERROR", "MODEL_FACE_INVALID", fpath,
                               "face spec phải là object"))
            continue
        out.extend(_validate_face_material(fpath, fs.get("texture"), textures))
        out.extend(_validate_uv(fpath, fs.get("uv")))
    return out


def _validate_face_material(fpath: str, tex: Any, textures: dict) -> list[dict]:
    """Texture ref: hex trực tiếp | key trong spec.textures | asset path."""
    if tex is None:
        return []     # missing face -> default "base" ở compiler
    if not isinstance(tex, str) or not tex.strip():
        return [finding("ERROR", "MODEL_TEXTURE_MISSING", f"{fpath}.texture",
                        f"texture ref không hợp lệ: {tex!r}")]
    v = tex.strip()
    if _is_hex(v):
        return []
    if _is_asset_path(v):
        return []
    if isinstance(textures, dict) and v in textures:
        target = textures[v]
        if isinstance(target, str) and (_is_hex(target) or _is_asset_path(target)):
            return []
        return [finding("ERROR", "MODEL_TEXTURE_INVALID",
                        f"model3d.textures.{v}",
                        f"texture '{v}' trỏ tới giá trị không hợp lệ: {target!r}")]
    return [finding("ERROR", "MODEL_TEXTURE_MISSING", f"{fpath}.texture",
                    f"texture '{v}' không có trong spec.textures")]


def _validate_uv(fpath: str, uv: Any) -> list[dict]:
    if uv is None:
        return []
    if not isinstance(uv, (list, tuple)) or len(uv) != 4 \
            or not all(isinstance(u, (int, float)) and not isinstance(u, bool)
                       and math.isfinite(float(u)) for u in uv):
        return [finding("ERROR", "MODEL_UV_INVALID", f"{fpath}.uv",
                        "uv phải là array 4 số", fixable=True)]
    x1, y1, x2, y2 = (float(u) for u in uv)
    out: list[dict] = []
    if x2 <= x1 or y2 <= y1:
        out.append(finding("ERROR", "MODEL_UV_INVALID", f"{fpath}.uv",
                           f"uv đảo chiều: [{x1}, {y1}, {x2}, {y2}] "
                           f"(cần x1<x2, y1<y2)", fixable=True))
    if not (0 <= x1 <= GRID and 0 <= x2 <= GRID and 0 <= y1 <= GRID
            and 0 <= y2 <= GRID):
        out.append(finding("WARNING", "MODEL_UV_OUT_OF_RANGE", f"{fpath}.uv",
                           f"uv ngoài [0, {GRID}] — Minecraft sẽ tile texture"))
    return out


# ----------------------------------------------------------------------
# Normalize — sanitize an toàn cho renderer/UI (không âm thầm sửa geometry)
# ----------------------------------------------------------------------

def normalize(spec: Any) -> dict:
    """Clone + fix deterministic: rename duplicate ID, bỏ cube lỗi nghiêm trọng.

    Chỉ dùng cho render/preview; build pipeline phải qua validate() trước.
    """
    if not isinstance(spec, dict) or not isinstance(spec.get("cubes"), list):
        return {"grid": GRID, "cubes": [], "textures": {}}
    out: dict = {"grid": spec.get("grid", GRID) or GRID,
                 "textures": dict(spec.get("textures") or {})}
    cubes: list[dict] = []
    used: set[str] = set()
    for cube in spec["cubes"]:
        if not isinstance(cube, dict):
            continue
        c = dict(cube)
        cid = c.get("id")
        if not isinstance(cid, str) or not cid:
            cid = "cube"
        base, n = cid, 2
        while cid in used:
            cid = f"{base}-{n}"
            n += 1
        used.add(cid)
        c["id"] = cid
        cubes.append(c)
    out["cubes"] = cubes
    return out
