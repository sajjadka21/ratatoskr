package app.ratatoskr.android

import android.content.ContentValues
import android.content.Context
import android.os.Environment
import android.provider.MediaStore
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import java.io.File

/** What the share sheet needs to know about a link before a quality is chosen. */
data class LinkInfo(val title: String, val heights: List<Int>, val hasVideo: Boolean)

/** One place that talks to yt-dlp, so the screens stay simple. */
object Engine {
    @Volatile private var ready = false

    @Synchronized
    fun init(context: Context) {
        if (ready) return
        YoutubeDL.getInstance().init(context.applicationContext)
        FFmpeg.getInstance().init(context.applicationContext)
        ready = true
    }

    /** Reads a link's title and available heights without downloading it. */
    fun probe(context: Context, url: String): LinkInfo {
        init(context)
        val request = YoutubeDLRequest(url).apply {
            addOption("--no-playlist")
            addOption("--playlist-items", "1")
            addOption("--socket-timeout", "30")
        }
        val info = YoutubeDL.getInstance().getInfo(request)
        val heights = info.formats.orEmpty().map { it.height.takeIf { h -> h > 0 } }
        return LinkInfo(
            title = info.title.orEmpty(),
            heights = LinkUtils.offeredHeights(heights),
            hasVideo = info.formats.orEmpty().any { it.vcodec != null && it.vcodec != "none" },
        )
    }

    /**
     * Downloads to the app's cache, then moves the file into Downloads/Ratatosk
     * with MediaStore, so no storage permission is needed. Returns the file name.
     * [onProgress] gets 0..100. [processId] lets the download be cancelled.
     */
    fun download(
        context: Context,
        url: String,
        height: Int?,
        audioOnly: Boolean,
        processId: String,
        onProgress: (Float) -> Unit,
    ): String {
        init(context)
        val work = File(context.cacheDir, "dl-$processId").apply { mkdirs() }
        try {
            val request = YoutubeDLRequest(url).apply {
                addOption("--no-playlist")
                addOption("--playlist-items", "1")
                addOption("--socket-timeout", "30")
                addOption("--retries", "3")
                addOption("-o", File(work, "%(title).80s [%(id)s].%(ext)s").absolutePath)
                if (audioOnly) {
                    addOption("-f", "bestaudio[ext=m4a]/bestaudio/best")
                } else {
                    addOption("-f", LinkUtils.videoFormat(height))
                    addOption("--merge-output-format", "mp4")
                }
            }
            YoutubeDL.getInstance().execute(request, processId) { progress, _, _ -> onProgress(progress) }
            val file = work.listFiles()
                ?.filter { it.isFile && !it.name.endsWith(".part") && !it.name.endsWith(".ytdl") }
                ?.maxByOrNull { it.length() }
                ?: error("nothing downloaded")
            return save(context, file, audioOnly)
        } finally {
            work.deleteRecursively()
        }
    }

    fun cancel(processId: String) {
        YoutubeDL.getInstance().destroyProcessById(processId)
    }

    private fun save(context: Context, file: File, audioOnly: Boolean): String {
        val name = LinkUtils.safeFileName(file.name)
        val mime = when (file.extension.lowercase()) {
            "mp4" -> "video/mp4"
            "m4a" -> "audio/mp4"
            "mp3" -> "audio/mpeg"
            "webm" -> if (audioOnly) "audio/webm" else "video/webm"
            "mkv" -> "video/x-matroska"
            "opus", "ogg" -> "audio/ogg"
            else -> "application/octet-stream"
        }
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, name)
            put(MediaStore.Downloads.MIME_TYPE, mime)
            put(MediaStore.Downloads.RELATIVE_PATH, "${Environment.DIRECTORY_DOWNLOADS}/Ratatosk")
            put(MediaStore.Downloads.IS_PENDING, 1)
        }
        val resolver = context.contentResolver
        val target = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
            ?: error("could not create the file")
        resolver.openOutputStream(target)?.use { out -> file.inputStream().use { it.copyTo(out) } }
            ?: error("could not write the file")
        values.clear()
        values.put(MediaStore.Downloads.IS_PENDING, 0)
        resolver.update(target, values, null, null)
        return name
    }

    /** Fetches the newest yt-dlp; sites change often and old copies stop working. */
    fun update(context: Context): Boolean {
        init(context)
        return YoutubeDL.getInstance().updateYoutubeDL(context, YoutubeDL.UpdateChannel.STABLE) != null
    }
}
