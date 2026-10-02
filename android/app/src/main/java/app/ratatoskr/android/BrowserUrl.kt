package app.ratatoskr.android

import java.net.URLEncoder

/** What the address bar of the built-in browser does with what was typed. Pure, so it runs as a unit test. */
object BrowserUrl {
    private val domain = Regex("""^[\p{L}\p{N}-]+(\.[\p{L}\p{N}-]+)+(:\d+)?([/?#]\S*)?$""")
    private val mediaTypes = setOf("mp4", "webm", "mkv", "mp3", "m4a", "flac", "ogg", "aac", "wav", "mov")
    private val streamTypes = setOf("m3u8", "mpd")

    fun normalize(input: String): String {
        val text = input.trim()
        return when {
            text.isEmpty() -> ""
            text.startsWith("http://", true) || text.startsWith("https://", true) -> text
            domain.matches(text) -> "https://$text"
            else -> "https://duckduckgo.com/?q=" + URLEncoder.encode(text, "UTF-8")
        }
    }

    /** True for an address the page itself loaded that is a media file or a stream playlist worth offering. */
    fun isMedia(url: String): Boolean = LinkPlan.extension(url).let { it in mediaTypes || it in streamTypes }
    /** Playlists (`.m3u8`, `.mpd`) need the video engine; plain files do not. */
    fun isStream(url: String): Boolean = LinkPlan.extension(url) in streamTypes
}
