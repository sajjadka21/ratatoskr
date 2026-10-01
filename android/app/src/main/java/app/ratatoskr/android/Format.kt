package app.ratatoskr.android

import java.util.Locale

/** Human-readable sizes, speeds and times; plain functions so they run as unit tests. */
object Format {
    fun bytes(value: Long): String {
        if (value < 0) return "?"
        val units = listOf("B", "KB", "MB", "GB", "TB")
        var size = value.toDouble()
        var unit = 0
        while (size >= 1024 && unit < units.lastIndex) { size /= 1024; unit++ }
        return if (unit == 0) "$value B" else String.format(Locale.US, if (size >= 100) "%.0f %s" else "%.1f %s", size, units[unit])
    }
    fun speed(bytesPerSecond: Long) = if (bytesPerSecond <= 0) "" else bytes(bytesPerSecond) + "/s"
    /** Remaining time as m:ss or h:mm:ss; empty when it cannot be known. */
    fun eta(remainingBytes: Long, bytesPerSecond: Long): String {
        if (remainingBytes <= 0 || bytesPerSecond <= 0) return ""
        val seconds = (remainingBytes / bytesPerSecond).coerceAtMost(99L * 3600)
        val h = seconds / 3600; val m = seconds % 3600 / 60; val s = seconds % 60
        return if (h > 0) String.format(Locale.US, "%d:%02d:%02d", h, m, s) else String.format(Locale.US, "%d:%02d", m, s)
    }
}

/** Smooths speed over a few samples so the number does not jump on every refresh. */
class SpeedMeter {
    private val last = HashMap<String, Pair<Long, Long>>()
    private val speeds = HashMap<String, Long>()
    fun sample(id: String, bytes: Long, nowMillis: Long): Long {
        val before = last.put(id, bytes to nowMillis)
        if (before == null || nowMillis <= before.second || bytes < before.first) { speeds.remove(id); return 0 }
        val instant = (bytes - before.first) * 1000 / (nowMillis - before.second)
        val smooth = speeds[id]?.let { (it * 6 + instant * 4) / 10 } ?: instant
        speeds[id] = smooth
        return smooth
    }
    fun forget(id: String) { last.remove(id); speeds.remove(id) }
}
