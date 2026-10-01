package app.ratatoskr.android

import okhttp3.Dns
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import java.io.ByteArrayInputStream
import java.io.IOException
import java.io.InputStream
import java.net.InetAddress
import java.net.Proxy
import java.net.URI
import java.net.URL
import java.net.UnknownHostException
import java.util.concurrent.TimeUnit

class TransferFailure(val code: String) : IOException(code)
class UnsafeDestination : UnknownHostException("bad_link")

/** OkHttp connects to these exact validated answers, without a second DNS lookup. */
class PublicDns(private val delegate: Dns = Dns.SYSTEM) : Dns {
    override fun lookup(hostname: String): List<InetAddress> {
        val host = if (':' in hostname && !hostname.startsWith('[')) "[$hostname]" else hostname
        val destination = "https://$host/"
        if (!LinkUtils.isPublicHttpUrl(destination) || runCatching { URI(destination).host.trim('[', ']') != hostname.trim('[', ']') }.getOrDefault(true)) throw UnsafeDestination()
        val answers = delegate.lookup(hostname)
        if (answers.isEmpty() || answers.any { !isPublic(it) }) throw UnsafeDestination()
        return answers
    }
    companion object {
        fun isPublic(address: InetAddress): Boolean {
            val bytes = address.address
            val first = bytes[0].toInt() and 255
            val second = bytes[1].toInt() and 255
            return !(address.isAnyLocalAddress || address.isLoopbackAddress || address.isLinkLocalAddress || address.isSiteLocalAddress || address.isMulticastAddress ||
                (bytes.size == 16 && (first and 0xfe) == 0xfc) ||
                (bytes.size == 4 && (first == 0 || first >= 240 || (first == 100 && second in 64..127) || (first == 198 && second in 18..19))))
        }
    }
}

interface HttpConnection {
    val responseCode: Int
    val contentLengthLong: Long
    val contentType: String?
    val url: URL
    val inputStream: InputStream
    fun getHeaderField(name: String): String?
    fun disconnect()
}

private class VerifiedConnection(private val response: Response) : HttpConnection {
    override val responseCode get() = response.code
    override val contentLengthLong get() = response.body?.contentLength() ?: -1
    override val contentType get() = response.header("Content-Type")
    override val url get() = response.request.url.toUrl()
    override val inputStream get() = response.body?.byteStream() ?: ByteArrayInputStream(ByteArray(0))
    override fun getHeaderField(name: String) = response.header(name)
    override fun disconnect() = response.close()
}

/** Every redirect gets a new validated resolution; platform TLS verification stays enabled. */
object SafeHttp {
    private val dns = PublicDns()
    private val client = OkHttpClient.Builder().dns(dns).proxy(Proxy.NO_PROXY)
        .followRedirects(false).followSslRedirects(false)
        .connectTimeout(15, TimeUnit.SECONDS).readTimeout(15, TimeUnit.SECONDS).build()

    fun publicDestination(url: String) {
        if (!LinkUtils.isPublicHttpUrl(url)) throw TransferFailure("bad_link")
        try { dns.lookup(URI(url).host.trim('[', ']')) }
        catch (error: UnsafeDestination) { throw TransferFailure("bad_link") }
    }

    fun open(url: String, headers: Map<String, String> = emptyMap(), check: () -> Unit = {}): HttpConnection {
        var destination = url
        repeat(6) {
            check()
            if (!LinkUtils.isPublicHttpUrl(destination)) throw TransferFailure("bad_link")
            // OkHttp skips Dns for numeric hosts, so validate those explicitly too.
            val host = URI(destination).host.trim('[', ']')
            if (':' in host || Regex("^[0-9.]+$").matches(host)) publicDestination(destination)
            val request = Request.Builder().url(destination).header("Accept-Encoding", "identity").header("User-Agent", "Ratatoskr Android")
            headers.forEach { (key, value) -> request.header(key, value) }
            val response = try { client.newCall(request.build()).execute() } catch (error: UnsafeDestination) { throw TransferFailure("bad_link") }
            val connection = VerifiedConnection(response)
            try {
                if (response.code in setOf(301, 302, 303, 307, 308)) {
                    val next = response.header("Location") ?: throw TransferFailure("bad_link")
                    val resolved = URI(destination).resolve(next).toASCIIString()
                    if (destination.startsWith("https:", true) && !resolved.startsWith("https:", true)) throw TransferFailure("bad_link")
                    destination = resolved
                    connection.disconnect()
                } else return connection
            } catch (error: Exception) { connection.disconnect(); throw error }
        }
        throw TransferFailure("bad_link")
    }

    fun canonicalSource(url: String, check: () -> Unit = {}): String {
        val canonical = LinkUtils.canonicalUrl(url)
        val host = URI(canonical).host.lowercase()
        if (host in setOf("instagram.com", "www.instagram.com", "m.instagram.com") && URI(canonical).path.startsWith("/share/")) {
            val connection = open(canonical, check = check)
            return try { LinkUtils.canonicalUrl(connection.url.toString()) } finally { connection.disconnect() }
        }
        publicDestination(canonical)
        return canonical
    }

    fun requireSuccess(status: Int) {
        when {
            status == 429 -> throw TransferFailure("rate_limited")
            status in setOf(401, 403) -> throw TransferFailure("auth_required")
            status == 404 || status == 410 -> throw TransferFailure("not_found")
            status !in 200..299 -> throw TransferFailure("network")
        }
    }
}
