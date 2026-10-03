package app.ratatoskr.android

import java.net.URI

enum class LinkKind { FILE, MEDIA }

/** Decides, without any network request, how a pasted link should be fetched. Pure, so it is unit tested. */
object LinkPlan {
    private val mediaHosts = listOf(
        "youtube.com", "youtu.be", "instagram.com", "spotify.com", "aparat.com", "tiktok.com", "twitter.com", "x.com",
        "facebook.com", "fb.watch", "soundcloud.com", "vimeo.com", "dailymotion.com", "reddit.com", "twitch.tv",
        "bandcamp.com", "namava.ir", "telewebion.com",
    )
    private val fileExtensions = setOf(
        "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "apk", "xapk", "exe", "msi", "dmg", "deb", "pdf", "epub",
        "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "csv", "mp3", "m4a", "flac", "wav", "ogg", "mp4", "mkv",
        "avi", "mov", "webm", "jpg", "jpeg", "png", "gif", "webp", "bin", "img", "torrent",
    )
    private const val MAX_LINKS = 200

    fun host(url: String): String = runCatching { URI(url).host.lowercase().removePrefix("www.") }.getOrDefault("")

    fun isMediaHost(url: String): Boolean = host(url).let { h -> mediaHosts.any { h == it || h.endsWith(".$it") } }

    fun extension(url: String): String = runCatching { URI(url).path.substringAfterLast('/').substringAfterLast('.', "").lowercase() }.getOrDefault("")

    /** Media sites go to the video engine; a path ending in a known file type is a plain file;
     * extensionless endpoints try HTTP first; HTML can fall back once to the media engine. */
    fun classify(url: String): LinkKind = when {
        isMediaHost(url) -> LinkKind.MEDIA
        extension(url) in setOf("m3u8", "mpd") -> LinkKind.MEDIA
        else -> LinkKind.FILE
    }

    /** Only unsupported generic pages may fall back to guarded HTTP. Never
     * reinterpret authentication, network or media-site failures as a file. */
    fun mayTryMedia(url: String, errorCode: String) = !isMediaHost(url) && extension(url) !in fileExtensions && errorCode == "not_a_file"

    fun mayTryFile(url: String, errorCode: String) = !isMediaHost(url) && errorCode == "unsupported_media"

    /** Every link in the text, with `[1-10]` / `[01-10]` ranges in an address expanded. */
    fun parse(text: String?): List<String> = LinkUtils.extractUrls(expand(text.orEmpty()), MAX_LINKS).filterNot { range.containsMatchIn(it) }.take(MAX_LINKS)

    private val range = Regex("\\[(\\d{1,6})-(\\d{1,6})]")

    fun expand(text: String): String = text.split(Regex("(?<=\\s)|(?=\\s)")).joinToString("") { token ->
        val match = range.find(token)
        if (match == null || !token.startsWith("http", true)) token
        else {
            val from = match.groupValues[1]; val to = match.groupValues[2]
            val low = from.toLong(); val high = to.toLong()
            if (high < low || high - low >= MAX_LINKS) token
            else (low..high).joinToString("\n") { n -> token.replaceFirst(match.value, n.toString().padStart(from.length, '0')) }
        }
    }

    data class Summary(val files: Int, val media: Int)
    fun summarize(urls: List<String>) = Summary(urls.count { classify(it) == LinkKind.FILE }, urls.count { classify(it) == LinkKind.MEDIA })
}

/** Folder inside Downloads/Ratatoskr, like the category tabs of other download managers. */
object FileCategory {
    fun folder(fileName: String, mime: String): String {
        val extension = fileName.substringAfterLast('.', "").lowercase()
        return when {
            mime.startsWith("video/") || extension in setOf("mp4", "mkv", "avi", "mov", "webm", "ts") -> "Video"
            mime.startsWith("audio/") || extension in setOf("mp3", "m4a", "flac", "wav", "ogg", "opus", "aac") -> "Music"
            extension in setOf("zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso") -> "Archives"
            extension in setOf("apk", "xapk", "exe", "msi", "dmg", "deb") -> "Programs"
            mime.startsWith("image/") -> ""
            extension in setOf("pdf", "epub", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "csv") -> "Documents"
            else -> "Other"
        }
    }
}
