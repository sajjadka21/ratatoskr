package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Test

class FormatTest {
    @Test fun sizes() {
        assertEquals("?", Format.bytes(-1))
        assertEquals("512 B", Format.bytes(512))
        assertEquals("1.5 KB", Format.bytes(1536))
        assertEquals("5.0 MB", Format.bytes(5L * 1024 * 1024))
        assertEquals("120 MB", Format.bytes(120L * 1024 * 1024))
        assertEquals("2.0 GB", Format.bytes(2L * 1024 * 1024 * 1024))
    }
    @Test fun speedAndEta() {
        assertEquals("", Format.speed(0))
        assertEquals("1.0 MB/s", Format.speed(1024 * 1024))
        assertEquals("", Format.eta(100, 0))
        assertEquals("1:40", Format.eta(1000, 10))
        assertEquals("1:01:40", Format.eta(37000, 10))
    }
    @Test fun meterSmoothsAndResets() {
        val meter = SpeedMeter()
        assertEquals(0, meter.sample("a", 0, 1000))
        assertEquals(1000, meter.sample("a", 1000, 2000))
        assertEquals(1000, meter.sample("a", 2000, 3000))
        assertEquals(0, meter.sample("a", 10, 4000))      // restarted from zero
        meter.forget("a")
        assertEquals(0, meter.sample("a", 500, 5000))
    }
}
