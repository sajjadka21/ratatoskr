package app.ratatoskr.android

import okhttp3.Dns
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Test
import java.net.InetAddress
import java.net.UnknownHostException

/** No DNS packets or socket connections: every answer is injected as raw bytes. */
class PublicDnsTest {
    private fun ipv4(vararg octets: Int): InetAddress =
        InetAddress.getByAddress("fixture", octets.map { it.toByte() }.toByteArray())

    private fun ipv6(first: Int, second: Int, last: Int = 1): InetAddress =
        InetAddress.getByAddress("fixture", ByteArray(16).apply {
            this[0] = first.toByte(); this[1] = second.toByte(); this[15] = last.toByte()
        })

    private class FakeDns(private val answer: List<InetAddress>) : Dns {
        val queries = mutableListOf<String>()
        override fun lookup(hostname: String): List<InetAddress> {
            queries.add(hostname)
            return answer
        }
    }

    @Test fun validatedAnswersAreReturnedExactlyWithoutASecondLookup() {
        val globalIpv6 = InetAddress.getByAddress("fixture", listOf(
            0x20, 0x01, 0x48, 0x60, 0x48, 0x60, 0, 0, 0, 0, 0, 0, 0, 0, 0x88, 0x88,
        ).map { it.toByte() }.toByteArray())
        val answer = listOf(ipv4(93, 184, 216, 34), globalIpv6, ipv4(8, 8, 8, 8))
        val delegate = FakeDns(answer)
        val returned = PublicDns(delegate).lookup("downloads.example.org")
        assertSame("The transport must use the already-validated answer", answer, returned)
        assertEquals(listOf("downloads.example.org"), delegate.queries)
    }

    @Test fun aValidPublicIpv6LiteralCanStillBeDownloaded() {
        val global = InetAddress.getByAddress("fixture", listOf(
            0x20, 0x01, 0x48, 0x60, 0x48, 0x60, 0, 0, 0, 0, 0, 0, 0, 0, 0x88, 0x88,
        ).map { it.toByte() }.toByteArray())
        val delegate = FakeDns(listOf(global))
        assertEquals(listOf(global), PublicDns(delegate).lookup("2001:4860:4860::8888"))
        assertEquals(listOf("2001:4860:4860::8888"), delegate.queries)
    }

    @Test fun loopbackAndUnspecifiedAnswersAreRefused() {
        listOf(ipv4(127, 0, 0, 1), ipv4(0, 0, 0, 0),
            ipv6(0, 0), ipv6(0, 0, last = 0)).forEach { address ->
            assertThrows(UnknownHostException::class.java) {
                PublicDns(FakeDns(listOf(address))).lookup("downloads.example.org")
            }
        }
    }

    @Test fun everyRfc1918RangeAndIpv4LinkLocalAreRefused() {
        listOf(ipv4(10, 1, 2, 3), ipv4(172, 16, 0, 1), ipv4(172, 31, 255, 254),
            ipv4(192, 168, 1, 1), ipv4(169, 254, 169, 254)).forEach { address ->
            assertThrows(UnknownHostException::class.java) {
                PublicDns(FakeDns(listOf(address))).lookup("downloads.example.org")
            }
        }
    }

    @Test fun carrierGradeNatIsBlockedAtBothEndsOfItsAddressRange() {
        listOf(ipv4(100, 64, 0, 1), ipv4(100, 127, 255, 254)).forEach { address ->
            assertThrows(UnknownHostException::class.java) {
                PublicDns(FakeDns(listOf(address))).lookup("downloads.example.org")
            }
        }
    }

    @Test fun ipv6UniqueLocalLinkLocalAndMulticastAnswersAreRefused() {
        listOf(ipv6(0xfc, 0), ipv6(0xfd, 0xff), ipv6(0xfe, 0x80), ipv6(0xff, 2))
            .forEach { address ->
                assertThrows(UnknownHostException::class.java) {
                    PublicDns(FakeDns(listOf(address))).lookup("downloads.example.org")
                }
            }
    }

    @Test fun aPublicAddressDoesNotMakeAMixedPrivateAnswerSafe() {
        val public = ipv4(93, 184, 216, 34)
        val private = ipv4(192, 168, 1, 1)
        listOf(listOf(public, private), listOf(private, public)).forEach { answer ->
            assertThrows(UnknownHostException::class.java) {
                PublicDns(FakeDns(answer)).lookup("downloads.example.org")
            }
        }
    }

    @Test fun anEmptyAnswerCannotAuthorizeAConnection() {
        assertThrows(UnknownHostException::class.java) {
            PublicDns(FakeDns(emptyList())).lookup("downloads.example.org")
        }
    }

    @Test fun invalidOrPrivateHostSyntaxIsRejectedBeforeInvokingTheDelegate() {
        val delegate = FakeDns(listOf(ipv4(93, 184, 216, 34)))
        val dns = PublicDns(delegate)
        listOf("", "localhost", "printer.local", "host.internal", "intranet", "127.0.0.1",
            "192.168.1.1", "example.org/path", "user@example.org", "bad host", "example.org:8080", "example.org#fragment")
            .forEach { host -> assertThrows("Refuse invalid DNS host $host", UnknownHostException::class.java) { dns.lookup(host) } }
        assertEquals("Invalid syntax must not reach a resolver", emptyList<String>(), delegate.queries)
    }
}
