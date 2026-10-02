package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ScheduleTest {
    private val night = DownloadWindow(true, 2 * 60, 7 * 60)
    private val overMidnight = DownloadWindow(true, 23 * 60, 6 * 60)

    @Test fun aDisabledWindowIsAlwaysOpen() {
        assertTrue(Schedule.isOpen(DownloadWindow(false, 2 * 60, 7 * 60), 12 * 60))
        assertTrue(Schedule.isOpen(DownloadWindow(true, 600, 600), 12 * 60))
    }

    @Test fun aSameDayWindowIncludesItsStartButNotItsEnd() {
        assertFalse(Schedule.isOpen(night, 119))
        assertTrue(Schedule.isOpen(night, 120))
        assertTrue(Schedule.isOpen(night, 419))
        assertFalse(Schedule.isOpen(night, 420))
    }

    @Test fun aWindowMayRunPastMidnight() {
        assertTrue(Schedule.isOpen(overMidnight, 23 * 60))
        assertTrue(Schedule.isOpen(overMidnight, 0))
        assertTrue(Schedule.isOpen(overMidnight, 5 * 60 + 59))
        assertFalse(Schedule.isOpen(overMidnight, 6 * 60))
        assertFalse(Schedule.isOpen(overMidnight, 12 * 60))
    }

    @Test fun waitsUntilTheWindowOpens() {
        assertEquals(0, Schedule.millisUntilOpen(night, 3 * 60))
        assertEquals(2 * 3_600_000L, Schedule.millisUntilOpen(night, 0))
        assertEquals(19 * 3_600_000L, Schedule.millisUntilOpen(night, 7 * 60))
        assertEquals(2 * 3_600_000L - 30_000L, Schedule.millisUntilOpen(night, 0, 30))
    }

    @Test fun dueMeansItsTimeHasCome() {
        assertTrue(Schedule.isDue(0, 1000))
        assertTrue(Schedule.isDue(1000, 1000))
        assertFalse(Schedule.isDue(1001, 1000))
    }

    @Test fun nextWakeIsTheEarliestMomentSomethingCanRun() {
        val open = DownloadWindow()
        assertNull(Schedule.nextWake(emptyList(), open, 0, 0))
        assertNull(Schedule.nextWake(listOf(0L), open, 1000, 12 * 60))                 // runnable now: no timer
        assertEquals(5000L, Schedule.nextWake(listOf(6000L, 9000L), open, 1000, 12 * 60))
        // outside the window: wait for it to open
        assertEquals(2 * 3_600_000L, Schedule.nextWake(listOf(0L), night, 1000, 0))
        // a start time later than the opening decides
        val start = 1000L + 3 * 3_600_000L
        assertEquals(3 * 3_600_000L, Schedule.nextWake(listOf(start), night, 1000, 0))
        // a start time that falls outside the window waits for the next opening after it
        val late = 1000L + 8 * 3_600_000L   // 08:00 when it is 00:00 now → next opening is 02:00 the day after
        assertEquals(26 * 3_600_000L, Schedule.nextWake(listOf(late), night, 1000, 0))
    }

    @Test fun clockText() {
        assertEquals("02:05", Schedule.clock(125))
        assertEquals("23:00", Schedule.clock(23 * 60))
    }
}

class AutoRetryTest {
    @Test fun transientFailuresRetryWithGrowingDelaysThenGiveUp() {
        assertEquals(10_000L, AutoRetry.delayMillis("network", 1))
        assertEquals(30_000L, AutoRetry.delayMillis("rate_limited", 2))
        assertEquals(90_000L, AutoRetry.delayMillis("network", 3))
        assertNull(AutoRetry.delayMillis("network", 4))
        assertNull(AutoRetry.delayMillis("network", 0))
    }

    @Test fun permanentFailuresAreNotRetried() {
        for (code in listOf("no_space", "auth_required", "not_found", "unsupported_media", "invalid_range", "download_failed")) assertNull(code, AutoRetry.delayMillis(code, 1))
    }
}
