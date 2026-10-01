package app.ratatoskr.android

import okhttp3.Dns
import java.io.BufferedInputStream
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

/** Scoped loopback proxy. TLS bytes remain end-to-end; no request data is logged. */
class SafeProxy(
    private val check: () -> Unit = {},
    private val dns: Dns = PublicDns(),
    private val connect: (InetAddress, Int) -> Socket = { address, port ->
        Socket().apply {
            soTimeout = 15000
            try { connect(InetSocketAddress(address, port), 15000) } catch (error: Exception) { close(); throw error }
        }
    },
) : AutoCloseable {
    private val closed = AtomicBoolean(false)
    private val sockets = ConcurrentHashMap.newKeySet<Socket>()
    private val workers = Executors.newFixedThreadPool(4) { action -> Thread(action, "ratatoskr-proxy").apply { isDaemon = true } }
    private val server = ServerSocket(0, 8, InetAddress.getByName("127.0.0.1"))
    val url = "http://127.0.0.1:" + server.localPort
    private val acceptor = thread(name = "ratatoskr-proxy-accept", isDaemon = true) {
        while (!closed.get()) {
            val client = try { server.accept() } catch (_: Exception) { break }
            if (closed.get() || sockets.size >= 12) { client.close(); continue }
            sockets.add(client); client.soTimeout = 15000
            try { workers.execute { handle(client) } } catch (_: Exception) { sockets.remove(client); client.close() }
        }
    }

    private fun handle(client: Socket) {
        var remote: Socket? = null
        try {
            check()
            val input = BufferedInputStream(client.getInputStream())
            var remaining = 16 * 1024
            fun line(): String {
                val bytes = ByteArrayOutputStream()
                while (true) {
                    if (--remaining < 0) throw TransferFailure("bad_link")
                    val next = input.read()
                    if (next < 0) throw TransferFailure("bad_link")
                    if (next == 10) {
                        val raw = bytes.toByteArray()
                        if (raw.isEmpty() || raw.last().toInt() != 13) throw TransferFailure("bad_link")
                        return String(raw, 0, raw.size - 1, Charsets.ISO_8859_1)
                    }
                    bytes.write(next)
                }
            }
            val requestLine = line()
            val destination = ProxyDestination.parse(requestLine)
            val headers = mutableListOf<String>()
            while (true) {
                val header = line()
                if (header.isEmpty()) break
                if (!header.contains(':') || header.first().isWhitespace() || header.any { it.code < 32 && it != '\t' || it.code == 127 }) throw TransferFailure("bad_link")
                val key = header.substringBefore(':').lowercase()
                if (key !in setOf("host", "connection", "proxy-connection", "proxy-authorization")) headers.add(header)
            }
            check()
            val addresses = dns.lookup(destination.host)
            if (addresses.isEmpty() || addresses.any { !PublicDns.isPublic(it) }) throw TransferFailure("bad_link")
            for (address in addresses) {
                check()
                try { remote = connect(address, destination.port); break } catch (_: java.io.IOException) { }
            }
            val peer = remote ?: throw TransferFailure("network")
            sockets.add(peer); peer.soTimeout = 15000
            check()
            if (destination.connect) {
                client.getOutputStream().write("HTTP/1.1 200 Connection Established\r\n\r\n".toByteArray(Charsets.ISO_8859_1))
                client.getOutputStream().flush()
            } else {
                val forwarded = requestLine.substringBefore(' ') + " " + destination.path + " HTTP/1.1\r\nHost: " + destination.authority +
                    "\r\nConnection: close\r\n" + headers.joinToString("", transform = { it + "\r\n" }) + "\r\n"
                peer.getOutputStream().write(forwarded.toByteArray(Charsets.ISO_8859_1)); peer.getOutputStream().flush()
            }
            val response = thread(name = "ratatoskr-proxy-response", isDaemon = true) {
                try { relay(peer.getInputStream(), client.getOutputStream()) } catch (_: Exception) { }
                finally { runCatching { client.close() }; runCatching { peer.close() } }
            }
            try { relay(input, peer.getOutputStream()) } finally {
                runCatching { peer.close() }; runCatching { client.close() }
                response.join(1000)
            }
        } catch (_: Exception) {
            // The owner handles failures. Never print URLs, headers or payloads.
        } finally {
            sockets.remove(client); runCatching { client.close() }
            remote?.let { sockets.remove(it); runCatching { it.close() } }
        }
    }
    private fun relay(input: InputStream, output: OutputStream) {
        val buffer = ByteArray(64 * 1024)
        while (!closed.get()) {
            check(); val count = input.read(buffer)
            if (count < 0) return
            check(); output.write(buffer, 0, count); output.flush()
        }
    }
    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        runCatching { server.close() }
        sockets.forEach { runCatching { it.close() } }; sockets.clear()
        workers.shutdownNow()
        acceptor.join(1000)
    }
}
