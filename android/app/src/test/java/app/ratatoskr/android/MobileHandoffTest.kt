package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.URLEncoder

/** Deep-link parsing is plain JVM code and must never initialize native media
 * tools, resolve DNS, or start a download merely because a QR was scanned. */
class MobileHandoffTest {
    private fun payload(url: String) = "ratatoskr://add?url=" + URLEncoder.encode(url, "UTF-8")

    @Test fun instagramAndSignedFileDestinationsRoundTripWithoutChangingQueryBytes() {
        listOf(
            "https://www.instagram.com/reel/Fixture123/?igsh=fixture%2Bvalue",
            "https://cdn.example.com/a%2Fb/file%20name.zip?X-Amz-Signature=fixture%2Bbytes&Expires=1700000000&label=a%2Bb+c&part=a%26b%3Dc",
            "https://example.com/%D9%BE%D8%B1%D9%88%D9%86%D8%AF%D9%87.zip?name=%D8%A2%D8%B2%D9%85%D8%A7%DB%8C%D8%B4",
            "http://93.184.216.34/file.zip",
        ).forEach { source ->
            val decoded = LinkUtils.handoffUrl(payload(source))
            assertEquals(source, decoded)
            assertTrue(LinkUtils.isPublicHttpUrl(decoded!!))
        }
    }

    @Test fun malformedOrExtendedDeepLinkEnvelopesAreRejected() {
        val value = URLEncoder.encode("https://example.com/file.zip", "UTF-8")
        listOf(
            "", "https://add?url=$value", "other://add?url=$value", "ratatoskr://other?url=$value",
            "ratatoskr://add:443?url=$value", "ratatoskr://user@add?url=$value", "ratatoskr://add/path?url=$value",
            "ratatoskr://add/?url=$value", "ratatoskr://add?url=$value#fragment",
            "ratatoskr://add?url=$value&url=$value", "ratatoskr://add?url=$value&mode=quick",
            "ratatoskr://add?other=$value", "ratatoskr://add", "ratatoskr://add?url=", "ratatoskr://add?url=%ZZ",
        ).forEach { assertNull(it, LinkUtils.handoffUrl(it)) }
    }

    @Test fun decodedDestinationStillMustBeAnUnambiguousPublicWebUrl() {
        listOf(
            "http://localhost/file", "http://printer.local/file", "http://router.internal/file", "http://intranet/file",
            "http://0.1.2.3/file", "http://127.0.0.1/file", "http://10.1.2.3/file", "http://192.168.1.1/file",
            "http://172.16.1.2/file", "http://169.254.169.254/file", "http://100.64.0.1/file",
            "http://224.0.0.1/file", "http://240.0.0.1/file", "http://255.255.255.255/file",
            "http://[::]/file", "http://[::1]/file", "http://[fc00::1]/file", "http://[fe80::1]/file",
            "http://[::ffff:192.168.1.1]/file", "http://134744072/file", "http://0x08080808/file",
            "http://010.010.010.010/file", "http://8.8.2056/file", "file:///tmp/file", "ftp://example.com/file",
        ).forEach { assertNull(it, LinkUtils.handoffUrl(payload(it))) }
    }

    @Test fun credentialsAndEncodedCredentialKeysCannotTravelInTheQr() {
        listOf(
            "https://user@example.com/file", "https://user:fixture@example.com/file",
            "https://example.com/file?access_token=fixture", "https://example.com/file?AUTHORIZATION=fixture",
            "https://example.com/file?Password=fixture", "https://example.com/file?SESSIONID=fixture",
            "https://example.com/file?cookie=fixture", "https://example.com/file?%61ccess_token=fixture",
            "https://example.com/file?%41UTHORIZATION=fixture", "https://example.com/file?%70assword=fixture",
        ).forEach { assertNull(it, LinkUtils.handoffUrl(payload(it))) }
    }

    @Test fun oversizedPayloadCannotBypassTheLimitWithMultibyteText() {
        assertNull(LinkUtils.handoffUrl(payload("https://example.com/?value=" + "x".repeat(2100))))
        assertNull(LinkUtils.handoffUrl(payload("https://example.com/?value=" + "آ".repeat(400))))
    }
}
