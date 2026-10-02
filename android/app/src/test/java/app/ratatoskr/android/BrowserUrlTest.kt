package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BrowserUrlTest {
    @Test fun whatIsTypedBecomesAnAddressOrASearch() {
        assertEquals("", BrowserUrl.normalize("   "))
        assertEquals("https://example.com/a", BrowserUrl.normalize(" https://example.com/a "))
        assertEquals("http://example.com", BrowserUrl.normalize("http://example.com"))
        assertEquals("https://example.com", BrowserUrl.normalize("example.com"))
        assertEquals("https://sub.example.co.uk/path?q=1", BrowserUrl.normalize("sub.example.co.uk/path?q=1"))
        assertEquals("https://example.com:8080/x", BrowserUrl.normalize("example.com:8080/x"))
        assertEquals("https://duckduckgo.com/?q=best+download+manager", BrowserUrl.normalize("best download manager"))
        assertEquals("https://duckduckgo.com/?q=localhost", BrowserUrl.normalize("localhost"))
    }

    @Test fun mediaAddressesAreNoticedAndStreamsNeedTheVideoEngine() {
        assertTrue(BrowserUrl.isMedia("https://cdn.example.com/v/clip.mp4?token=1"))
        assertTrue(BrowserUrl.isMedia("https://cdn.example.com/a.MP3"))
        assertTrue(BrowserUrl.isMedia("https://cdn.example.com/live/index.m3u8"))
        assertFalse(BrowserUrl.isMedia("https://example.com/page.html"))
        assertFalse(BrowserUrl.isMedia("https://example.com/"))
        assertTrue(BrowserUrl.isStream("https://cdn.example.com/live/index.m3u8"))
        assertFalse(BrowserUrl.isStream("https://cdn.example.com/a.mp4"))
    }
}
