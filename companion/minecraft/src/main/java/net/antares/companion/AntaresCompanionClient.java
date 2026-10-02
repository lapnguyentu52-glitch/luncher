package net.antares.companion;

import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.rendering.v1.HudRenderCallback;

import java.nio.file.Path;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;

/**
 * Antares Companion — client entrypoint (spec 3.0 mục 12).
 *
 * Flow:
 *   1. Lúc client khởi tạo: đọc <gameDir>/companion.json (launcher ghi lúc launch).
 *   2. Vào world -> gửi runtime.hello (sessionId = gameDir hash).
 *   3. Mỗi nửa giây (2Hz): gom FPS/frametime trung bình -> runtime.performance.
 *   4. Rời world / tắt game -> runtime.bye.
 *
 * FPS lấy từ renderer (frameTimeNs qua Fabric HudRenderCallback) — không cần
 * Mixin vào Minecraft class nào, giảm rủi ro break giữa các bản game.
 */
public final class AntaresCompanionClient implements ClientModInitializer {
    private static final long SAMPLE_INTERVAL_MS = 500;
    private static final String[] PACKET_TYPES = {
            "runtime.hello", "runtime.bye", "runtime.performance",
            "runtime.game_state", "runtime.chat", "runtime.error",
    };

    private final ScheduledExecutorService scheduler =
            Executors.newSingleThreadScheduledExecutor(r -> {
                Thread t = new Thread(r, "antares-companion");
                t.setDaemon(true);
                return t;
            });

    private IpcClient ipc;
    private String sessionId;
    private boolean connected;

    // frametime sampling
    private long frameCount;
    private long frameNsTotal;
    private long lastFrameNs;

    @Override
    public void onInitializeClient() {
        Path gameDir = findRunDir();
        String[] pairing = gameDir != null ? PairingReader.read(gameDir) : null;
        if (pairing == null) {
            // Launcher không chạy / không có pairing — mod im lặng hoàn toàn.
            return;
        }
        ipc = new IpcClient(pairing[0], pairing[1]);
        sessionId = Integer.toHexString(gameDir.toAbsolutePath().hashCode());

        ClientLifecycleEvents.CLIENT_STOPPING.register(client -> sendBye());

        // Handshake + sampling loop 2Hz (thread riêng — không đụng render loop)
        ClientTickEvents.END_CLIENT_TICK.register(client -> {
            if (!connected && client.world != null && client.player != null) {
                connect();
            } else if (connected && client.world == null) {
                sendBye();
            }
        });

        // Frametime đo ở render callback (nanosecond chính xác)
        HudRenderCallback.EVENT.register((drawContext, tickDelta) -> {
            long now = System.nanoTime();
            if (lastFrameNs > 0) {
                long delta = now - lastFrameNs;
                // Bỏ hitch/pause (menu, load screen) — delta > 1s không phải frame thật
                if (delta > 0 && delta < 1_000_000_000L) {
                    frameNsTotal += delta;
                    frameCount++;
                }
            }
            lastFrameNs = now;
        });
    }

    private void connect() {
        connected = true;
        frameCount = 0;
        frameNsTotal = 0;
        lastFrameNs = 0;
        scheduler.scheduleAtFixedRate(this::tickSampling, SAMPLE_INTERVAL_MS,
                SAMPLE_INTERVAL_MS, TimeUnit.MILLISECONDS);
        sendAsync("runtime.hello", new IpcClient.JsonObject()
                .put("sessionId", sessionId)
                .put("instanceId", sessionId));
    }

    private void tickSampling() {
        try {
            if (!connected || frameCount == 0) return;
            double avgFrameMs = (frameNsTotal / 1e6) / Math.max(1, frameCount);
            double fps = avgFrameMs > 0 ? 1000.0 / avgFrameMs : 0;
            // 1% low ước lượng: 80% trung bình (companion thật sẽ giữ histogram)
            double low1 = fps * 0.8;
            sendAsync("runtime.performance", new IpcClient.JsonObject()
                    .put("sessionId", sessionId)
                    .put("fps", round(fps))
                    .put("frameMs", round(avgFrameMs))
                    .put("low1", round(low1)));
            frameCount = 0;
            frameNsTotal = 0;
        } catch (Throwable ignored) {
            // không bao giờ để scheduler chết
        }
    }

    private void sendBye() {
        if (!connected) return;
        connected = false;
        sendAsync("runtime.bye", new IpcClient.JsonObject().put("sessionId", sessionId));
    }

    private void sendAsync(String type, IpcClient.JsonObject payload) {
        double ts = System.currentTimeMillis() / 1000.0;
        scheduler.submit(() -> ipc.send(type, payload, ts));
    }

    private static double round(double v) {
        return Math.round(v * 10.0) / 10.0;
    }

    /** Game dir: cwd của client (launcher spawn với cwd=game dir). */
    private static Path findRunDir() {
        return Path.of("").toAbsolutePath();
    }

    /** Expose cho test/debug — loại packet hợp lệ theo launcher. */
    static String[] packetTypes() {
        return PACKET_TYPES.clone();
    }
}
