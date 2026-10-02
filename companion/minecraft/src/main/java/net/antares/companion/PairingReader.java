package net.antares.companion;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Đọc pairing do launcher ghi: <game dir>/companion.json
 *   {"version":1, "endpoint":"http://127.0.0.1:PORT/packet", "token":"...", ...}
 *
 * Parse JSON bằng regex có chủ đích: file do launcher sinh, schema cố định,
 * tránh thêm dependency JSON cho mod. Endpoint/token validate trước khi dùng
 * (chỉ nhận 127.0.0.1 — chặn file lỗi dẫn tới request ngoài loopback).
 */
public final class PairingReader {
    private static final Pattern ENDPOINT = Pattern.compile("\"endpoint\"\\s*:\\s*\"([^\"]+)\"");
    private static final Pattern TOKEN = Pattern.compile("\"token\"\\s*:\\s*\"([^\"]+)\"");

    private PairingReader() {}

    public static String[] read(Path gameDir) {
        Path file = gameDir.resolve("companion.json");
        if (!Files.isRegularFile(file)) return null;
        String json;
        try {
            json = Files.readString(file);
        } catch (IOException e) {
            return null;
        }
        Matcher em = ENDPOINT.matcher(json);
        Matcher tm = TOKEN.matcher(json);
        if (!em.find() || !tm.find()) return null;
        String endpoint = em.group(1);
        // An toàn: chỉ loopback (spec mục 13 — không gọi ra ngoài)
        if (!endpoint.startsWith("http://127.0.0.1:")) return null;
        return new String[]{endpoint, tm.group(1)};
    }
}
