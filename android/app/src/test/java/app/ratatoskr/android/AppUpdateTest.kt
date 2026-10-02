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
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
class AppUpdateTest {
    private val base = "https://github.com/sajjadka21/ratatoskr/releases/download/v1.2.0/"
    private val digest = "a".repeat(64)
    private fun release(tag: String = "v1.2.0", extra: String = "", url: String = base) = """{ "tag_name": "$tag", "body": "Notes", $extra "assets": [
        { "name": "Ratatoskr-android-arm64-v8a.apk", "browser_download_url": "${url}Ratatoskr-android-arm64-v8a.apk", "size": 100, "digest": "sha256:$digest" },
        { "name": "Ratatoskr-android-armeabi-v7a.apk", "browser_download_url": "${url}Ratatoskr-android-armeabi-v7a.apk", "size": 90 },
        { "name": "Ratatoskr-android.apk", "browser_download_url": "${url}Ratatoskr-android.apk", "size": 100, "digest": "sha256:$digest" } ] }"""

    @Test fun versionsCompareNumerically() {
        assertTrue(AppUpdate.isNewer("1.2.0", "1.1.1"))
        assertTrue(AppUpdate.isNewer("v1.10.0", "1.9.9"))
        assertTrue(AppUpdate.isNewer("2", "1.9"))
        assertFalse(AppUpdate.isNewer("1.1.1", "1.1.1"))
        assertFalse(AppUpdate.isNewer("1.1.0", "1.1.1"))
        assertFalse(AppUpdate.isNewer("1.1.1-beta", "1.1.1"))
    }

    @Test fun picksTheApkForThisCpuWithItsChecksum() {
        val offer = AppUpdate.parse(release(), listOf("arm64-v8a", "armeabi-v7a"), "1.1.1")!!
        assertEquals("1.2.0", offer.version)
        assertTrue(offer.url.endsWith("Ratatoskr-android-arm64-v8a.apk"))
        assertEquals(digest, offer.sha256)
        val older = AppUpdate.parse(release(), listOf("armeabi-v7a"), "1.1.1")!!
        assertEquals("", older.sha256)   // published without a checksum: never installed automatically
    }

    @Test fun nothingIsOfferedWhenCurrentDraftOrFromAnotherPlace() {
        assertNull(AppUpdate.parse(release(), listOf("arm64-v8a"), "1.2.0"))
        assertNull(AppUpdate.parse(release(extra = "\"prerelease\": true,"), listOf("arm64-v8a"), "1.1.1"))
        assertNull(AppUpdate.parse(release(extra = "\"draft\": true,"), listOf("arm64-v8a"), "1.1.1"))
        assertNull(AppUpdate.parse(release(url = "https://evil.example/"), listOf("arm64-v8a"), "1.1.1"))
        assertNull(AppUpdate.parse(release(), listOf("mips"), "1.1.1")?.takeIf { false })
    }

    private class Conn(private val c: HttpURLConnection) : HttpConnection {
        override val responseCode get() = c.responseCode
        override val contentLengthLong get() = c.contentLengthLong
        override val contentType: String? get() = c.contentType
        override val url: URL get() = c.url
        override val inputStream: InputStream get() = c.inputStream
        override fun getHeaderField(name: String) = c.getHeaderField(name)
        override fun disconnect() = c.disconnect()
    }

    private fun serve(body: ByteArray, test: (String) -> Unit) {
        val server = TestHttpServer { request -> request.respond(200, emptyMap(), body.size.toLong()); request.out.write(body) }
        server.start()
        try { test("http://127.0.0.1:${server.port}/app.apk") } finally { server.stop() }
    }
    private val open: (String, Map<String, String>, () -> Unit) -> HttpConnection = { url, _, _ -> Conn(URL(url).openConnection() as HttpURLConnection) }

    @Test fun aMatchingChecksumKeepsTheFileAndAWrongOneDeletesIt() {
        val body = ByteArray(300_000) { it.toByte() }
        val good = MessageDigest.getInstance("SHA-256").digest(body).joinToString("") { "%02x".format(it) }
        val dir = java.nio.file.Files.createTempDirectory("upd").toFile()
        try {
            serve(body) { url ->
                val target = java.io.File(dir, "ok.apk")
                AppUpdate.download(UpdateOffer("1.2.0", "", url, good, body.size.toLong()), target, TransferControl { true }, { _, _ -> }, open)
                assertEquals(body.size.toLong(), target.length())

                val bad = java.io.File(dir, "bad.apk")
                val error = assertThrows(TransferFailure::class.java) {
                    AppUpdate.download(UpdateOffer("1.2.0", "", url, "b".repeat(64), body.size.toLong()), bad, TransferControl { true }, { _, _ -> }, open)
                }
                assertEquals("checksum_mismatch", error.code)
                assertFalse(bad.exists())

                val none = assertThrows(TransferFailure::class.java) {
                    AppUpdate.download(UpdateOffer("1.2.0", "", url, "", 0), java.io.File(dir, "none.apk"), TransferControl { true }, { _, _ -> }, open)
                }
                assertEquals("unverifiable", none.code)
            }
        } finally { dir.deleteRecursively() }
    }
}
