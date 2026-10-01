package app.ratatoskr.android

import java.io.OutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket

/** A tiny HTTP/1.1 server for tests, built on plain sockets because Android's unit-test classpath has no JDK HTTP server. */
class TestHttpServer(private val handle: (Request) -> Unit) {
    class Request(val headers: Map<String, String>, val out: OutputStream, private val socket: Socket) {
        fun respond(status: Int, headers: Map<String, String>, length: Long) {
            val text = buildString {
                append("HTTP/1.1 $status X\r\n")
                headers.forEach { (name, value) -> append("$name: $value\r\n") }
                append("Content-Length: $length\r\nConnection: close\r\n\r\n")
            }
            out.write(text.toByteArray(Charsets.ISO_8859_1))
        }
        /** Drops the connection without finishing the body. */
        fun abort() = socket.close()
    }

    private val server = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
    val port get() = server.localPort
    @Volatile private var running = true

    fun start() {
        Thread {
            while (running) {
                val socket = try { server.accept() } catch (error: Exception) { break }
                Thread {
                    try {
                        socket.use {
                            val reader = it.getInputStream().bufferedReader(Charsets.ISO_8859_1)
                            reader.readLine() ?: return@use
                            val headers = mutableMapOf<String, String>()
                            while (true) {
                                val line = reader.readLine() ?: break
                                if (line.isEmpty()) break
                                headers[line.substringBefore(':').trim().lowercase()] = line.substringAfter(':').trim()
                            }
                            val out = it.getOutputStream().buffered()
                            handle(Request(headers, out, it))
                            out.flush()
                        }
                    } catch (_: Exception) { /* client went away */ }
                }.apply { isDaemon = true }.start()
            }
        }.apply { isDaemon = true }.start()
    }

    fun stop() { running = false; runCatching { server.close() } }
}
