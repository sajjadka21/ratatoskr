package app.ratatoskr.android

import java.net.InetAddress
import java.net.URI

/** Pure helpers with no Android classes, so they run as plain unit tests. */
object LinkUtils {
    private val urlRegex = Regex("""https?://[^\s<>"']+""", RegexOption.IGNORE_CASE)
    private const val TRAILING = ".,;:!?)]}»"
    private val standardHeights = listOf(2160, 1440, 1080, 720, 480, 360, 240)

    /** The first web address in shared text, without trailing punctuation. */
    fun extractUrl(text: String?): String? =
        urlRegex.find(text.orEmpty())?.value?.trimEnd { it in TRAILING }

    /** True for http(s) addresses that do not point at the phone's own network. */
    fun isPublicHttpUrl(url: String): Boolean {
        val uri = try { URI(url) } catch (e: Exception) { return false }
        if (uri.scheme?.lowercase() !in listOf("http", "https")) return false
        val host = uri.host?.lowercase()?.trimEnd('.') ?: return false
        if (host == "localhost" || listOf(".local", ".internal", ".localhost", ".lan").any { host.endsWith(it) }) {
            return false
        }
        val literal = Regex("""^(\d{1,3}(\.\d{1,3}){3}|\[?[0-9a-f:]+\]?)$""").matches(host)
        if (literal) {
            val address = try { InetAddress.getByName(host.trim('[', ']')) } catch (e: Exception) { return false }
            return !(address.isAnyLocalAddress || address.isLoopbackAddress || address.isLinkLocalAddress ||
                address.isSiteLocalAddress || address.isMulticastAddress ||
                // unique-local IPv6 (fc00::/7)
                (address.address.size == 16 && (address.address[0].toInt() and 0xFE) == 0xFC))
        }
        return '.' in host
    }

    /** Heights to offer, from what the video has: each rounded down to a standard height, top four. */
    fun offeredHeights(available: List<Int?>): List<Int> =
        available.filterNotNull().filter { it > 0 }
            .map { h -> standardHeights.firstOrNull { it <= h } ?: h }
            .distinct().sortedDescending().take(4)

    fun videoFormat(height: Int?): String =
        if (height == null || height <= 0) "bv*+ba/b" else "bv*[height<=$height]+ba/b[height<=$height]/b"

    fun safeFileName(name: String, fallback: String = "file"): String =
        name.replace(Regex("""[\\/:*?"<>|\u0000-\u001f]"""), "_").trim(' ', '.').take(120).ifEmpty { fallback }
}
