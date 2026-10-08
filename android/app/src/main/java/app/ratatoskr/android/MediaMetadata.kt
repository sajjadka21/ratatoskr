package app.ratatoskr.android

import org.json.JSONObject
import java.net.URI

data class MediaItem(
    val index: Int, val title: String, val kind: String, val heights: List<Int>,
    val downloadUrl: String? = null, val thumbnail: String? = null,
)
data class LinkInfo(
    val title: String, val heights: List<Int>, val hasVideo: Boolean,
    val items: List<MediaItem> = emptyList(), val url: String = "", val truncated: Boolean = false,
    val qualities: List<MediaQualityOption> = emptyList(), val audioBytes: Long? = null,
    val durationSeconds: Long? = null,
)
data class MediaQualityOption(val height: Int, val bytes: Long?)

/** Only Instagram's photo metadata can turn thumbnail candidates into originals.
 * A video poster or another site's preview is never a downloadable photo. */
object MediaMetadata {
    fun parse(url: String, json: String): LinkInfo {
        require(json.length <= 4 * 1024 * 1024) { "metadata_too_large" }
        val canonical = LinkUtils.canonicalUrl(url)
        val instagram = URI(canonical).host in setOf("www.instagram.com", "instagram.com", "m.instagram.com")
        val root = JSONObject(json)
        val entries = root.optJSONArray("entries")
        val count = entries?.length() ?: 1
        val items = (0 until minOf(count, 50)).map { position ->
            val item = if (entries == null) root else entries.optJSONObject(position) ?: error("unsupported_media")
            val formats = item.optJSONArray("formats")
            val rows = (0 until (formats?.length() ?: 0)).mapNotNull { formats?.optJSONObject(it) }
            val direct = LinkUtils.isPublicHttpUrl(item.optString("url"))
            val videoExts = setOf("mp4", "webm", "mkv", "mov", "m4v")
            val audioExts = setOf("m4a", "mp3", "aac", "opus", "ogg", "wav", "flac")
            val video = (direct && item.optString("ext") in videoExts) || rows.any { it.optString("vcodec").let { codec -> codec.isNotEmpty() && codec != "none" } || (it.optString("vcodec").isEmpty() && it.optString("ext") in videoExts && LinkUtils.isPublicHttpUrl(it.optString("url"))) }
            val audio = (direct && item.optString("ext") in audioExts) || rows.any { it.optString("acodec").let { codec -> codec.isNotEmpty() && codec != "none" } }
            val thumbs = item.optJSONArray("thumbnails")
            val candidates = (0 until (thumbs?.length() ?: 0)).mapNotNull { thumbs?.optJSONObject(it) }
                .filter { LinkUtils.isPublicHttpUrl(it.optString("url")) }
            val image = candidates.maxByOrNull { it.optLong("width").coerceAtLeast(0) * it.optLong("height").coerceAtLeast(0) }
            val thumbnail = image?.optString("url") ?: item.optString("thumbnail").takeIf { LinkUtils.isPublicHttpUrl(it) }
            val kind = when { video -> "video"; audio -> "audio"; instagram && image != null && rows.isEmpty() && item.optString("ext") !in videoExts && item.optString("ext") !in audioExts -> "photo"; else -> error("unsupported_media") }
            MediaItem(position + 1, item.optString("title", root.optString("title")).take(300), kind,
                LinkUtils.offeredHeights(rows.map { it.optInt("height").takeIf { height -> height > 0 } }),
                if (kind == "photo") image!!.getString("url") else null, thumbnail)
        }
        require(items.isNotEmpty()) { "unsupported_media" }
        val duration = root.optDouble("duration", Double.NaN).takeIf { it.isFinite() && it > 0 }
        val rootFormats = root.optJSONArray("formats")
        val formats = (0 until (rootFormats?.length() ?: 0)).mapNotNull { rootFormats?.optJSONObject(it) }
        fun hasPicture(format: org.json.JSONObject): Boolean = when (format.optString("vcodec")) {
            "none" -> false
            "" -> format.optInt("height") > 0
            else -> true
        }
        fun mayHaveSound(format: org.json.JSONObject) = format.optString("acodec") != "none"
        fun hasSound(format: org.json.JSONObject) = format.optString("acodec").let { it.isNotEmpty() && it != "none" }
        fun soundOnly(format: org.json.JSONObject) = hasSound(format) && !hasPicture(format)
        fun rate(format: org.json.JSONObject) = format.optDouble("tbr", 0.0).takeIf { it.isFinite() && it > 0 } ?: 0.0
        fun size(format: org.json.JSONObject): Long? {
            val exact = format.optDouble("filesize", Double.NaN).takeIf { it.isFinite() && it > 0 }
            val approximate = format.optDouble("filesize_approx", Double.NaN).takeIf { it.isFinite() && it > 0 }
            val estimated = duration?.let { seconds -> rate(format).takeIf { it > 0 }?.let { it * 1000.0 / 8.0 * seconds } }
            return (exact ?: approximate ?: estimated)?.takeIf { it.isFinite() && it > 0 }?.toLong()
        }
        fun best(rows: List<org.json.JSONObject>) = rows.maxByOrNull(::rate)
        val audioBytes = best(formats.filter(::soundOnly))?.let(::size)
        val usableFormats = formats.filter(::hasPicture)
        val qualities = LinkUtils.offeredHeights(usableFormats.map { it.optInt("height").takeIf { height -> height > 0 } }).map { height ->
            val pictureBytes = best(usableFormats.filter { it.optInt("height") == height })?.let(::size)
            val hasMuxedSound = best(usableFormats.filter { it.optInt("height") == height })?.let(::mayHaveSound) == true
            MediaQualityOption(height, pictureBytes?.let { if (hasMuxedSound) it else it + (audioBytes ?: 0L) })
        }
        return LinkInfo(root.optString("title").take(300), LinkUtils.offeredHeights(items.flatMap { it.heights }),
            items.any { it.kind == "video" }, items, canonical, count > 50, qualities, audioBytes,
            duration?.toLong())
    }
}

