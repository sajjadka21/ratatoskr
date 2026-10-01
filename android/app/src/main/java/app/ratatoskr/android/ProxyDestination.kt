package app.ratatoskr.android

import java.net.InetAddress
import java.net.URI

/** Parse without network access. Resolution and pinning happen immediately before dialing. */
data class ProxyDestination(val host: String, val port: Int, val connect: Boolean, val path: String, val authority: String) {
    companion object {
        fun parse(requestLine: String): ProxyDestination {
            fun fail(): Nothing = throw TransferFailure("bad_link")
            if (requestLine.any { it.code < 32 || it.code == 127 }) fail()
            val fields = requestLine.split(' ')
            if (fields.size != 3 || fields.any { it.isEmpty() } ||
                !Regex("^[A-Z]+$").matches(fields[0]) || fields[2] !in setOf("HTTP/1.0", "HTTP/1.1")) fail()
            val connect = fields[0] == "CONNECT"
            val target = fields[1]
            val uri = try { URI(if (connect) "https://$target" else target) } catch (error: Exception) { fail() }
            if (uri.scheme != if (connect) "https" else "http") fail()
            if (uri.rawFragment != null || uri.userInfo != null) fail()
            val authority = uri.rawAuthority ?: fail()
            val host = uri.host?.trim('[', ']') ?: fail()
            val suffix = if (authority.startsWith('[')) authority.substringAfter(']', "invalid")
                else authority.substringAfter(host, "invalid")
            val explicitPort = suffix.startsWith(':')
            if (suffix.isNotEmpty() && !explicitPort) fail()
            val port = if (explicitPort) {
                val raw = suffix.drop(1)
                if (!Regex("^[0-9]+$").matches(raw)) fail()
                raw.toIntOrNull()?.takeIf { it in 1..65535 } ?: fail()
            } else if (connect) fail() else 80
            if (connect && (!uri.rawPath.isNullOrEmpty() || uri.rawQuery != null)) fail()
            val validation = "http://$authority/"
            if (!LinkUtils.isPublicHttpUrl(validation)) fail()
            val octets = host.split('.')
            val numeric = octets.size == 4 && octets.all { Regex("^(?:[0-9]+|0x[0-9a-fA-F]+)$").matches(it) }
            if (numeric && octets.any { it.startsWith("0x", true) || (it.length > 1 && it.startsWith('0')) }) fail()
            if (':' in host || numeric) {
                val address = runCatching { InetAddress.getByName(host) }.getOrElse { fail() }
                if (!PublicDns.isPublic(address)) fail()
            }
            val path = if (connect) "" else uri.rawPath.orEmpty().ifEmpty { "/" } +
                (uri.rawQuery?.let { "?$it" } ?: "")
            return ProxyDestination(host, port, connect, path, authority)
        }
    }
}
