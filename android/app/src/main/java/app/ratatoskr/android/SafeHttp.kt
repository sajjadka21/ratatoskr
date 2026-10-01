package app.ratatoskr.android

import java.io.IOException
import java.net.HttpURLConnection
import java.net.InetAddress
import java.net.URI
import java.net.URL

class TransferFailure(val code: String) : IOException(code)

/** Validate the initial destination and every explicit redirect; keep TLS verification enabled. */
object SafeHttp {
    fun publicDestination(url: String) {
        if (!LinkUtils.isPublicHttpUrl(url)) throw TransferFailure("bad_link")
        val host = URI(url).host.trim('[', ']')
        val addresses = InetAddress.getAllByName(host)
        if (addresses.isEmpty() || addresses.any { address ->
            val a = address.address
            address.isAnyLocalAddress || address.isLoopbackAddress || address.isLinkLocalAddress ||
                address.isSiteLocalAddress || address.isMulticastAddress ||
                (a.size == 16 && (a[0].toInt() and 0xfe) == 0xfc) ||
                (a.size == 4 && (a[0].toInt() and 255) == 100 && (a[1].toInt() and 255) in 64..127)
        }) throw TransferFailure("bad_link")
    }

    fun open(url: String, headers: Map<String, String> = emptyMap(), check: () -> Unit = {}): HttpURLConnection {
        var destination = url
        repeat(6) {
            check()
            publicDestination(destination)
            val connection = URL(destination).openConnection() as HttpURLConnection
            connection.instanceFollowRedirects = false
            connection.connectTimeout = 15000
            connection.readTimeout = 15000
            connection.setRequestProperty("Accept-Encoding", "identity")
            connection.setRequestProperty("User-Agent", "Ratatoskr/1.0 Android")
            headers.forEach { (key, value) -> connection.setRequestProperty(key, value) }
            try {
                val status = connection.responseCode
                if (status in setOf(301, 302, 303, 307, 308)) {
                    val next = connection.getHeaderField("Location") ?: throw TransferFailure("bad_link")
                    val resolved = URI(destination).resolve(next).toASCIIString()
                    if (destination.startsWith("https:", true) && !resolved.startsWith("https:", true)) throw TransferFailure("bad_link")
                    destination = resolved
                    connection.disconnect()
                } else return connection
            } catch (error: Exception) {
                connection.disconnect()
                throw error
            }
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
