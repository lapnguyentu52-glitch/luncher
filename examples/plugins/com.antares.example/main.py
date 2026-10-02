

def setup(ctx) -> None:
    # 1) Command palette command (cần quyền 'ui')
    ctx.register_command("hello", lambda: {"message": "Hello từ example plugin!"})

    # 2) Notification chào mừng lúc plugin bật (cần 'ui')
    ctx.notify("Example plugin đã bật", "Gõ 'hello' trong command palette.", "INFO")

    # 3) Nhận telemetry từ companion mod (cần 'minecraft.telemetry')
    ctx.last_fps = {"fps": None}

    def on_perf(env) -> None:
        ctx.last_fps = {"fps": env.payload.get("fps"),
                        "frameMs": env.payload.get("frameMs")}

    ctx.on_minecraft_telemetry(on_perf)

    # 4) State đọc được qua command — demo vòng đời dữ liệu trong plugin
    ctx.register_command("fps", lambda: dict(ctx.last_fps))


def teardown(ctx) -> None:
    ctx.log("example plugin unloaded")
