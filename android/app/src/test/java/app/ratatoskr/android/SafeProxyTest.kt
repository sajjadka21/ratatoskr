package app.ratatoskr.android

import okhttp3.Dns
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.InputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.SocketException
import java.net.URI
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/** Every connection is to a server owned by the test on loopback. The injected
 * connector records the validated public address instead of reaching it. */
class SafeProxyTest {
    private val loopback = InetAddress.getByName("127.0.0.1")
    private val publicAddress = InetAddress.getByAddress(byteArrayOf(93, 184.toByte(), 216.toByte(), 34))
    private val executor = Executors.newCachedThreadPool()
    private val sockets = mutableListOf<Socket>()
    private val servers = mutableListOf<ServerSocket>()
    private val proxies = mutableListOf<SafeProxy>()

    @After fun closeOnlyTestOwnedSocketsAndThreads() {
        proxies.forEach { it.close() }
        synchronized(sockets) { sockets.forEach { runCatching { it.close() } } }
        servers.forEach { it.close() }
        executor.shutdownNow()
        assertTrue("loopback fixture threads terminate", executor.awaitTermination(5, TimeUnit.SECONDS))
    }

    private fun peer(): ServerSocket = ServerSocket(0, 5, loopback).also {
        it.soTimeout = 3_000
        servers.add(it)
    }

    private fun track(socket: Socket): Socket = socket.also {
        it.soTimeout = 3_000
        synchronized(sockets) { sockets.add(it) }
    }

    private fun proxy(check: () -> Unit = {}, dns: Dns = object : Dns { override fun lookup(hostname: String) = listOf(publicAddress) }, connect: (InetAddress, Int) -> Socket): SafeProxy =
        SafeProxy(check = check, dns = dns, connect = connect).also { proxies.add(it) }

    private fun client(proxy: SafeProxy): Socket {
        val uri = URI(proxy.url)
        assertEquals("http", uri.scheme)
        assertEquals("127.0.0.1", uri.host)
        assertTrue(uri.port > 0)
        return track(Socket(loopback, uri.port))
    }

    private fun headers(input: InputStream): String {
        val bytes = ByteArrayOutputStream()
        while (bytes.size() < 16 * 1024) {
            val next = input.read()
            if (next == -1) return bytes.toString("ISO-8859-1")
            bytes.write(next)
            val content = bytes.toByteArray()
            if (content.size >= 4 && content.takeLast(4) == listOf(13.toByte(), 10.toByte(), 13.toByte(), 10.toByte())) return bytes.toString("ISO-8859-1")
        }
        throw AssertionError("fixture header limit exceeded")
    }

    private fun send(socket: Socket, request: String) {
        socket.getOutputStream().write(request.toByteArray(Charsets.ISO_8859_1))
        socket.getOutputStream().flush()
    }

    @Test fun connectTunnelsOpaqueBytesWithoutInterceptingTls() {
        val peer = peer()
        val requestBytes = byteArrayOf(22, 3, 3, 0, 5, 0, 127, -1, 10, 13)
        val responseBytes = byteArrayOf(23, 3, 3, 0, 4, -128, 0, 11, 12)
        val received = executor.submit<ByteArray> {
            val remote = track(peer.accept())
            val bytes = ByteArray(requestBytes.size)
            DataInputStream(remote.getInputStream()).readFully(bytes)
            remote.getOutputStream().write(responseBytes)
            remote.getOutputStream().flush()
            bytes
        }
        val connections = AtomicInteger()
        val proxy = proxy(connect = { address, port ->
            assertEquals(publicAddress, address)
            assertEquals(443, port)
            connections.incrementAndGet()
            track(Socket(loopback, peer.localPort))
        })
        val client = client(proxy)
        send(client, "CONNECT media.example.org:443 HTTP/1.1\r\nHost: media.example.org:443\r\n\r\n")
        assertTrue(headers(client.getInputStream()).lineSequence().first().matches(Regex("HTTP/1\\.[01] 200.*")))
        client.getOutputStream().write(requestBytes)
        client.getOutputStream().flush()
        val reply = ByteArray(responseBytes.size)
        DataInputStream(client.getInputStream()).readFully(reply)
        assertArrayEquals(requestBytes, received.get(3, TimeUnit.SECONDS))
        assertArrayEquals(responseBytes, reply)
        assertEquals(1, connections.get())
    }

    @Test fun httpForwardsExactTargetAndReplacesSpoofedHostWithoutProxyCredentials() {
        val peer = peer()
        val received = executor.submit<String> {
            val remote = track(peer.accept())
            val request = headers(remote.getInputStream())
            send(remote, "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            request
        }
        val proxy = proxy(connect = { address, port ->
            assertEquals(publicAddress, address)
            assertEquals(8080, port)
            track(Socket(loopback, peer.localPort))
        })
        val client = client(proxy)
        send(client, "GET http://media.example.org:8080/path%2Fkeep?q=1&signature=a%2Fb%3D HTTP/1.1\r\n" +
            "Host: 127.0.0.1\r\nProxy-Authorization: Basic secret\r\nProxy-Connection: keep-alive\r\nUser-Agent: fixture\r\n\r\n")
        assertTrue(headers(client.getInputStream()).startsWith("HTTP/1.1 200"))
        val body = ByteArray(2)
        DataInputStream(client.getInputStream()).readFully(body)
        assertEquals("ok", String(body, Charsets.US_ASCII))
        val request = received.get(3, TimeUnit.SECONDS)
        assertTrue(request.startsWith("GET /path%2Fkeep?q=1&signature=a%2Fb%3D HTTP/1.1\r\n"))
        assertTrue(request.lineSequence().any { it.equals("Host: media.example.org:8080", ignoreCase = true) })
        assertFalse(request.contains("127.0.0.1"))
        assertFalse(request.contains("Proxy-Authorization", ignoreCase = true))
        assertFalse(request.contains("Proxy-Connection", ignoreCase = true))
        assertFalse(request.contains("secret"))
    }

    @Test fun publicDnsRejectsPrivateAnswerBeforeThePinnedConnectorRuns() {
        val calls = AtomicInteger()
        val proxy = proxy(dns = PublicDns(object : Dns { override fun lookup(hostname: String) = listOf(loopback) }), connect = { _, _ ->
            calls.incrementAndGet()
            throw AssertionError("private DNS answer must never reach a socket connector")
        })
        val client = client(proxy)
        send(client, "CONNECT media.example.org:443 HTTP/1.1\r\nHost: media.example.org:443\r\n\r\n")
        val response = headers(client.getInputStream())
        assertFalse(response.lineSequence().first().matches(Regex("HTTP/1\\.[01] 200.*")))
        assertEquals(0, calls.get())
    }

    @Test fun stoppedPolicyCheckAbortsBeforeOutboundConnection() {
        val calls = AtomicInteger()
        val proxy = proxy(check = { throw TransferFailure("waiting_network") }, connect = { _, _ ->
            calls.incrementAndGet()
            throw AssertionError("revoked network consent must stop before connect")
        })
        val client = client(proxy)
        send(client, "GET http://media.example.org/file HTTP/1.1\r\nHost: media.example.org\r\n\r\n")
        val response = headers(client.getInputStream())
        assertFalse(response.lineSequence().first().matches(Regex("HTTP/1\\.[01] 200.*")))
        assertEquals(0, calls.get())
    }

    @Test fun closeTerminatesAnOpenTunnelAndReleasesItsPeer() {
        val peer = peer()
        val accepted = CountDownLatch(1)
        val exited = executor.submit<Boolean> {
            val remote = track(peer.accept())
            accepted.countDown()
            try { remote.getInputStream().read() == -1 } catch (_: SocketException) { true }
        }
        val proxy = proxy(connect = { _, _ -> track(Socket(loopback, peer.localPort)) })
        val client = client(proxy)
        send(client, "CONNECT media.example.org:443 HTTP/1.1\r\nHost: media.example.org:443\r\n\r\n")
        assertTrue(headers(client.getInputStream()).lineSequence().first().matches(Regex("HTTP/1\\.[01] 200.*")))
        assertTrue(accepted.await(3, TimeUnit.SECONDS))
        proxy.close()
        val clientClosed = try { client.getInputStream().read() == -1 } catch (_: SocketException) { true }
        assertTrue(clientClosed)
        assertTrue(exited.get(3, TimeUnit.SECONDS))
        // close is safe when invoked again by an engine finally block.
        proxy.close()
    }
}
