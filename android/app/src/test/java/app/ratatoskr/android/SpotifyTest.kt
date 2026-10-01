package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SpotifyTest {
    @Test fun recognisesTrackLinksOnly() {
        assertTrue(Spotify.isTrackUrl("https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT"))
        assertTrue(Spotify.isTrackUrl("https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT?si=abc"))
        assertTrue(Spotify.isTrackUrl("https://open.spotify.com/intl-fa/track/4cOdK2wGLETKBW3PvgPWqT"))
        assertFalse(Spotify.isTrackUrl("https://open.spotify.com/album/4cOdK2wGLETKBW3PvgPWqT"))
        assertFalse(Spotify.isTrackUrl("https://evil.example/track/4cOdK2wGLETKBW3PvgPWqT"))
        assertFalse(Spotify.isTrackUrl("https://open.spotify.com.evil.example/track/4cOdK2wGLETKBW3PvgPWqT"))
        assertFalse(Spotify.isTrackUrl("not a url"))
    }

    @Test fun readsTitleAndArtistFromOpenGraphTags() {
        val html = """<html><head><meta property="og:title" content="Never Gonna Give You Up &amp; More"/>
            <meta property="og:description" content="Rick Astley · Whenever You Need Somebody · Song · 1987"/></head></html>"""
        val track = Spotify.parsePage(html)!!
        assertEquals("Never Gonna Give You Up & More", track.title)
        assertEquals("Rick Astley", track.artist)
        assertEquals("Rick Astley - Never Gonna Give You Up & More", track.query)
    }

    @Test fun worksWithReversedAttributesAndNoArtist() {
        val track = Spotify.parsePage("""<meta content='Solo Title' property='og:title'>""")!!
        assertEquals("Solo Title", track.title); assertEquals("", track.artist); assertEquals("Solo Title", track.query)
    }

    @Test fun refusesPagesWithoutATitle() {
        assertNull(Spotify.parsePage("<html></html>"))
        assertNull(Spotify.parsePage("""<meta property="og:title" content="  ">"""))
    }

    @Test fun controlCharactersNeverReachTheSearch() {
        val track = Spotify.parsePage("""<meta property="og:title" content="A&#10;B">""")!!
        assertEquals("A B", track.title)
    }
}
