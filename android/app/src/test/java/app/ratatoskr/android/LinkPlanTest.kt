package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Test

class LinkPlanTest {
    @Test fun mediaSitesAndFilesAreTold() {
        assertEquals(LinkKind.MEDIA, LinkPlan.classify("https://www.youtube.com/watch?v=abc"))
        assertEquals(LinkKind.MEDIA, LinkPlan.classify("https://youtu.be/abc"))
        assertEquals(LinkKind.MEDIA, LinkPlan.classify("https://m.instagram.com/reel/abc/"))
        assertEquals(LinkKind.MEDIA, LinkPlan.classify("https://www.pornhub.com/view_video.php?viewkey=6abbc213c20ff"))
        assertEquals(LinkKind.MEDIA, LinkPlan.classify("https://www.pinterest.com/pin/123456789/"))
        assertEquals(LinkKind.MEDIA, LinkPlan.classify("https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT"))
        assertEquals(LinkKind.FILE, LinkPlan.classify("https://cdn.example.org/files/Setup.EXE?token=1"))
        assertEquals(LinkKind.FILE, LinkPlan.classify("https://example.org/a/book.pdf"))
        assertEquals(LinkKind.FILE, LinkPlan.classify("https://example.org/watch/123"))
    }
    @Test fun detectsMislabeledWebPagesBeforeSavingThem() {
        assertEquals(true, LinkPlan.isHtmlResponse("text/html; charset=utf-8", byteArrayOf()))
        assertEquals(true, LinkPlan.isHtmlResponse("application/octet-stream", " <!doctype html><html>".toByteArray()))
        assertEquals(false, LinkPlan.isHtmlResponse("application/octet-stream", byteArrayOf(0, 1, 2, 3)))
        org.junit.Assert.assertTrue(LinkPlan.mayTryMedia("https://example.org/share/abc", "not_a_file"))
        org.junit.Assert.assertFalse(LinkPlan.mayTryMedia("https://example.org/file.mp4", "not_a_file"))
    }
    @Test fun lookalikeHostsAreNotMedia() {
        assertEquals(LinkKind.FILE, LinkPlan.classify("https://evilyoutube.com/x.zip"))
        assertEquals(LinkKind.FILE, LinkPlan.classify("https://youtube.com.evil.example/x.zip"))
    }
    @Test fun rangesExpandWithPadding() {
        assertEquals(listOf("https://e.org/p01.jpg", "https://e.org/p02.jpg", "https://e.org/p03.jpg"), LinkPlan.parse("https://e.org/p[01-03].jpg"))
        assertEquals(listOf("https://e.org/1.zip", "https://e.org/2.zip", "https://a.org/z"), LinkPlan.parse("https://e.org/[1-2].zip\nhttps://a.org/z"))
    }
    @Test fun hugeOrBackwardsRangesAreLeftAlone() {
        assertEquals(emptyList<String>(), LinkPlan.parse("https://e.org/[1-9999].jpg"))
        assertEquals(emptyList<String>(), LinkPlan.parse("https://e.org/[5-1].jpg"))
    }
    @Test fun parsesSeveralLinksFromMessyText() {
        val urls = LinkPlan.parse("look: https://a.org/x.zip, and (https://youtu.be/abc)\n\nhttps://a.org/x.zip")
        assertEquals(listOf("https://a.org/x.zip", "https://youtu.be/abc"), urls)
        assertEquals(LinkPlan.Summary(1, 1), LinkPlan.summarize(urls))
    }
    @Test fun unsupportedGenericLinksMayUseGuardedFileTransferButMediaAndAuthNeverDo() {
        org.junit.Assert.assertTrue(LinkPlan.mayTryFile("https://example.org/download?id=42", "unsupported_media"))
        org.junit.Assert.assertFalse(LinkPlan.mayTryFile("https://instagram.com/reel/abc", "unsupported_media"))
        org.junit.Assert.assertFalse(LinkPlan.mayTryFile("https://example.org/download", "auth_required"))
        org.junit.Assert.assertFalse(LinkPlan.mayTryFile("https://example.org/download", "network"))
    }
    @Test fun categories() {
        assertEquals("Video", FileCategory.folder("a.mp4", "video/mp4"))
        assertEquals("Music", FileCategory.folder("a.m4a", "audio/mp4"))
        assertEquals("Archives", FileCategory.folder("a.zip", "application/zip"))
        assertEquals("Programs", FileCategory.folder("a.apk", "application/vnd.android.package-archive"))
        assertEquals("Documents", FileCategory.folder("a.pdf", "application/pdf"))
        assertEquals("", FileCategory.folder("a.jpg", "image/jpeg"))
        assertEquals("Other", FileCategory.folder("a.xyz", "application/octet-stream"))
    }
}
