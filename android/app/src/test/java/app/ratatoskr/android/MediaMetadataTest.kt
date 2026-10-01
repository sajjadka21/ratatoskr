package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** Small extractor-shaped fixtures; parsing never contacts a media host or
 * initializes yt-dlp/FFmpeg. Robolectric provides Android's JSONObject. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
class MediaMetadataTest {
    private val instagram = "https://www.instagram.com/p/Fixture123/"

    @Test fun instagramPhotoSelectsTheLargestPublicImageCandidate() {
        val info = MediaMetadata.parse(instagram, """
            {
              "title": "A public photo", "extractor_key": "Instagram", "formats": [],
              "thumbnails": [
                {"url": "https://cdn.example.com/photo-small.jpg", "width": 150, "height": 150},
                {"url": "https://cdn.example.com/photo-large.jpg", "width": 1080, "height": 1350},
                {"url": "https://cdn.example.com/photo-medium.jpg", "width": 640, "height": 800}
              ]
            }
        """.trimIndent())

        assertEquals("A public photo", info.title)
        assertEquals(instagram, info.url)
        assertFalse(info.hasVideo)
        assertTrue(info.heights.isEmpty())
        assertFalse(info.truncated)
        assertEquals(1, info.items.size)
        assertEquals(1, info.items.single().index)
        assertEquals("photo", info.items.single().kind)
        assertEquals("https://cdn.example.com/photo-large.jpg", info.items.single().downloadUrl)
    }

    @Test fun mixedAlbumKeepsAllItemsInSourceOrderAndTheReal540pHeight() {
        val info = MediaMetadata.parse(instagram, """
            {
              "_type": "playlist", "title": "Mixed album", "entries": [
                {"title": "First photo", "formats": [],
                 "thumbnails": [{"url": "https://cdn.example.com/first.jpg", "width": 1080, "height": 1080}]},
                {"title": "Middle video", "thumbnail": "https://cdn.example.com/poster.jpg",
                 "formats": [
                   {"format_id": "540", "height": 540, "vcodec": "h264", "acodec": "aac"},
                   {"format_id": "540-copy", "height": 540, "vcodec": "h264", "acodec": "aac"},
                   {"format_id": "audio", "vcodec": "none", "acodec": "aac"}
                 ]},
                {"title": "Last photo", "formats": [],
                 "thumbnails": [{"url": "https://cdn.example.com/last.jpg", "width": 1080, "height": 1350}]}
              ]
            }
        """.trimIndent())

        assertEquals("Mixed album", info.title)
        assertEquals(listOf(1, 2, 3), info.items.map { it.index })
        assertEquals(listOf("photo", "video", "photo"), info.items.map { it.kind })
        assertEquals(listOf("First photo", "Middle video", "Last photo"), info.items.map { it.title })
        assertEquals(listOf(540), info.items[1].heights)
        assertEquals(listOf(540), info.heights)
        assertTrue(info.hasVideo)
        assertNull(info.items[1].downloadUrl)
        assertFalse(info.truncated)
    }

    @Test fun videoPosterIsNeverOfferedAsAnExtraPhotoDownload() {
        val info = MediaMetadata.parse(instagram, """
            {"title": "A reel", "thumbnail": "https://cdn.example.com/poster.jpg",
             "thumbnails": [{"url": "https://cdn.example.com/poster-large.jpg", "width": 2000, "height": 2000}],
             "formats": [{"height": 720, "vcodec": "h264", "acodec": "aac"}]}
        """.trimIndent())

        assertEquals(1, info.items.size)
        assertEquals("video", info.items.single().kind)
        assertEquals(listOf(720), info.items.single().heights)
        assertNull(info.items.single().downloadUrl)
        assertTrue(info.hasVideo)
    }

    @Test fun thumbnailOnlyMetadataFromAnotherSiteIsUnsupportedMedia() {
        val metadata = """
            {"title": "Preview, not an image download", "formats": [],
             "thumbnails": [{"url": "https://cdn.example.com/preview.jpg", "width": 1080, "height": 1080}]}
        """.trimIndent()
        assertThrows(RuntimeException::class.java) {
            MediaMetadata.parse("https://example.com/article", metadata)
        }
    }

    @Test fun instagramLookingHostnameDoesNotGrantPhotoFallbackToAnotherSite() {
        val metadata = """
            {"title": "Untrusted preview", "formats": [],
             "thumbnails": [{"url": "https://cdn.example.com/preview.jpg", "width": 1080, "height": 1080}]}
        """.trimIndent()
        assertThrows(RuntimeException::class.java) {
            MediaMetadata.parse("https://www.instagram.com.evil.example.com/p/Fixture123/", metadata)
        }
    }

    @Test fun albumLimitPreservesTheFirst50EntriesAndExposesTruncation() {
        val entries = (1..51).joinToString(",") { index ->
            """{"title":"Photo $index","formats":[],"thumbnails":[{"url":"https://cdn.example.com/photo-$index.jpg","width":1080,"height":1080}]}"""
        }
        val info = MediaMetadata.parse(instagram, """{"_type":"playlist","title":"Large album","entries":[$entries]}""")

        assertEquals(50, info.items.size)
        assertEquals((1..50).toList(), info.items.map { it.index })
        assertEquals("Photo 1", info.items.first().title)
        assertEquals("Photo 50", info.items.last().title)
        assertTrue(info.truncated)
    }

    @Test fun exactly50EntriesAreNotReportedAsTruncated() {
        val entries = (1..50).joinToString(",") { index ->
            """{"title":"Photo $index","formats":[],"thumbnails":[{"url":"https://cdn.example.com/photo-$index.jpg","width":1080,"height":1080}]}"""
        }
        val info = MediaMetadata.parse(instagram, """{"_type":"playlist","title":"Album","entries":[$entries]}""")
        assertEquals(50, info.items.size)
        assertFalse(info.truncated)
    }

    @Test fun audioOnlyFormatsAreAudioRatherThanPhotosOrVideo() {
        val info = MediaMetadata.parse("https://example.com/music", """
            {"title": "An audio track", "thumbnail": "https://cdn.example.com/cover.jpg",
             "formats": [{"format_id": "m4a", "ext": "m4a", "vcodec": "none", "acodec": "aac"}]}
        """.trimIndent())

        assertEquals(1, info.items.size)
        assertEquals("audio", info.items.single().kind)
        assertFalse(info.hasVideo)
        assertTrue(info.heights.isEmpty())
        assertTrue(info.items.single().heights.isEmpty())
        assertNull(info.items.single().downloadUrl)
    }

    @Test fun photoCandidateSelectionFiltersPrivateAndNonHttpDestinations() {
        listOf(
            "file:///tmp/photo.jpg", "ftp://example.com/photo.jpg", "http://127.0.0.1/photo.jpg",
            "http://192.168.1.2/photo.jpg", "http://169.254.169.254/photo.jpg", "http://[::1]/photo.jpg",
        ).forEach { unsafe ->
            val metadata = """{"title":"Unsafe image","formats":[],"thumbnails":[{"url":"$unsafe","width":4096,"height":4096}]}"""
            assertThrows("unsafe candidate $unsafe", RuntimeException::class.java) {
                MediaMetadata.parse(instagram, metadata)
            }
        }
    }

    @Test fun unsafeLargestCandidateCannotDisplaceASafeSmallerPhoto() {
        val info = MediaMetadata.parse(instagram, """
            {"title": "Safe image", "formats": [], "thumbnails": [
              {"url": "http://10.0.0.1/private.jpg", "width": 4096, "height": 4096},
              {"url": "https://cdn.example.com/public.jpg", "width": 640, "height": 480}
            ]}
        """.trimIndent())
        assertEquals("photo", info.items.single().kind)
        assertEquals("https://cdn.example.com/public.jpg", info.items.single().downloadUrl)
    }
}
