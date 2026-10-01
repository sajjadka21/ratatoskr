package app.ratatoskr.android

import java.net.InetAddress
import java.net.URI
import java.net.URLDecoder

/** Pure helpers with no Android classes, so they run as plain unit tests. */
object LinkUtils {
    private val urlRegex = Regex("""https?://[^\s<>"']+""", RegexOption.IGNORE_CASE)
    private const val TRAILING = ".,;:!?)]}»"

    /** The first web address in shared text, without trailing punctuation. */
    fun extractUrl(text: String?): String? =
        extractUrls(text).firstOrNull()

    /** True for http(s) addresses that do not point at the phone's own network. */
    fun isPublicHttpUrl(url: String): Boolean {
        val uri = try { URI(url) } catch (e: Exception) { return false }
        if (uri.scheme?.lowercase() !in listOf("http", "https")) return false
        if (uri.userInfo != null) return false
        val keys = try { uri.rawQuery.orEmpty().split('&').map { URLDecoder.decode(it.substringBefore('='), "UTF-8").lowercase() } }
        catch (e: IllegalArgumentException) { return false }
        if (keys.any { it in setOf("access_token", "authorization", "password", "sessionid", "cookie") }) return false
        val host = uri.host?.lowercase()?.trimEnd('.') ?: return false
        if (uri.port != -1 && uri.port !in 1..65535) return false
        if (host.startsWith("0x") || (host.all { it.isDigit() || it == '.' } &&
            !Regex("^(?:0|[1-9][0-9]{0,2})(?:\\.(?:0|[1-9][0-9]{0,2})){3}$").matches(host))) return false
        if (host == "localhost" || listOf(".local", ".internal", ".localhost", ".lan").any { host.endsWith(it) }) {
            return false
        }
        val literal = Regex("""^(\d{1,3}(\.\d{1,3}){3}|\[?[0-9a-f:.]+\]?)$""").matches(host)
        if (literal) {
            val address = try { InetAddress.getByName(host.trim('[', ']')) } catch (e: Exception) { return false }
            val bytes = address.address.map { it.toInt() and 255 }
            if (bytes.size == 4 && (bytes[0] == 0 || bytes[0] >= 224 ||
                (bytes[0] == 100 && bytes[1] in 64..127) || (bytes[0] == 198 && bytes[1] in 18..19))) return false
            return !(address.isAnyLocalAddress || address.isLoopbackAddress || address.isLinkLocalAddress ||
                address.isSiteLocalAddress || address.isMulticastAddress ||
                // unique-local IPv6 (fc00::/7)
                (address.address.size == 16 && (address.address[0].toInt() and 0xFE) == 0xFC))
        }
        return '.' in host
    }

    /** Decode exactly once, then apply the same destination policy as shared links. */
    fun handoffUrl(payload: String?): String? {
        if (payload == null || payload.toByteArray(Charsets.UTF_8).size > 2000) return null
        val uri = try { URI(payload) } catch (e: Exception) { return null }
        if (uri.scheme != "ratatoskr" || uri.host != "add" || uri.port != -1 ||
            uri.userInfo != null || !uri.rawPath.isNullOrEmpty() || uri.rawFragment != null) return null
        val query = uri.rawQuery ?: return null
        if (!query.startsWith("url=") || '&' in query) return null
        val url = try { URLDecoder.decode(query.substring(4), "UTF-8") } catch (e: Exception) { return null }
        return url.takeIf { isPublicHttpUrl(it) }
    }

    /** Label the actual source heights; never advertise 480p for a 540p source. */
    fun offeredHeights(available: List<Int?>): List<Int> =
        available.filterNotNull().filter { it > 0 }
            .distinct().sortedDescending().take(4)

    fun videoFormat(height: Int?): String =
        if (height == null || height <= 0) "bv*+ba/b" else "bv*[height<=$height]+ba/b[height<=$height]"

    fun extractUrls(text: String?): List<String> = urlRegex.findAll(text.orEmpty())
        .map { match ->
            var value = if ('?' in match.value) match.value else match.value.trimEnd { c -> c in TRAILING }
            for ((closing, opening) in listOf(')' to '(', ']' to '[', '}' to '{')) {
                while (value.endsWith(closing) && value.count { it == closing } > value.count { it == opening }) value = value.dropLast(1)
            }
            value
        }.distinct().take(50).toList()

    fun contentIdentity(url: String): String {
        val canonical = canonicalUrl(url)
        val uri = URI(canonical)
        if (uri.host == "www.instagram.com") {
            val match = Regex("^/(?:p|reel|tv)/([A-Za-z0-9_-]+)/$").matchEntire(uri.path)
            if (match != null) return "instagram:${match.groupValues[1]}"
        }
        return canonical
    }

    fun canonicalUrl(url: String): String {
        require(isPublicHttpUrl(url)) { "bad_link" }
        val uri = URI(url)
        val host = uri.host.lowercase().trimEnd('.')
        if (host in setOf("instagram.com", "www.instagram.com", "m.instagram.com")) {
            val match = Regex("(?:^|/)(p|reel|reels|tv)/([A-Za-z0-9_-]+)/?").find(uri.path)
            if (match != null) return "https://www.instagram.com/${if (match.groupValues[1] == "reels") "reel" else match.groupValues[1]}/${match.groupValues[2]}/"
        }
        return uri.toASCIIString().substringBefore('#')
    }

    fun safeFileName(name: String, fallback: String = "file"): String =
        name.replace(Regex("""[\\/:*?"<>|\u0000-\u001f]"""), "_").trim(' ', '.').take(120).ifEmpty { fallback }
}
