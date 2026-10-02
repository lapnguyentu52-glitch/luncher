"""JAR reader & Java bytecode decoder — nền tảng cho mod scanner.

Decode Java class file format (JVMS spec) bằng pure Python, không cần JVM:
 - magic 0xCAFEBABE, version
 - constant pool đầy đủ (Utf8, Class, Methodref, MethodHandle, InvokeDynamic...)
 - access flags, this/super class, interfaces
 - fields & methods — PARSE Code attribute: walk bytecode thu per-method invokes
 - BootstrapMethods attribute — phục vụ InvokeDynamic analysis (lambda factory,
   StringConcat, custom bootstrap = ẩn logic động)
"""
from __future__ import annotations

import struct
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

MAGIC = 0xCAFEBABE

# Constant pool tags (JVMS §4.4)
CONSTANT_Utf8 = 1
CONSTANT_Integer = 3
CONSTANT_Float = 4
CONSTANT_Long = 5
CONSTANT_Double = 6
CONSTANT_Class = 7
CONSTANT_String = 8
CONSTANT_Fieldref = 9
CONSTANT_Methodref = 10
CONSTANT_InterfaceMethodref = 11
CONSTANT_NameAndType = 12
CONSTANT_MethodHandle = 15
CONSTANT_MethodType = 16
CONSTANT_Dynamic = 17
CONSTANT_InvokeDynamic = 18
CONSTANT_Module = 19
CONSTANT_Package = 20

# Opcodes để walk Code attribute
OP_INVOKENONVIRTUAL = 0x77  # invokespecial (old name)
OP_INVOKESTATIC = 0xB8
OP_INVOKEVIRTUAL = 0xB6
OP_INVOKESPECIAL = 0xB7
OP_INVOKEINTERFACE = 0xB9
OP_INVOKEDYNAMIC = 0xBA
OP_TABLESWITCH = 0xAA
OP_LOOKUPSWITCH = 0xAB
OP_WIDE = 0xC4

# Opcode -> (operand bytes sau opcode, mnemonic group); 0 = biến đặc biệt
_FIXED_LENGTHS: dict[int, int] = {
    0x00: 0, 0x01: 0, 0x02: 0, 0x03: 0, 0x04: 0, 0x05: 0, 0x06: 0, 0x07: 0,
    0x08: 0, 0x09: 0, 0x0A: 0, 0x0B: 0, 0x0C: 0, 0x0D: 0, 0x0E: 0, 0x0F: 0,
    0x10: 1, 0x11: 2, 0x12: 1,                                   # bipush, sipush, ldc
    0x13: 2, 0x14: 2,                                            # ldc_w, ldc2_w
    0x15: 1, 0x16: 1, 0x17: 1, 0x18: 1, 0x19: 1,                 # loads
    0x1A: 0, 0x1B: 0, 0x1C: 0, 0x1D: 0,
    0x1E: 0, 0x1F: 0, 0x20: 0, 0x21: 0,
    0x22: 0, 0x23: 0, 0x24: 0, 0x25: 0,
    0x26: 0, 0x27: 0, 0x28: 0, 0x29: 0,
    0x2A: 0, 0x2B: 0, 0x2C: 0, 0x2D: 0,
    0x2E: 0, 0x2F: 0, 0x30: 0, 0x31: 0, 0x32: 0, 0x33: 0, 0x34: 0, 0x35: 0,
    0x36: 1, 0x37: 1, 0x38: 1, 0x39: 1, 0x3A: 1,                 # stores
    0x3B: 0, 0x3C: 0, 0x3D: 0, 0x3E: 0,
    0x3F: 0, 0x40: 0, 0x41: 0, 0x42: 0,
    0x43: 0, 0x44: 0, 0x45: 0, 0x46: 0,
    0x47: 0, 0x48: 0, 0x49: 0, 0x4A: 0,
    0x4B: 0, 0x4C: 0, 0x4D: 0, 0x4E: 0,
    0x4F: 0, 0x50: 0, 0x51: 0, 0x52: 0, 0x53: 0, 0x54: 0, 0x55: 0, 0x56: 0,
    0x57: 0, 0x58: 0, 0x59: 0, 0x5A: 0, 0x5B: 0, 0x5C: 0, 0x5D: 0, 0x5E: 0,
    0x5F: 0,
    0x60: 0, 0x61: 0, 0x62: 0, 0x63: 0, 0x64: 0, 0x65: 0, 0x66: 0, 0x67: 0,
    0x68: 0, 0x69: 0, 0x6A: 0, 0x6B: 0, 0x6C: 0, 0x6D: 0, 0x6E: 0, 0x6F: 0,
    0x70: 0, 0x71: 0, 0x72: 0, 0x73: 0, 0x74: 0, 0x75: 0, 0x76: 0, 0x77: 0,
    0x78: 0, 0x79: 0, 0x7A: 0, 0x7B: 0, 0x7C: 0, 0x7D: 0, 0x7E: 0, 0x7F: 0,
    0x80: 0, 0x81: 0, 0x82: 0, 0x83: 0, 0x84: 1, 0x85: 0, 0x86: 0, 0x87: 0,
    0x88: 0, 0x89: 0, 0x8A: 0, 0x8B: 0, 0x8C: 0, 0x8D: 0, 0x8E: 0, 0x8F: 0,
    0x90: 0, 0x91: 0, 0x92: 0, 0x93: 0, 0x94: 0, 0x95: 0, 0x96: 0, 0x97: 0,
    0x98: 0, 0x99: 2, 0x9A: 2, 0x9B: 2, 0x9C: 2, 0x9D: 2, 0x9E: 2,  # if*
    0x9F: 2, 0xA0: 2, 0xA1: 2, 0xA2: 2, 0xA3: 2, 0xA4: 2, 0xA5: 2, 0xA6: 2,
    0xA7: 2, 0xA8: 2,                                            # goto, jsr
    0xA9: 0,                                                     # ret
    0xAC: 0, 0xAD: 0, 0xAE: 0, 0xAF: 0, 0xB0: 0, 0xB1: 0,        # returns
    0xB2: 2, 0xB3: 2, 0xB4: 2, 0xB5: 2,                          # get/putstatic/field
    0xB6: 2, 0xB7: 2, 0xB8: 2,                                   # invoke*
    0xB9: 4, 0xBA: 4,                                            # invokeinterface/invokedynamic
    0xBB: 2, 0xBC: 1, 0xBD: 2,                                   # new, newarray, anewarray
    0xBE: 0, 0xBF: 0,                                            # arraylength, athrow
    0xC0: 2, 0xC1: 2,                                            # checkcast, instanceof
    0xC2: 0, 0xC3: 0,                                            # monitorenter/exit
    0xC5: 3, 0xC6: 2, 0xC7: 2,                                   # multianewarray, ifnull
    0xC8: 4, 0xC9: 4,                                            # goto_w, jsr_w
    0xCA: 0,                                                     # breakpoint
}
# Opcodes bỏ qua (reserved, quick, impdep)
for _r in list(range(0xCA, 0xFF)) + [0xFE, 0xFF]:
    _FIXED_LENGTHS.setdefault(_r, 0)
_FIXED_LENGTHS[0xCB] = 0


@dataclass
class MethodHandleInfo:
    kind: int
    ref_class: str = ""
    ref_name: str = ""


@dataclass
class BootstrapMethod:
    handle: MethodHandleInfo | None = None
    args_classes: list[str] = field(default_factory=list)


@dataclass
class ClassInfo:
    """Thông tin decode được từ 1 file .class."""
    name: str = ""
    super_name: str = ""
    interfaces: list[str] = field(default_factory=list)
    version_major: int = 0
    version_minor: int = 0
    strings: list[str] = field(default_factory=list)
    class_refs: list[str] = field(default_factory=list)
    method_refs: list[str] = field(default_factory=list)   # "class.method"
    field_refs: list[str] = field(default_factory=list)
    # Call graph: callee class (internal) <- từng instruction invoke trong Code
    callees: set[str] = field(default_factory=set)
    # InvokeDynamic
    invokedynamic_count: int = 0
    bootstrap_methods: list[BootstrapMethod] = field(default_factory=list)
    bootstrap_classes: set[str] = field(default_factory=set)  # class của bootstrap handle
    # Reflection
    uses_reflection: bool = False     # Class.forName / Method.invoke
    uses_method_handles: bool = False # MethodHandles.Lookup / invoke

    @property
    def java_version(self) -> int:
        """Major 52 = Java 8, 61 = Java 17, 65 = Java 21."""
        return self.version_major - 44 if self.version_major >= 45 else self.version_major


class ClassFileError(Exception):
    pass


def decode_class(data: bytes) -> ClassInfo:
    """Decode 1 file .class từ bytes. Raise ClassFileError nếu không phải class."""
    if len(data) < 10 or struct.unpack(">I", data[:4])[0] != MAGIC:
        raise ClassFileError("Not a Java class file (bad magic)")

    info = ClassInfo()
    minor, major = struct.unpack(">HH", data[4:8])
    info.version_minor, info.version_major = minor, major

    pos = 8
    cp_count = struct.unpack(">H", data[pos:pos + 2])[0]
    pos += 2

    # ---- Parse constant pool (1-indexed; long/double chiếm 2 slot) ----
    pool: dict[int, dict] = {}
    i = 1
    while i < cp_count:
        tag = data[pos]
        pos += 1
        if tag == CONSTANT_Utf8:
            length = struct.unpack(">H", data[pos:pos + 2])[0]
            pos += 2
            raw = data[pos:pos + length]
            pos += length
            try:
                pool[i] = {"tag": tag, "value": raw.decode("utf-8", errors="replace")}
            except Exception:
                pool[i] = {"tag": tag, "value": ""}
        elif tag in (CONSTANT_Integer, CONSTANT_Float):
            pos += 4
            pool[i] = {"tag": tag}
        elif tag in (CONSTANT_Long, CONSTANT_Double):
            pos += 8
            pool[i] = {"tag": tag}
            i += 1  # chiếm 2 slot
        elif tag in (CONSTANT_Class, CONSTANT_String, CONSTANT_MethodType,
                     CONSTANT_Module, CONSTANT_Package):
            pool[i] = {"tag": tag, "index": struct.unpack(">H", data[pos:pos + 2])[0]}
            pos += 2
        elif tag in (CONSTANT_Fieldref, CONSTANT_Methodref, CONSTANT_InterfaceMethodref,
                     CONSTANT_NameAndType, CONSTANT_Dynamic, CONSTANT_InvokeDynamic):
            a, b = struct.unpack(">HH", data[pos:pos + 4])
            pos += 4
            key = ("class", "nat") if tag != CONSTANT_NameAndType else ("nat",)
            if tag == CONSTANT_NameAndType:
                pool[i] = {"tag": tag, "name_idx": a, "desc_idx": b}
            elif tag == CONSTANT_InvokeDynamic:
                pool[i] = {"tag": tag, "bootstrap_index": a, "nat_index": b}
            else:
                pool[i] = {"tag": tag, "class_index": a, "nat_index": b}
        elif tag == CONSTANT_MethodHandle:
            kind, ref = struct.unpack(">BH", data[pos:pos + 3])
            pos += 3
            pool[i] = {"tag": tag, "kind": kind, "ref_index": ref}
        else:
            raise ClassFileError(f"Unknown constant tag {tag} at #{i}")
        i += 1

    def utf8(idx: int) -> str:
        entry = pool.get(idx)
        return entry.get("value", "") if entry else ""

    def class_name(idx: int) -> str:
        entry = pool.get(idx)
        if not entry or entry["tag"] != CONSTANT_Class:
            return ""
        return utf8(entry["index"]).replace("/", ".")

    def nat_name(nat_idx: int) -> str:
        entry = pool.get(nat_idx)
        if not entry or entry["tag"] != CONSTANT_NameAndType:
            return ""
        return utf8(entry["name_idx"])

    # ---- MethodHandle resolve ----
    def method_handle(idx: int) -> MethodHandleInfo | None:
        entry = pool.get(idx)
        if not entry or entry["tag"] != CONSTANT_MethodHandle:
            return None
        ref = pool.get(entry["ref_index"], {})
        mh = MethodHandleInfo(kind=entry["kind"])
        if ref.get("tag") in (CONSTANT_Methodref, CONSTANT_InterfaceMethodref,
                              CONSTANT_Fieldref):
            mh.ref_class = class_name(ref.get("class_index", 0))
            mh.ref_name = nat_name(ref.get("nat_index", 0))
        return mh

    # ---- Access flags + this/super/interfaces ----
    access, this_idx, super_idx = struct.unpack(">HHH", data[pos:pos + 6])
    pos += 6
    info.name = class_name(this_idx)
    info.super_name = class_name(super_idx)

    if_count = struct.unpack(">H", data[pos:pos + 2])[0]
    pos += 2
    for _ in range(if_count):
        info.interfaces.append(class_name(struct.unpack(">H", data[pos:pos + 2])[0]))
        pos += 2

    # ---- Collect strings & refs từ pool ----
    invoke_refs_by_index: dict[int, str] = {}
    for idx, entry in pool.items():
        tag = entry["tag"]
        if tag == CONSTANT_Utf8:
            s = entry.get("value", "")
            if len(s) >= 4:
                info.strings.append(s)
        elif tag == CONSTANT_String:
            s = utf8(entry["index"])
            if s:
                info.strings.append(s)
        elif tag in (CONSTANT_Methodref, CONSTANT_InterfaceMethodref):
            cls = class_name(entry.get("class_index", 0))
            name = nat_name(entry.get("nat_index", 0))
            ref = f"{cls}.{name}"
            invoke_refs_by_index[idx] = ref
            info.method_refs.append(ref)
        elif tag == CONSTANT_Fieldref:
            cls = class_name(entry.get("class_index", 0))
            name = nat_name(entry.get("nat_index", 0))
            info.field_refs.append(f"{cls}.{name}")
        elif tag == CONSTANT_Class:
            name = utf8(entry.get("index", 0))
            if name:
                info.class_refs.append(name.replace("/", "."))

    # ---- Fields & methods (parse Code attribute) ----
    def skip_or_parse_members(pos: int, *, parse_code: bool) -> int:
        count = struct.unpack(">H", data[pos:pos + 2])[0]
        pos += 2
        for _ in range(count):
            pos += 6  # access, name_idx, desc_idx
            attrs = struct.unpack(">H", data[pos:pos + 2])[0]
            pos += 2
            for _ in range(attrs):
                a_name = utf8(struct.unpack(">H", data[pos:pos + 2])[0])
                pos += 2
                alen = struct.unpack(">I", data[pos:pos + 4])[0]
                pos += 4
                if parse_code and a_name == "Code" and alen >= 8:
                    code_body = data[pos:pos + alen]
                    _parse_code_attribute(info, code_body, invoke_refs_by_index,
                                          pool, class_name, nat_name)
                pos += alen
        return pos

    pos = skip_or_parse_members(pos, parse_code=False)  # fields
    pos = skip_or_parse_members(pos, parse_code=True)   # methods

    # ---- Class attributes (BootstrapMethods) ----
    attrs = struct.unpack(">H", data[pos:pos + 2])[0]
    pos += 2
    for _ in range(attrs):
        a_name = utf8(struct.unpack(">H", data[pos:pos + 2])[0])
        pos += 2
        alen = struct.unpack(">I", data[pos:pos + 4])[0]
        pos += 4                      # bỏ qua u2 length trước khi đọc body
        if a_name == "BootstrapMethods":
            body = data[pos:pos + alen]
            bpos = 0
            num = struct.unpack(">H", body[bpos:bpos + 2])[0]
            bpos += 2
            for _ in range(num):
                mh_idx, arg_count = struct.unpack(">HH", body[bpos:bpos + 4])
                bpos += 4
                bm = BootstrapMethod(handle=method_handle(mh_idx))
                if bm.handle and bm.handle.ref_class:
                    info.bootstrap_classes.add(bm.handle.ref_class)
                for _a in range(arg_count):
                    arg_idx = struct.unpack(">H", body[bpos:bpos + 2])[0]
                    bpos += 2
                    arg_entry = pool.get(arg_idx, {})
                    if arg_entry.get("tag") == CONSTANT_MethodHandle:
                        mh = method_handle(arg_idx)
                        if mh and mh.ref_class:
                            bm.args_classes.append(mh.ref_class)
                    elif arg_entry.get("tag") == CONSTANT_Class:
                        bm.args_classes.append(class_name(arg_idx))
                info.bootstrap_methods.append(bm)
        pos += alen

    # ---- Reflection flags (invokedynamic đã đếm theo instruction trong Code walk,
    # tránh double-count vì mỗi instruction trỏ đúng 1 pool entry) ----
    ref_set = set(info.method_refs)
    info.uses_reflection = ("java.lang.Class.forName" in ref_set
                            and "java.lang.reflect.Method.invoke" in ref_set)
    info.uses_method_handles = any(
        c.startswith("java.lang.invoke.MethodHandles") for c in info.class_refs)
    return info


def _parse_code_attribute(info: ClassInfo, body: bytes,
                          invoke_refs: dict[int, str], pool: dict,
                          class_name_fn, nat_name_fn) -> None:
    """Parse Code attribute: walk bytecode thu callee classes + invokedynamic."""
    if len(body) < 8:
        return
    code_len = struct.unpack(">I", body[4:8])[0]
    code = body[8:8 + code_len]
    if len(code) != code_len:
        return

    pos = 0
    n = len(code)
    while pos < n:
        op = code[pos]
        pos += 1
        if op == OP_INVOKEDYNAMIC:
            if pos + 4 <= n:
                cp_idx = struct.unpack(">H", code[pos:pos + 2])[0]
                info.invokedynamic_count += 1
            pos += 4
            continue
        if op in (OP_INVOKEVIRTUAL, OP_INVOKESPECIAL, OP_INVOKESTATIC,
                  OP_INVOKENONVIRTUAL):
            if pos + 2 <= n:
                cp_idx = struct.unpack(">H", code[pos:pos + 2])[0]
                ref = invoke_refs.get(cp_idx)
                if ref:
                    callee = ref.rsplit(".", 1)[0]
                    info.callees.add(callee)
            pos += 2
            continue
        if op == OP_INVOKEINTERFACE:
            if pos + 4 <= n:
                cp_idx = struct.unpack(">H", code[pos:pos + 2])[0]
                ref = invoke_refs.get(cp_idx)
                if ref:
                    callee = ref.rsplit(".", 1)[0]
                    info.callees.add(callee)
            pos += 4
            continue
        if op == OP_TABLESWITCH:
            pad = (4 - (pos % 4)) % 4
            pos += pad
            # default + low + high + (high-low+1) offsets
            if pos + 12 <= n:
                low, high = struct.unpack(">ii", code[pos + 4:pos + 12])
                count = max(0, high - low + 1)
                pos += 12 + count * 4
            continue
        if op == OP_LOOKUPSWITCH:
            pad = (4 - (pos % 4)) % 4
            pos += pad
            if pos + 8 <= n:
                npairs = struct.unpack(">i", code[pos + 4:pos + 8])[0]
                npairs = max(0, npairs)
                pos += 8 + npairs * 8
            continue
        if op == OP_WIDE:
            # wide: next opcode + 2 bytes; iinc: +4
            if pos < n:
                next_op = code[pos]
                pos += 2 + (2 if next_op == 0x84 else 0)
            continue
        length = _FIXED_LENGTHS.get(op, 0)
        pos += length


def scan_jar_classes(jar_path: Path, *, max_classes: int = 500) -> list[ClassInfo]:
    """Decode tất cả .class trong jar (giới hạn để tránh DoS file cực lớn)."""
    infos: list[ClassInfo] = []
    with zipfile.ZipFile(jar_path, "r") as zf:
        class_entries = [n for n in zf.namelist()
                         if n.endswith(".class") and not n.endswith("module-info.class")]
        for name in class_entries[:max_classes]:
            try:
                infos.append(decode_class(zf.read(name)))
            except (ClassFileError, zipfile.BadZipFile, struct.error, IndexError):
                continue
    return infos


def jar_structure(jar_path: Path) -> dict:
    """Tổng quan jar: entry list + file trông khả nghi (nested jar, exe, dll)."""
    result = {"entries": 0, "classes": 0, "configs": [], "suspicious_files": [],
              "nested_jars": [], "fabric_mod_json": None, "forge_toml": None}
    suspicious_ext = (".exe", ".dll", ".so", ".bat", ".cmd", ".sh", ".ps1", ".vbs")
    try:
        with zipfile.ZipFile(jar_path, "r") as zf:
            names = zf.namelist()
            result["entries"] = len(names)
            for name in names:
                low = name.lower()
                if low.endswith(".class"):
                    result["classes"] += 1
                elif low.endswith((".json", ".toml")) and "/" not in low.rstrip("/"):
                    try:
                        content = zf.read(name).decode("utf-8", errors="replace")[:20000]
                        if "fabric" in low:
                            result["fabric_mod_json"] = content
                        elif "mods" in low and low.endswith(".toml"):
                            result["forge_toml"] = content
                        else:
                            result["configs"].append(name)
                    except Exception:
                        pass
                elif low.endswith(".jar"):
                    result["nested_jars"].append(name)
                elif low.endswith(suspicious_ext):
                    result["suspicious_files"].append(name)
    except zipfile.BadZipFile as e:
        raise ClassFileError(f"Corrupt jar: {e}") from e
    return result
