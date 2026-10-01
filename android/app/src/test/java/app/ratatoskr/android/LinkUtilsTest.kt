package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LinkUtilsTest {
    @Test fun urlIsFoundWithoutTrailingPunctuation() {
        assertEquals("https://youtu.be/abc123", LinkUtils.extractUrl("look: https://youtu.be/abc123, nice"))
        assertEquals("https://example.com/a?b=1", LinkUtils.extractUrl("(https://example.com/a?b=1)"))
        assertNull(LinkUtils.extractUrl("no link"))
        assertNull(LinkUtils.extractUrl(null))
    }

    @Test fun localAddressesAreRefused() {
        listOf(
            "http://localhost/x", "http://127.0.0.1/x", "http://10.0.0.5/x", "http://192.168.1.1/",
            "http://169.254.169.254/latest", "http://[::1]/", "http://printer.local/",
            "ftp://example.com/f", "file:///etc/passwd", "http://intranet/",
        ).forEach { assertFalse(it, LinkUtils.isPublicHttpUrl(it)) }
        assertTrue(LinkUtils.isPublicHttpUrl("https://www.instagram.com/reel/abc/"))
        assertTrue(LinkUtils.isPublicHttpUrl("http://93.184.216.34/file.zip"))
    }

    @Test fun onlyHeightsTheVideoHasAreOffered() {
        assertEquals(listOf(1080, 720, 360), LinkUtils.offeredHeights(listOf(360, 720, 1080)))
        assertEquals(listOf(2160, 1440, 1080, 720), LinkUtils.offeredHeights(listOf(1080, 2160, 1440, 720, 480, 360)))
        assertEquals(listOf(540), LinkUtils.offeredHeights(listOf(540)))
        assertEquals(emptyList<Int>(), LinkUtils.offeredHeights(listOf(null)))
    }

    @Test fun formatAndNames() {
        assertTrue(LinkUtils.videoFormat(720).contains("height<=720"))
        assertEquals("bv*+ba/b", LinkUtils.videoFormat(null))
        assertEquals("a_b_c__.mp4", LinkUtils.safeFileName("a/b:c*?.mp4"))
        assertEquals("file", LinkUtils.safeFileName("..."))
    }
}
