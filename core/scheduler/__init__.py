"""core/scheduler — background task governor (spec 3.0 mục 30).

Khi Minecraft đang chạy, các việc nền (telemetry, scan, cache cleanup...)
phải nhường CPU cho game: giảm tần số, dừng việc nặng, không thêm lỗi mới
vào máy đang chơi game.
"""
