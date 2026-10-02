package net.antares.companion;

import java.io.IOException;
import java.io.OutputStream;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.time.Duration;

/**
 * Gửi packet JSON tới launcher IPC (loopback HTTP POST /packet).
 *
 * Packet model (spec mục 13):
 *   {"version":1, "type":"runtime.performance", "timestamp":..., "payload":{...}}
 * Auth: header X-Antares-Token (đọc từ companion.json do launcher ghi lúc launch).
 *
 * Thiết kế an toàn cho game: fire-and-forget, timeout ngắn, KHÔNG block
 * client thread — gửi từ thread scheduler riêng; lỗi bỏ qua (launcher tắt
 * hoặc chưa chạy thì telemetry chỉ bị drop, game không ảnh hưởng).
 */
public final class IpcClient {
    private static final Duration TIMEOUT = Duration.ofMillis(800);
    private static final int MAX_BODY = 60_000;

    private final HttpClient http = HttpClient.newBuilder()
            .connectTimeout(TIMEOUT)
            .build();
    private final String endpoint;
    private final String token;

    public IpcClient(String endpoint, String token) {
        this.endpoint = endpoint;
        this.token = token;
    }

    /** Gửi packet; trả false nếu drop (lỗi network/serialize) — không raise. */
    public boolean send(String type, JsonObject payload, double timestamp) {
        if (endpoint == null || token == null) return false;
        String body = "{\"version\":1,\"type\":\"" + type + "\",\"timestamp\":" + timestamp
                + ",\"payload\":" + payload.toString() + "}";
        if (body.length() > MAX_BODY) return false;
        try {
            HttpRequest req = HttpRequest.newBuilder(URI.create(endpoint))
                    .timeout(TIMEOUT)
                    .header("Content-Type", "application/json")
                    .header("X-Antares-Token", token)
                    .POST(HttpRequest.BodyPublishers.ofString(body, StandardCharsets.UTF_8))
                    .build();
            HttpResponse<String> resp = http.send(req, HttpResponse.BodyHandlers.ofString());
            return resp.statusCode() == 200;
        } catch (IOException | InterruptedException e) {
            return false;  // launcher không chạy / tắt rồi — im lặng
        }
    }

    /** JsonObject tối thiểu — tránh thêm dependency gson ngoài loader. */
    public static final class JsonObject {
        private final StringBuilder sb = new StringBuilder("{");
        private boolean first = true;

        public JsonObject put(String key, String value) {
            sep().append("\"").append(esc(key)).append("\":\"").append(esc(value)).append("\"");
            return this;
        }

        public JsonObject put(String key, Number value) {
            sep().append("\"").append(esc(key)).append("\":").append(value);
            return this;
        }

        public JsonObject put(String key, boolean value) {
            sep().append("\"").append(esc(key)).append("\":").append(value);
            return this;
        }

        private StringBuilder sep() {
            if (!first) sb.append(",");
            first = false;
            return sb;
        }

        @Override
        public String toString() {
            return sb + "}";
        }

        private static String esc(String s) {
            return s.replace("\\", "\\\\").replace("\"", "\\\"");
        }
    }
}
