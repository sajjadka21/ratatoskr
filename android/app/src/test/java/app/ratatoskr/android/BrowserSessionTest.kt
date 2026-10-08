package app.ratatoskr.android

import org.junit.Assert.*
import org.junit.Test

class BrowserSessionTest {
    @Test fun sessionIsBoundToTheHttpsOriginAndNeverPrinted() {
        val session = BrowserSession.create("https://www.pornhub.com/view_video.php", "auth=secret; locale=en")!!
        assertEquals("auth=secret; Secure; locale=en; Secure", session.secureCookieHeaderFor("https://www.pornhub.com/other"))
        assertNull(session.secureCookieHeaderFor("http://www.pornhub.com/other"))
        assertNull(session.secureCookieHeaderFor("https://cdn.pornhub.com/video"))
        assertNull(session.secureCookieHeaderFor("https://pornhub.com/video"))
        assertNull(session.secureCookieHeaderFor("https://www.pornhub.com:8443/video"))
        assertEquals(mapOf("Cookie" to "auth=secret; locale=en"), session.directHeadersFor("https://www.pornhub.com/file.mp4"))
        assertTrue(session.directHeadersFor("https://cdn.pornhub.com/file.mp4").isEmpty())
        assertTrue(session.directHeadersFor("http://www.pornhub.com/file.mp4").isEmpty())
        assertFalse(session.toString().contains("secret"))
    }

    @Test fun rejectsInsecureEmptyOversizedAndHeaderInjectionSessions() {
        assertNull(BrowserSession.create("http://example.com/video", "auth=x"))
        assertNull(BrowserSession.create("https://example.com/video", null))
        assertNull(BrowserSession.create("https://example.com/video", " \n "))
        assertNull(BrowserSession.create("https://example.com/video", "auth=x\r\nAuthorization: stolen"))
        assertNull(BrowserSession.create("https://example.com/video", "x=" + "a".repeat(16 * 1024)))
    }

    @Test fun handoffIsOneTimeAndTaskVaultCanForgetSecrets() {
        val session = BrowserSession.create("https://example.com/video", "auth=secret")!!
        val token = BrowserSessionHandoff.stage(session)!!
        assertNotNull(BrowserSessionHandoff.take(token))
        assertNull(BrowserSessionHandoff.take(token))

        BrowserSessionVault.attach("task-1", session)
        assertNotNull(BrowserSessionVault.forTask("task-1"))
        BrowserSessionVault.forget("task-1")
        assertNull(BrowserSessionVault.forTask("task-1"))
    }
}

