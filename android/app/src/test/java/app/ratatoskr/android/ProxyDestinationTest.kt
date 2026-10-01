package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** Parse the extractor's proxy request without sockets or DNS. Actual DNS
 * resolution is guarded separately at connect time, including every redirect. */
class ProxyDestinationTest {
    private fun rejected(line: String) {
        val error = assertThrows("Reject malformed or private proxy target: $line", TransferFailure::class.java) {
            ProxyDestination.parse(line)
        }
        assertEquals("bad_link", error.code)
    }

    @Test fun connectRequiresAnExplicitPublicTlsAuthority() {
        val target = ProxyDestination.parse("CONNECT media.example.org:443 HTTP/1.1")
        assertEquals("media.example.org", target.host)
        assertEquals(443, target.port)
        assertTrue(target.connect)
        assertEquals("", target.path)
        assertEquals("media.example.org:443", target.authority)
    }

    @Test fun connectPreservesBracketedPublicIpv6AuthorityButExposesBareHost() {
        val target = ProxyDestination.parse("CONNECT [2001:4860:4860::8888]:443 HTTP/1.1")
        assertEquals("2001:4860:4860::8888", target.host)
        assertEquals(443, target.port)
        assertTrue(target.connect)
        assertEquals("", target.path)
        assertEquals("[2001:4860:4860::8888]:443", target.authority)
    }

    @Test fun absoluteHttpTargetKeepsItsExactEscapedPathAndSignedQuery() {
        val target = ProxyDestination.parse("GET http://media.example.org/path%2Fkeep?q=1&signature=a%2Fb%3D HTTP/1.1")
        assertEquals(ProxyDestination("media.example.org", 80, false,
            "/path%2Fkeep?q=1&signature=a%2Fb%3D", "media.example.org"), target)
    }

    @Test fun explicitPublicHttpPortIsPreservedInAuthority() {
        assertEquals(ProxyDestination("media.example.org", 8080, false,
            "/file?q=1", "media.example.org:8080"),
            ProxyDestination.parse("GET http://media.example.org:8080/file?q=1 HTTP/1.1"))
    }

    @Test fun emptyHttpPathDefaultsToSlashIncludingWhenQueryIsPresent() {
        assertEquals("/", ProxyDestination.parse("GET http://media.example.org HTTP/1.1").path)
        assertEquals("/?q=1", ProxyDestination.parse("GET http://media.example.org?q=1 HTTP/1.1").path)
    }

    @Test fun headAndPostUseTheSameDestinationValidationAsGet() {
        for (method in listOf("HEAD", "POST")) {
            assertEquals(ProxyDestination("media.example.org", 80, false, "/endpoint", "media.example.org"),
                ProxyDestination.parse("$method http://media.example.org/endpoint HTTP/1.1"))
        }
    }

    @Test fun credentialsAreNeverAcceptedForEitherProxyTransport() {
        listOf(
            "GET http://user:password@media.example.org/file HTTP/1.1",
            "GET http://user@media.example.org/file HTTP/1.1",
            "CONNECT user:password@media.example.org:443 HTTP/1.1",
        ).forEach(::rejected)
    }

    @Test fun literalPrivateDestinationsCannotBeReachedThroughTheExtractor() {
        for (host in listOf("127.0.0.1", "0.0.0.0", "10.0.0.1", "172.16.0.1", "192.168.1.1", "169.254.169.254", "100.64.0.1", "[::1]", "[::]", "[fd12::1]", "[fe80::1]")) {
            rejected("CONNECT $host:443 HTTP/1.1")
            rejected("GET http://$host/private HTTP/1.1")
        }
    }

    @Test fun localNamesAndAmbiguousNumericAddressesAreRejected() {
        for (host in listOf("localhost", "localhost.", "router.local", "router.internal", "router.lan", "test.localhost", "intranet", "2130706433", "0x7f000001", "0177.0.0.1")) {
            rejected("CONNECT $host:443 HTTP/1.1")
            rejected("GET http://$host/file HTTP/1.1")
        }
    }

    @Test fun connectCannotSmuggleAUrlPathQueryOrFragment() {
        listOf(
            "CONNECT media.example.org HTTP/1.1",
            "CONNECT media.example.org:443/path HTTP/1.1",
            "CONNECT media.example.org:443?q=1 HTTP/1.1",
            "CONNECT media.example.org:443#fragment HTTP/1.1",
            "CONNECT https://media.example.org:443 HTTP/1.1",
            "CONNECT http://media.example.org:80 HTTP/1.1",
        ).forEach(::rejected)
    }

    @Test fun absoluteHttpsMustUseConnectInsteadOfAPlainHttpForward() {
        rejected("GET https://media.example.org/file HTTP/1.1")
        rejected("POST https://media.example.org/file HTTP/1.1")
        rejected("GET ftp://media.example.org/file HTTP/1.1")
        rejected("GET /relative/path HTTP/1.1")
        rejected("GET //media.example.org/file HTTP/1.1")
    }

    @Test fun invalidAndOutOfRangePortsAreRejectedBeforeOpeningASocket() {
        for (port in listOf("", "0", "-1", "65536", "999999999999999999999", "443x", "+443")) {
            rejected("CONNECT media.example.org:$port HTTP/1.1")
            rejected("GET http://media.example.org:$port/file HTTP/1.1")
        }
    }

    @Test fun incompleteRequestLinesAndInvalidMethodTokensAreRejected() {
        listOf(
            "", "GET", "GET HTTP/1.1", "GET  HTTP/1.1",
            "GET http://media.example.org/file", "G@T http://media.example.org/file HTTP/1.1",
            "GET http://media.example.org/file HTTP/1.1 trailing",
            "GET http://media.example.org/file BAD/1.1",
        ).forEach(::rejected)
    }

    @Test fun rawControlsAndHeaderInjectionAreRejectedRatherThanTrimmed() {
        listOf(
            "GET http://media.example.org/file HTTP/1.1\r\nHost: 127.0.0.1",
            "GET http://media.example.org/file HTTP/1.1\n",
            "CONNECT media.example.org:443 HTTP/1.1\r",
            "GET\thttp://media.example.org/file HTTP/1.1",
            "GET http://media.example.org/\u0000file HTTP/1.1",
            "GET http://media.example.org/\u007ffile HTTP/1.1",
        ).forEach(::rejected)
    }

    @Test fun fragmentsAreRejectedWithoutSilentlyChangingTheRequestedTarget() {
        rejected("GET http://media.example.org/file#fragment HTTP/1.1")
        rejected("GET http://media.example.org/file?q=1#fragment HTTP/1.1")
    }
}
