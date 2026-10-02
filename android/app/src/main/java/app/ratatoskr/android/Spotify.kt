package app.ratatoskr.android

import java.io.InputStream
import java.net.URI

data class SpotifyTrack(val title: String, val artist: String) {
    val display get() = if (artist.isEmpty()) title else "$artist - $title"
    val query get() = display
}

/** Spotify audio is DRM-protected, so a track link only supplies the name; the audio comes from YouTube. */
object Spotify {
    private val track = Regex("^/(?:intl-[a-z]{2,5}(?:-[a-z]{2,4})?/)?track/[A-Za-z0-9]{10,30}/?$")

    fun isTrackUrl(url: String): Boolean {
        val uri = runCatching { URI(url) }.getOrNull() ?: return false
        return uri.scheme?.lowercase() in setOf("http", "https") &&
            uri.host?.lowercase() == "open.spotify.com" && track.matches(uri.path.orEmpty())
    }

    fun readLimited(stream: InputStream, limit: Int = 512 * 1024): String = LinkUtils.readText(stream, limit)

    /** Reads the public page's Open Graph tags: og:title is the song, og:description starts with the artist. */
    fun parsePage(html: String): SpotifyTrack? {
        fun meta(property: String): String? {
            val tag = Regex("<meta\\s+[^>]*>", RegexOption.IGNORE_CASE).findAll(html).map { it.value }.firstOrNull {
                Regex("(?:property|name)\\s*=\\s*[\"']$property[\"']", RegexOption.IGNORE_CASE).containsMatchIn(it)
            } ?: return null
            return Regex("content\\s*=\\s*\"([^\"]*)\"|content\\s*=\\s*'([^']*)'", RegexOption.IGNORE_CASE).find(tag)
                ?.let { it.groupValues[1].ifEmpty { it.groupValues[2] } }?.let(::unescape)
        }
        val title = clean(meta("og:title") ?: return null)
        if (title.isEmpty()) return null
        val artist = meta("og:description")?.split('·', '•')?.map { it.trim() }?.takeIf { it.size >= 2 }?.first()?.let(::clean).orEmpty()
        return SpotifyTrack(title, artist)
    }

    private fun clean(value: String) = value.replace(Regex("[\\u0000-\\u001f]"), " ").trim().take(150)

    private fun unescape(value: String) = value
        .replace(Regex("&#x([0-9a-fA-F]+);")) { String(Character.toChars(it.groupValues[1].toInt(16))) }
        .replace(Regex("&#([0-9]+);")) { String(Character.toChars(it.groupValues[1].toInt())) }
        .replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}
