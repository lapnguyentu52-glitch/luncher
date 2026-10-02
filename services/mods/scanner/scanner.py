"""Mod security scanner — heuristic engine phân tích jar tìm behavior nguy hiểm.

Phát hiện (dựa trên decode bytecode):
 - Remote download & execute chain (threat cao nhất: tải file + exec)
 - Reverse shell / network bind
 - Credential theft (đọc authlib token, cookie, password store)
 - Crypto ransomware pattern (AES + file IO mass)
 - Obfuscation cực đoan + reflection bypass
 - Windows registry / process exec
 - Suspicious embedded files (exe/dll/bat trong jar)
 - InvokeDynamic với custom bootstrap (ẩn logic động khỏi bytecode tĩnh)
 - Call graph: path từ mod class tới exec/network sink (BFS depth-limited)

 Scoring: mỗi finding có weight; verdict = SAFE | SUSPICIOUS | DANGEROUS.
"""
from __future__ import annotations

import re
import time
from dataclasses import dataclass, field
from pathlib import Path

from services.mods.scanner.jar_reader import (
    ClassInfo, jar_structure, scan_jar_classes, ClassFileError,
)

VERDICT_SAFE = "SAFE"
VERDICT_SUSPICIOUS = "SUSPICIOUS"
VERDICT_DANGEROUS = "DANGEROUS"

DANGEROUS_THRESHOLD = 60
SUSPICIOUS_THRESHOLD = 25


@dataclass
class Finding:
    rule_id: str
    severity: str          # low | medium | high | critical
    weight: int
    title: str
    detail: str
    evidence: list[str] = field(default_factory=list)


@dataclass
class ScanReport:
    file: str
    size: int
    sha256: str
    verdict: str = VERDICT_SAFE
    score: int = 0
    findings: list[Finding] = field(default_factory=list)
    classes_scanned: int = 0
    java_version: int = 0
    mod_id: str | None = None
    mod_name: str | None = None
    scanned_at: float = field(default_factory=time.time)

    def to_dict(self) -> dict:
        return {
            "file": self.file, "size": self.size, "sha256": self.sha256,
            "verdict": self.verdict, "score": self.score,
            "classesScanned": self.classes_scanned,
            "javaVersion": self.java_version,
            "modId": self.mod_id, "modName": self.mod_name,
            "findings": [
                {"ruleId": f.rule_id, "severity": f.severity, "weight": f.weight,
                 "title": f.title, "detail": f.detail, "evidence": f.evidence[:5]}
                for f in self.findings
            ],
        }


# ---- Rule helpers ----

_URL_RE = re.compile(r"https?://[^\s\"']{8,}")

EXEC_CLASSES = ("java.lang.Runtime", "java.lang.ProcessBuilder")
EXEC_METHODS = ("java.lang.Runtime.exec", "java.lang.ProcessBuilder.start")

NET_CLASSES = ("java.net.Socket", "java.net.ServerSocket", "java.net.URL",
               "java.net.HttpURLConnection", "java.net.InetSocketAddress",
               "javax.net.ssl.HttpsURLConnection", "java.nio.channels.SocketChannel")

CREDENTIAL_PATHS = (
    ".minecraft/libraries/com/mojang/authlib", "authlib", "accessToken",
    "session_token", "login_data", "user data/default/login data",
    "cookies.sqlite", "credential", "key4.db", "logins.json",
    "appdata/local/microsoft", "appdata/roaming/.minecraft",
)

CRYPTO_CLASSES = ("javax/crypto/Cipher", "javax/crypto/CipherOutputStream",
                  "java/security/MessageDigest")

REGISTRY_INDICATORS = ("reg add", "regedit", "hkey_local_machine",
                       "hkey_current_user", "currentversion\\run")

SHELL_STRINGS = ("/bin/bash", "/bin/sh", "cmd.exe", "powershell",
                 "-c exec", "nc -e", "netcat")

OBFUSCATION_NAME_RE = re.compile(r"^[a-zA-Z]{1,2}(/\$\$\d+)?|\$\d+$")

# Bootstrap factory "chuẩn" của JVM: lambda + string concat (JEP 309).
# Bootstrap ngoài bộ này => code động do tác giả tự viết — đáng ngờ hơn.
STANDARD_BOOTSTRAPS = ("java.lang.invoke.LambdaMetafactory",
                       "java.lang.invoke.StringConcatFactory",
                       "java.lang.runtime.ObjectMethods")

# Sink classes cho call graph: đích cuối của chain exec/network.
SINK_CLASSES = ("java.lang.Runtime", "java.lang.ProcessBuilder",
                "java.net.Socket", "java.net.ServerSocket",
                "java.net.URL", "java.net.HttpURLConnection",
                "java.net.URLConnection", "java.nio.channels.SocketChannel")

# Depth tối đa khi BFS call graph — depth ngắn = liên kết trực tiếp, tin cậy hơn.
CALLGRAPH_MAX_DEPTH = 4

# Ngưỡng invokedynamic/class coi là "lạm dụng" (loader ẩn code thường dùng >= 20).
INVOKEDYNAMIC_ABUSE_THRESHOLD = 20


# Prefix KHÔNG phải class nội bộ của mod (JDK, Kotlin, Minecraft, Fabric API).
NON_INTERNAL_PREFIXES = ("java.", "javax.", "jdk.", "sun.", "com.sun.",
                         "org.w3c.", "org.xml.", "org.ietf.", "org.jcp.",
                         "kotlin.", "scala.", "net.minecraft.", "net.fabricmc.")


def _is_internal(cls: str) -> bool:
    """Class nội bộ của mod (không JDK / không Minecraft / không Fabric API)."""
    return bool(cls) and "." in cls and not cls.startswith(NON_INTERNAL_PREFIXES)


def _match_any(strings_or_refs: list[str], needles) -> list[str]:
    out = []
    for item in strings_or_refs:
        low = item.lower()
        for needle in needles:
            if needle in low:
                out.append(item)
                break
    return out


def _classes_having_method(infos: list[ClassInfo], method_needle: str) -> list[str]:
    return sorted({i.name for i in infos
                   if any(m == method_needle or m.endswith(method_needle)
                          for m in i.method_refs)})


def scan_mod(jar_path: Path, sha256: str = "") -> ScanReport:
    """Quét 1 jar mod: structure + bytecode heuristics."""
    report = ScanReport(
        file=jar_path.name, size=jar_path.stat().st_size, sha256=sha256)

    # 1. Cấu trúc jar
    try:
        struct_info = jar_structure(jar_path)
    except ClassFileError as e:
        report.verdict = VERDICT_SUSPICIOUS
        report.score = SUSPICIOUS_THRESHOLD
        report.findings.append(Finding(
            "JAR_CORRUPT", "medium", SUSPICIOUS_THRESHOLD,
            "Corrupt / not a real jar", str(e)))
        return report

    if struct_info["suspicious_files"]:
        report.findings.append(Finding(
            "EMBEDDED_EXECUTABLE", "critical", 70,
            "Native executable embedded in jar",
            f"jar chứa file thực thi: {struct_info['suspicious_files']}",
            struct_info["suspicious_files"][:3]))
    if struct_info["nested_jars"]:
        report.findings.append(Finding(
            "NESTED_JAR", "medium", 15,
            "Nested jar (self-extract / loader)",
            f"{len(struct_info['nested_jars'])} nested jar",
            struct_info["nested_jars"][:3]))

    # 2. Metadata (fabric.mod.json / mods.toml)
    fmj = struct_info.get("fabric_mod_json")
    if fmj:
        try:
            import json
            data = json.loads(fmj)
            report.mod_id = data.get("id")
            report.mod_name = data.get("name")
        except Exception:
            pass

    # 3. Decode bytecode
    try:
        infos = scan_jar_classes(jar_path)
    except ClassFileError:
        infos = []
    report.classes_scanned = len(infos)
    if infos:
        report.java_version = max(i.java_version for i in infos)

    all_strings: list[str] = []
    all_method_refs: list[str] = []
    all_class_refs: list[str] = []
    for i in infos:
        all_strings += i.strings
        all_method_refs += i.method_refs
        all_class_refs.append(i.name)
        all_class_refs += i.class_refs

    # 4. Rules
    _rule_download_exec(report, infos, all_strings, all_method_refs)
    _rule_reverse_shell(report, all_strings, all_method_refs)
    _rule_credential_theft(report, all_strings, all_method_refs)
    _rule_ransomware(report, infos, all_strings, all_method_refs)
    _rule_registry(report, all_strings)
    _rule_obfuscation(report, all_class_refs)
    _rule_network_beacon(report, all_strings, all_method_refs)
    _rule_invokedynamic(report, infos)
    _rule_callgraph_depth(report, infos)
    _rule_reflection_abuse(report, infos)

    # 5. Verdict
    report.score = sum(f.weight for f in report.findings)
    if report.score >= DANGEROUS_THRESHOLD:
        report.verdict = VERDICT_DANGEROUS
    elif report.score >= SUSPICIOUS_THRESHOLD:
        report.verdict = VERDICT_SUSPICIOUS
    else:
        report.verdict = VERDICT_SAFE
    return report


# ---- Rules ----

def _rule_download_exec(report, infos, strings, method_refs):
    """Tải file + exec cùng class = chain nguy hiểm nhất (FR3EM1UM style)."""
    download_classes = _classes_having_method(
        infos, "java.net.URL.openConnection") + \
        _classes_having_method(infos, "java.nio.file.Files.copy")
    exec_classes = _classes_having_method(infos, "java.lang.ProcessBuilder.start") + \
        _classes_having_method(infos, "java.lang.Runtime.exec")
    both = set(download_classes) & set(exec_classes)
    if both:
        report.findings.append(Finding(
            "DOWNLOAD_AND_EXECUTE", "critical", 80,
            "Downloads file AND executes it in same class",
            f"Class vừa tải file vừa thực thi: {sorted(both)[:3]}",
            sorted(both)[:3]))


def _rule_reverse_shell(report, strings, method_refs):
    hits = _match_any(strings, SHELL_STRINGS)
    net = [m for m in method_refs
           if any(n in m for n in ("java.net.Socket", "ServerSocket.accept",
                                   "SocketChannel.connect"))]
    if hits and net:
        report.findings.append(Finding(
            "REVERSE_SHELL_PATTERN", "critical", 75,
            "Shell execution combined with raw socket",
            f"shell strings: {hits[:2]} + network: {net[:2]}", hits[:3]))


def _rule_credential_theft(report, strings, method_refs):
    cred_hits = _match_any(strings, CREDENTIAL_PATHS)
    # Đọc credential paths trong string pool = intent truy cập secret store
    if cred_hits:
        report.findings.append(Finding(
            "CREDENTIAL_ACCESS", "high", 45,
            "References credential/token storage paths",
            f"{len(cred_hits)} string trỏ tới credential store",
            cred_hits[:3]))


def _rule_ransomware(report, infos, strings, method_refs):
    crypto = [m for m in method_refs
              if any(c in m for c in ("javax.crypto.Cipher.init",
                                      "CipherOutputStream"))]
    files_walk = _classes_having_method(infos, "java.nio.file.Files.walk")
    if crypto and files_walk:
        report.findings.append(Finding(
            "MASS_CRYPTO_FILE_IO", "high", 55,
            "Bulk file traversal combined with crypto (ransomware pattern)",
            f"Files.walk trong {files_walk[:2]} + Cipher", files_walk[:2]))


def _rule_registry(report, strings):
    hits = _match_any(strings, REGISTRY_INDICATORS)
    if hits:
        report.findings.append(Finding(
            "WINDOWS_REGISTRY", "medium", 25,
            "Windows registry manipulation strings",
            "Startup persistence hoặc chỉnh registry",
            hits[:3]))


def _rule_obfuscation(report, class_refs):
    """Obfuscation cực đoan: tỷ lệ class tên 1-2 ký tự cao bất thường."""
    if len(class_refs) < 20:
        return
    obfuscated = sum(1 for c in class_refs if OBFUSCATION_NAME_RE.match(c.split(".")[-1]))
    ratio = obfuscated / max(1, len(class_refs))
    if ratio > 0.7:
        report.findings.append(Finding(
            "HEAVY_OBFUSCATION", "medium", 20,
            f"Heavy obfuscation ({ratio:.0%} classes with tiny names)",
            "Mod bị obfuscate mạnh — khó audit, thường gặp ở cheat/malware",
            class_refs[:3]))


def _rule_invokedynamic(report, infos):
    """InvokeDynamic với custom bootstrap: ẩn logic động khỏi phân tích tĩnh.

    Lambda/record hợp lệ đều dùng LambdaMetafactory/StringConcatFactory.
    Bootstrap tự viết (hoặc đặt tên giả mạo java.* không tồn tại) = dấu hiệu
    che giấu: hàm thật sẽ chạy qua invokedynamic tại runtime, bytecode tĩnh
    không thấy callsite. Loader malware mới hay dùng kỹ thuật này.
    """
    total_indy = sum(i.invokedynamic_count for i in infos)
    if not total_indy:
        return

    # 1) Custom bootstrap: class bootstrap không thuộc bộ chuẩn JVM.
    custom = set()
    for i in infos:
        for bc in i.bootstrap_classes:
            if bc and not bc.startswith(STANDARD_BOOTSTRAPS):
                custom.add(bc)

    # 2) Giả mạo: bootstrap "java.*" nhưng không phải class thật của JDK
    #    (malware tự viết class tên java/lang/Xxx để qua lời sàng package).
    fake_jdk = {c for c in custom
                if (c.startswith("java.") or c.startswith("sun.") or c.startswith("jdk."))
                and not c.startswith(STANDARD_BOOTSTRAPS)}

    if custom:
        report.findings.append(Finding(
            "CUSTOM_INVOKEDYNAMIC_BOOTSTRAP", "high", 35,
            f"InvokeDynamic with custom bootstrap ({len(custom)} non-standard)",
            "Indy không dùng LambdaMetafactory/StringConcatFactory — logic động "
            "được ẩn qua bootstrap tự viết",
            sorted(custom | fake_jdk)[:3]))

    # 3) Khối lượng indy bất thường: packer/loader sinh indy hàng loạt.
    if total_indy >= INVOKEDYNAMIC_ABUSE_THRESHOLD:
        report.findings.append(Finding(
            "INVOKEDYNAMIC_ABUSE", "medium", 30,
            f"Abnormal invokedynamic density ({total_indy} callsites)",
            f"{total_indy} invokedynamic trên {len(infos)} class — pattern của "
            "loader sinh code động", []))


def _rule_callgraph_depth(report, infos):
    """BFS qua call graph nội bộ: tìm path mod-class -> sink nguy hiểm.

    Chỉ flag khi CÓ path thật (callees được lấy từ Code attribute), giảm
    false positive so với co-occurrence heuristic. Depth ngắn = chain trực
    tiếp, đáng tin; depth sâu hơn ngưỡng chỉ nghi vấn nhẹ.
    """
    # Graph giữ RAW callees (để soi sink java.*); chỉ TRAVERSE qua class nội bộ.
    graph: dict[str, set[str]] = {}
    for i in infos:
        if i.callees:
            graph.setdefault(i.name, set()).update(i.callees)
    if not graph:
        return

    # BFS đa nguồn: entry node (không class nào gọi tới) tới mọi sink.
    referenced = {c for callees in graph.values() for c in callees}
    queue = [(n, 0) for n in graph if n not in referenced]
    if not queue:                       # mọi node nằm trong cycle — quét từ tất cả
        queue = [(n, 0) for n in graph]
    best_depth: int | None = None
    hit_sinks: set[str] = set()
    visited: set[str] = set()
    qi = 0
    while qi < len(queue):
        node, depth = queue[qi]
        qi += 1
        if node in visited or depth > CALLGRAPH_MAX_DEPTH:
            continue
        visited.add(node)
        callees = graph.get(node, ())
        for sink in SINK_CLASSES:
            if any(c.startswith(sink) for c in callees):
                hit_sinks.add(sink)
                if best_depth is None or depth < best_depth:
                    best_depth = depth
        for nxt in callees:                            # chỉ đi tiếp qua class nội bộ
            if _is_internal(nxt) and nxt not in visited:
                queue.append((nxt, depth + 1))

    if best_depth is None:
        return

    # Nghiêm trọng theo depth: chain trực tiếp (0-1) ~ như download_exec,
    # sâu hơn giảm dần; quá depth max thì coi như không (đã skip ở BFS).
    weight = 65 if best_depth <= 1 else (55 if best_depth <= 2 else 40)
    severity = "critical" if best_depth <= 1 else "high"
    report.findings.append(Finding(
        "EXEC_NETWORK_CALLGRAPH", severity, weight,
        f"Call-graph path (depth {best_depth}) to exec/network sink",
        f"Path {best_depth} bước qua class nội bộ tới {sorted(hit_sinks)[:2]}",
        sorted(hit_sinks)[:3]))


def _rule_reflection_abuse(report, infos):
    """Reflection + MethodHandles kết hợp: bypass tĩnh (điểm nghi vấn nhẹ)."""
    refl = [i.name for i in infos if i.uses_reflection]
    mh = [i.name for i in infos if i.uses_method_handles]
    if refl and mh:
        report.findings.append(Finding(
            "REFLECTION_METHODHANDLES", "medium", 20,
            f"Reflection + MethodHandles combined ({len(set(refl))} classes)",
            "Class.forName/Method.invoke + MethodHandles.Lookup — pattern ẩn "
            "API call khỏi method_refs", refl[:3]))


def _rule_network_beacon(report, strings, method_refs):
    urls = _match_any(strings, ("http://", "https://"))
    urls = [u for u in urls if _URL_RE.match(u)]
    # Không phải bản thân malicious, nhưng ghi lại endpoint lạ
    known_ok = ("modrinth", "curseforge", "github", "maven", "minecraft",
                "fabricmc", "mojang", "parchment", "sonatype", "jitpack")
    weird = [u for u in urls if not any(k in u.lower() for k in known_ok)]
    if len(weird) >= 5:
        report.findings.append(Finding(
            "MANY_EXTERNAL_ENDPOINTS", "low", 10,
            f"{len(weird)} non-standard external endpoints",
            "Mod giao tiếp với nhiều domain ngoài ecosystem",
            weird[:3]))
