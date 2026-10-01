package app.ratatoskr.android

import java.util.Calendar

/** "Only download between 02:00 and 07:00" (a window that may pass midnight, e.g. 23:00–06:00). */
data class DownloadWindow(val enabled: Boolean = false, val startMinute: Int = 2 * 60, val endMinute: Int = 7 * 60)

/** Time rules for scheduled downloads, as plain functions so they run as unit tests. */
object Schedule {
    private const val DAY_MINUTES = 24 * 60

    /** Whether [minuteOfDay] (0–1439) is inside the window. A disabled window, or one whose ends are equal, is always open. */
    fun isOpen(window: DownloadWindow, minuteOfDay: Int): Boolean {
        if (!window.enabled || window.startMinute == window.endMinute) return true
        val m = minuteOfDay.mod(DAY_MINUTES)
        return if (window.startMinute < window.endMinute) m >= window.startMinute && m < window.endMinute
        else m >= window.startMinute || m < window.endMinute
    }

    /** A task with no start time, or whose start time has come. */
    fun isDue(startAt: Long, now: Long) = startAt <= now

    /** Milliseconds until the window next opens; 0 when it is open already. */
    fun millisUntilOpen(window: DownloadWindow, minuteOfDay: Int, secondOfMinute: Int = 0, millis: Int = 0): Long {
        if (isOpen(window, minuteOfDay)) return 0
        val minutes = (window.startMinute - minuteOfDay).mod(DAY_MINUTES)
        return minutes * 60_000L - secondOfMinute * 1000L - millis
    }

    /** How long to sleep before something waiting can run, or null when there is nothing to wait for.
     * [startTimes] are the start times of the waiting tasks (0 = none). */
    fun nextWake(startTimes: List<Long>, window: DownloadWindow, now: Long, minuteOfDay: Int, secondOfMinute: Int = 0, millis: Int = 0): Long? {
        if (startTimes.isEmpty()) return null
        val windowWait = millisUntilOpen(window, minuteOfDay, secondOfMinute, millis)
        val waits = startTimes.map { start ->
            val untilStart = if (isDue(start, now)) 0L else start - now
            // Whichever comes later decides when this task can run: its own time or the window opening.
            maxOf(untilStart, if (untilStart > windowWait) millisUntilOpenAt(window, minuteOfDay, secondOfMinute, millis, untilStart) else windowWait)
        }
        return waits.filter { it > 0 }.minOrNull()
    }

    /** Wait until the window is open at the moment [after] milliseconds from now. */
    private fun millisUntilOpenAt(window: DownloadWindow, minuteOfDay: Int, secondOfMinute: Int, millis: Int, after: Long): Long {
        val totalMillis = minuteOfDay * 60_000L + secondOfMinute * 1000L + millis + after
        val atMinute = ((totalMillis / 60_000L) % DAY_MINUTES).toInt()
        val extra = millisUntilOpen(window, atMinute, ((totalMillis / 1000L) % 60).toInt(), (totalMillis % 1000).toInt())
        return after + extra
    }

    fun now(window: DownloadWindow, calendar: Calendar = Calendar.getInstance()): Boolean =
        isOpen(window, calendar.get(Calendar.HOUR_OF_DAY) * 60 + calendar.get(Calendar.MINUTE))

    fun wake(startTimes: List<Long>, window: DownloadWindow, nowMillis: Long = System.currentTimeMillis(), calendar: Calendar = Calendar.getInstance()): Long? =
        nextWake(startTimes, window, nowMillis, calendar.get(Calendar.HOUR_OF_DAY) * 60 + calendar.get(Calendar.MINUTE),
            calendar.get(Calendar.SECOND), calendar.get(Calendar.MILLISECOND))

    fun clock(minute: Int) = "%02d:%02d".format(minute / 60, minute % 60)
}
