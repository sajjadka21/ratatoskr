package app.ratatoskr.android

import android.content.ContentValues
import android.content.Context
import android.os.Environment
import android.provider.MediaStore
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import java.io.File
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap

data class SavedMedia(val uri: String, val name: String, val mime: String)

/** Native mobile backend. Workspaces survive pause, failure and process death. */
object Engine {
    @Volatile private var ready = false
    private val cache = ConcurrentHashMap<String, Pair<Long, LinkInfo>>()
    @Synchronized fun init(context: Context) {
        if (ready) return
        YoutubeDL.getInstance().init(context.applicationContext)
        FFmpeg.getInstance().init(context.applicationContext)
        ready = true
    }
    fun probe(context: Context, url: String, processId: String? = null, check: () -> Unit = {}): LinkInfo {
        val canonical = SafeHttp.canonicalSource(url, check)
        cache[canonical]?.takeIf { System.currentTimeMillis() - it.first < 120000 }?.let { return it.second }
        check(); init(context)
        val request = YoutubeDLRequest(canonical).apply {
            addOption("--dump-single-json"); addOption("--skip-download"); addOption("--ignore-no-formats-error")
            addOption("--yes-playlist"); addOption("--playlist-end", "51"); addOption("--no-warnings")
            addOption("--socket-timeout", "15"); addOption("--no-cache-dir")
        }
        val response = SafeProxy(check).use { proxy ->
            request.addOption("--proxy", proxy.url)
            YoutubeDL.getInstance().execute(request, processId)
        }
        check()
        val info = MediaMetadata.parse(canonical, response.out)
        if (cache.size >= 10) cache.clear()
        cache[canonical] = System.currentTimeMillis() to info
        return info
    }
    fun work(context: Context, id: String) = File(context.filesDir, "download-work/$id")
    fun discard(context: Context, id: String) { work(context, id).deleteRecursively() }

    fun download(context: Context, task: MobileTask, control: TransferControl,
                 onState: (TaskState) -> Unit, onProgress: (Float) -> Unit): List<SavedMedia> {
        val store = TaskStore.get(context)
        if (task.kind == "file") {
            onState(TaskState.DOWNLOADING)
            val file = DirectDownload.fetch(context, task, task.url, control, onProgress)
            control.check(); onState(TaskState.SAVING)
            val result = publish(context, task.id, 1, file, control)
            discard(context, task.id)
            return listOf(result)
        }
        onState(TaskState.PROBING)
        val info = probe(context, task.url, task.id, control::check)
        store.update(task.id, ContentValues().apply { put("title", info.title) })
        val selected = task.selectedItems.split(',').mapNotNull { it.toIntOrNull() }.toSet()
        val items = info.items.filter { (selected.isEmpty() || it.index in selected) && (!task.audioOnly || it.kind != "photo") }
        if (items.isEmpty()) throw TransferFailure("unsupported_media")
        val results = mutableListOf<SavedMedia>()
        items.forEachIndexed { position, item ->
            control.check()
            val previous = store.outputAt(task.id, item.index)
            if (previous != null && runCatching { context.contentResolver.openFileDescriptor(android.net.Uri.parse(previous.uri), "r")?.use { true } ?: false }.getOrDefault(false)) {
                results.add(previous)
                return@forEachIndexed
            }
            val directory = File(work(context, task.id), "item-${item.index}").apply { mkdirs() }
            onState(TaskState.DOWNLOADING)
            val progress: (Float) -> Unit = { value -> control.check(); onProgress((position * 100f + value.coerceIn(0f, 100f)) / items.size) }
            val file = if (item.kind == "photo") {
                DirectDownload.fetch(context, task.copy(fileName = "${item.index}-${LinkUtils.safeFileName(item.title, "photo")}.jpg"),
                    item.downloadUrl!!, control, progress, directory)
            } else {
                ensureSpace(directory, 32L * 1024 * 1024)
                val request = YoutubeDLRequest(info.url).apply {
                    addOption("--yes-playlist"); addOption("--playlist-items", item.index.toString())
                    addOption("--socket-timeout", "15"); addOption("--retries", "3"); addOption("--fragment-retries", "3")
                    addOption("--continue"); addOption("--no-cache-dir"); addOption("--no-warnings")
                    addOption("-o", File(directory, "%(title).80s [%(id)s].%(ext)s").absolutePath)
                    val rate = MobilePreferences(context).speedLimit
                    if (rate > 0) addOption("--limit-rate", rate.toString())
                    // Restrict every fallback to protocols that use the guarded native transport.
                    val guardedFormat = MediaOptions.format(task.height, task.audioOnly || item.kind == "audio")
                        .split('/').joinToString("/") { fallback -> fallback.split('+').joinToString("+") {
                            it + "[protocol~='^(https?|m3u8_native|http_dash_segments)$']"
                        } }
                    addOption("-f", guardedFormat)
                    if (task.audioOnly || item.kind == "audio") { addOption("-x"); addOption("--audio-format", "m4a") }
                    else addOption("--merge-output-format", "mp4")
                }
                SafeProxy(control::check).use { proxy ->
                    request.addOption("--proxy", proxy.url)
                    request.addOption("--downloader", "native")
                    YoutubeDL.getInstance().execute(request, task.id) { value, _, _ -> progress(value.coerceAtMost(98f)) }
                }
                control.check(); onState(TaskState.MERGING)
                val files = directory.listFiles().orEmpty().filter { it.isFile && it.length() > 0 &&
                    it.extension.lowercase() in setOf("mp4", "m4a", "mp3", "webm", "mkv", "opus", "ogg", "aac", "wav", "flac", "mov") }
                val output = if (task.audioOnly || item.kind == "audio") files.singleOrNull { it.extension.equals("m4a", true) } else files.singleOrNull()
                output ?: throw TransferFailure("invalid_output")
            }
            control.check(); onState(TaskState.SAVING)
            results.add(publish(context, task.id, item.index, file, control))
            directory.deleteRecursively()
            onProgress((position + 1) * 100f / items.size)
        }
        discard(context, task.id)
        return results
    }
    fun cancel(processId: String) { if (ready) runCatching { YoutubeDL.getInstance().destroyProcessById(processId) } }
    fun ensureSpace(directory: File, needed: Long) {
        directory.mkdirs()
        if (directory.usableSpace < needed + 16L * 1024 * 1024) throw TransferFailure("no_space")
    }
    /** Journal the pending URI before copying so an interrupted save can be cleaned. */
    private fun publish(context: Context, id: String, index: Int, file: File, control: TransferControl): SavedMedia {
        if (file.length() == 0L) throw TransferFailure("invalid_output")
        ensureSpace(context.filesDir, file.length())
        val name = LinkUtils.safeFileName(file.name)
        val mime = android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension.lowercase()) ?: "application/octet-stream"
        val images = mime.startsWith("image/")
        val collection = if (images) MediaStore.Images.Media.EXTERNAL_CONTENT_URI else MediaStore.Downloads.EXTERNAL_CONTENT_URI
        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, name); put(MediaStore.MediaColumns.MIME_TYPE, mime)
            put(MediaStore.MediaColumns.RELATIVE_PATH, "${if (images) Environment.DIRECTORY_PICTURES else Environment.DIRECTORY_DOWNLOADS}/Ratatoskr")
            put(MediaStore.MediaColumns.IS_PENDING, 1)
        }
        val resolver = context.contentResolver
        val journal = File(work(context, id), "pending-$index.uri")
        if (journal.exists()) {
            val old = android.net.Uri.parse(journal.readText())
            val committed = runCatching { resolver.query(old, arrayOf(MediaStore.MediaColumns.IS_PENDING, MediaStore.MediaColumns.DISPLAY_NAME, MediaStore.MediaColumns.MIME_TYPE), null, null, null)?.use {
                if (it.moveToFirst() && it.getInt(0) == 0) SavedMedia(old.toString(), it.getString(1), it.getString(2)) else null
            } }.getOrNull()
            if (committed != null) { TaskStore.get(context).recordOutput(id, index, committed); journal.delete(); return committed }
            runCatching { resolver.delete(old, null, null) }; journal.delete()
        }
        val target = resolver.insert(collection, values) ?: throw TransferFailure("no_space")
        journal.parentFile?.mkdirs(); journal.writeText(target.toString())
        try {
            resolver.openOutputStream(target)?.use { output -> file.inputStream().use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) { control.check(); val count = input.read(buffer); if (count < 0) break; output.write(buffer, 0, count) }
                output.flush()
            } } ?: throw IOException("cannot_write")
            control.check()
            values.clear(); values.put(MediaStore.MediaColumns.IS_PENDING, 0)
            if (resolver.update(target, values, null, null) != 1) throw TransferFailure("cannot_write")
            val result = SavedMedia(target.toString(), name, mime)
            TaskStore.get(context).recordOutput(id, index, result)
            journal.delete()
            return result
        } catch (error: Exception) {
            runCatching { resolver.delete(target, null, null) }; journal.delete()
            throw error
        }
    }
    fun update(context: Context): Boolean { init(context); return YoutubeDL.getInstance().updateYoutubeDL(context, YoutubeDL.UpdateChannel.STABLE) != null }
}
