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
        if (Spotify.isTrackUrl(canonical)) return probeSpotify(context, canonical, processId, check)
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
    /** Spotify streams are DRM-protected and cannot be fetched. Like the desktop app and the bot,
     * read the track's name from the public page and download the matching audio from YouTube. */
    private fun probeSpotify(context: Context, spotifyUrl: String, processId: String?, check: () -> Unit): LinkInfo {
        cache[spotifyUrl]?.takeIf { System.currentTimeMillis() - it.first < 120000 }?.let { return it.second }
        val page = SafeHttp.open(spotifyUrl, check = check)
        val html = try {
            SafeHttp.requireSuccess(page.responseCode)
            Spotify.readLimited(page.inputStream)
        } finally { page.disconnect() }
        val track = Spotify.parsePage(html) ?: throw TransferFailure("unsupported_media")
        check(); init(context)
        val search = "ytsearch1:${track.query}"
        val request = YoutubeDLRequest(search).apply {
            addOption("--dump-single-json"); addOption("--skip-download"); addOption("--ignore-no-formats-error")
            addOption("--no-warnings"); addOption("--socket-timeout", "15"); addOption("--no-cache-dir")
        }
        val response = SafeProxy(check).use { proxy ->
            request.addOption("--proxy", proxy.url)
            YoutubeDL.getInstance().execute(request, processId)
        }
        val parsed = MediaMetadata.parse(spotifyUrl, response.out)
        val info = parsed.copy(title = track.display, url = search, hasVideo = false,
            items = parsed.items.take(1).map { it.copy(title = track.display, kind = "audio") })
        cache[spotifyUrl] = System.currentTimeMillis() to info
        return info
    }
    fun work(context: Context, id: String) = File(context.filesDir, "download-work/$id")
    private fun isMediaRow(target: android.net.Uri): Boolean =
        target.scheme == "content" && target.authority == "media" && target.query == null && target.fragment == null &&
            Regex("/external(?:_primary)?/(?:downloads|images/media)/[0-9]+").matches(target.path.orEmpty())
    fun discard(context: Context, id: String) {
        val directory = work(context, id)
        var cleaned = true
        directory.listFiles().orEmpty().filter { Regex("pending-[0-9]+\\.uri").matches(it.name) }.forEach { journal ->
            try {
                val target = android.net.Uri.parse(journal.readText())
                if (isMediaRow(target)) {
                    val pending = context.contentResolver.query(target, arrayOf(MediaStore.MediaColumns.IS_PENDING,
                        MediaStore.MediaColumns.OWNER_PACKAGE_NAME), null, null, null)?.use {
                        it.moveToFirst() && it.getInt(0) == 1 && it.getString(1) == context.packageName
                    } ?: false
                    if (pending) context.contentResolver.delete(target, null, null)
                }
                journal.delete()
            } catch (_: Exception) {
                // Keep the journal for a later cleanup attempt; never remove published media.
                cleaned = false
            }
        }
        if (cleaned) directory.deleteRecursively()
    }

    fun download(context: Context, task: MobileTask, control: TransferControl,
                 onState: (TaskState) -> Unit, onProgress: (Float) -> Unit): List<SavedMedia> {
        val store = TaskStore.get(context)
        if (task.kind == "file") {
            onState(TaskState.DOWNLOADING)
            val prefs = MobilePreferences(context)
            val directory = work(context, task.id)
            val file = SegmentedDownload.fetch(directory, task.fileName, task.url, control, prefs.connections, prefs.speedLimit,
                { received, total ->
                    val done = if (total > 0) minOf(received, total) else received   // overlapping retries can briefly count extra
                    store.update(task.id, ContentValues().apply { put("bytes_done", done); put("total_bytes", total) })
                    onProgress(if (total > 0) done.toFloat() / total * 98f else 0f)
                },
                { DirectDownload.fetch(context, task, task.url, control, onProgress) })
            control.check(); onState(TaskState.SAVING)
            val result = publish(context, task.id, 1, file, control)
            discard(context, task.id)
            return listOf(result)
        }
        onState(TaskState.PROBING)
        val info = probe(context, task.url, task.id, control::check)
        val spotify = Spotify.isTrackUrl(task.url)
        val audioOnly = task.audioOnly || spotify
        store.update(task.id, ContentValues().apply { put("title", info.title) })
        val selected = task.selectedItems.split(',').mapNotNull { it.toIntOrNull() }.toSet()
        val items = info.items.filter { (selected.isEmpty() || it.index in selected) && (!audioOnly || it.kind != "photo") }
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
                    addOption("-o", File(directory, if (spotify) LinkUtils.safeFileName(info.title, "track").replace("%", "%%") + ".%(ext)s" else "%(title).80s [%(id)s].%(ext)s").absolutePath)
                    val rate = MobilePreferences(context).speedLimit
                    if (rate > 0) addOption("--limit-rate", rate.toString())
                    // Restrict every fallback to protocols that use the guarded native transport.
                    addOption("-f", MediaOptions.guardedFormat(task.height, audioOnly || item.kind == "audio"))
                    if (audioOnly || item.kind == "audio") { addOption("-x"); addOption("--audio-format", "m4a") }
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
                val output = if (audioOnly || item.kind == "audio") files.singleOrNull { it.extension.equals("m4a", true) } else files.singleOrNull()
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
        if (MobilePreferences(context).saveTree.isNotEmpty()) return publishToTree(context, id, index, file, control)
        if (android.os.Build.VERSION.SDK_INT < 29) return publishLegacy(context, id, index, file, control)
        ensureSpace(context.filesDir, file.length())
        val name = LinkUtils.safeFileName(Plugins.rename(PluginStore.active(context), file.name))
        val mime = android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension.lowercase()) ?: "application/octet-stream"
        val images = mime.startsWith("image/")
        val collection = if (images) MediaStore.Images.Media.EXTERNAL_CONTENT_URI else MediaStore.Downloads.EXTERNAL_CONTENT_URI
        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, name); put(MediaStore.MediaColumns.MIME_TYPE, mime)
            put(MediaStore.MediaColumns.RELATIVE_PATH, "${if (images) Environment.DIRECTORY_PICTURES else Environment.DIRECTORY_DOWNLOADS}/Ratatoskr" +
                if (images || !MobilePreferences(context).categoryFolders) "" else FileCategory.folder(name, mime).let { if (it.isEmpty()) "" else "/$it" })
            put(MediaStore.MediaColumns.IS_PENDING, 1)
        }
        val resolver = context.contentResolver
        val journal = File(work(context, id), "pending-$index.uri")
        if (journal.exists()) {
            val old = android.net.Uri.parse(journal.readText())
            if (!isMediaRow(old)) { journal.delete(); throw TransferFailure("invalid_output") }
            val committed = runCatching { resolver.query(old, arrayOf(MediaStore.MediaColumns.IS_PENDING, MediaStore.MediaColumns.DISPLAY_NAME, MediaStore.MediaColumns.MIME_TYPE), null, null, null)?.use {
                if (it.moveToFirst() && it.getInt(0) == 0) SavedMedia(old.toString(), it.getString(1), it.getString(2)) else null
            } }.getOrNull()
            if (committed != null) { TaskStore.get(context).recordOutput(id, index, committed); journal.delete(); return committed }
            runCatching { resolver.delete(old, null, null) }; journal.delete()
        }
        val target = resolver.insert(collection, values) ?: throw TransferFailure("no_space")
        journal.parentFile?.mkdirs(); journal.writeText(target.toString())
        var published = false
        try {
            resolver.openOutputStream(target)?.use { output -> file.inputStream().use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) { control.check(); val count = input.read(buffer); if (count < 0) break; output.write(buffer, 0, count) }
                output.flush()
            } } ?: throw IOException("cannot_write")
            control.check()
            values.clear(); values.put(MediaStore.MediaColumns.IS_PENDING, 0)
            if (resolver.update(target, values, null, null) != 1) throw TransferFailure("cannot_write")
            published = true
            val result = SavedMedia(target.toString(), name, mime)
            TaskStore.get(context).recordOutput(id, index, result)
            journal.delete()
            return result
        } catch (error: Exception) {
            if (!published) { runCatching { resolver.delete(target, null, null) }; journal.delete() }
            throw error
        }
    }
    /** Saves into the folder the user chose (Storage Access Framework), with the same category sub-folders. */
    private fun publishToTree(context: Context, id: String, index: Int, file: File, control: TransferControl): SavedMedia {
        val resolver = context.contentResolver
        val tree = android.net.Uri.parse(MobilePreferences(context).saveTree)
        val name = LinkUtils.safeFileName(Plugins.rename(PluginStore.active(context), file.name))
        val mime = android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension.lowercase()) ?: "application/octet-stream"
        var target: android.net.Uri? = null
        try {
            var parent = android.provider.DocumentsContract.buildDocumentUriUsingTree(tree, android.provider.DocumentsContract.getTreeDocumentId(tree))
            val category = if (!MobilePreferences(context).categoryFolders) "" else FileCategory.folder(name, mime)
            if (category.isNotEmpty()) parent = childFolder(context, tree, parent, category)
            target = android.provider.DocumentsContract.createDocument(resolver, parent, mime, name) ?: throw TransferFailure("cannot_write")
            resolver.openOutputStream(target)?.use { output -> file.inputStream().use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) { control.check(); val count = input.read(buffer); if (count < 0) break; output.write(buffer, 0, count) }
                output.flush()
            } } ?: throw TransferFailure("cannot_write")
            control.check()
            val shown = resolver.query(target, arrayOf(android.provider.DocumentsContract.Document.COLUMN_DISPLAY_NAME), null, null, null)?.use { if (it.moveToFirst()) it.getString(0) else null } ?: name
            val result = SavedMedia(target.toString(), shown, mime)
            TaskStore.get(context).recordOutput(id, index, result)
            return result
        } catch (error: SecurityException) {
            target?.let { runCatching { android.provider.DocumentsContract.deleteDocument(resolver, it) } }
            throw TransferFailure("storage_permission")
        } catch (error: Exception) {
            target?.let { runCatching { android.provider.DocumentsContract.deleteDocument(resolver, it) } }
            throw error
        }
    }
    /** The sub-folder called [name] inside [parent], created when it does not exist yet. */
    private fun childFolder(context: Context, tree: android.net.Uri, parent: android.net.Uri, name: String): android.net.Uri {
        val resolver = context.contentResolver
        val children = android.provider.DocumentsContract.buildChildDocumentsUriUsingTree(tree, android.provider.DocumentsContract.getDocumentId(parent))
        resolver.query(children, arrayOf(android.provider.DocumentsContract.Document.COLUMN_DOCUMENT_ID, android.provider.DocumentsContract.Document.COLUMN_DISPLAY_NAME, android.provider.DocumentsContract.Document.COLUMN_MIME_TYPE), null, null, null)?.use {
            while (it.moveToNext()) if (it.getString(1) == name && it.getString(2) == android.provider.DocumentsContract.Document.MIME_TYPE_DIR)
                return android.provider.DocumentsContract.buildDocumentUriUsingTree(tree, it.getString(0))
        }
        return android.provider.DocumentsContract.createDocument(resolver, parent, android.provider.DocumentsContract.Document.MIME_TYPE_DIR, name) ?: throw TransferFailure("cannot_write")
    }

    /** Android 8 and 9 have no per-app Downloads access: write into the public Downloads folder (the user
     * allowed storage once) and register the file so other apps can open and share it. */
    private fun publishLegacy(context: Context, id: String, index: Int, file: File, control: TransferControl): SavedMedia {
        if (context.checkSelfPermission(android.Manifest.permission.WRITE_EXTERNAL_STORAGE) != android.content.pm.PackageManager.PERMISSION_GRANTED)
            throw TransferFailure("storage_permission")
        val name = LinkUtils.safeFileName(Plugins.rename(PluginStore.active(context), file.name))
        val mime = android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension.lowercase()) ?: "application/octet-stream"
        val images = mime.startsWith("image/")
        val root = File(Environment.getExternalStoragePublicDirectory(if (images) Environment.DIRECTORY_PICTURES else Environment.DIRECTORY_DOWNLOADS), "Ratatoskr")
        val category = if (images || !MobilePreferences(context).categoryFolders) "" else FileCategory.folder(name, mime)
        val directory = if (category.isEmpty()) root else File(root, category)
        if (!directory.isDirectory && !directory.mkdirs()) throw TransferFailure("cannot_write")
        ensureSpace(directory, file.length())
        var target = File(directory, name)
        var copy = 1
        while (target.exists()) { target = File(directory, name.substringBeforeLast('.', name) + " ($copy)" + name.substringAfterLast('.', "").let { if (it.isEmpty()) "" else ".$it" }); copy++ }
        val temp = File(directory, target.name + ".ratatoskr-part")
        try {
            file.inputStream().use { input -> temp.outputStream().use { output ->
                val buffer = ByteArray(64 * 1024)
                while (true) { control.check(); val count = input.read(buffer); if (count < 0) break; output.write(buffer, 0, count) }
            } }
            control.check()
            if (!temp.renameTo(target)) throw TransferFailure("cannot_write")
        } catch (error: Exception) { temp.delete(); throw error }
        val values = ContentValues().apply {
            @Suppress("DEPRECATION") put(MediaStore.MediaColumns.DATA, target.absolutePath)
            put(MediaStore.MediaColumns.DISPLAY_NAME, target.name); put(MediaStore.MediaColumns.MIME_TYPE, mime); put(MediaStore.MediaColumns.SIZE, target.length())
        }
        val uri = context.contentResolver.insert(MediaStore.Files.getContentUri("external"), values) ?: throw TransferFailure("cannot_write")
        val result = SavedMedia(uri.toString(), target.name, mime)
        TaskStore.get(context).recordOutput(id, index, result)
        return result
    }
    fun update(context: Context): Boolean { init(context); return YoutubeDL.getInstance().updateYoutubeDL(context, YoutubeDL.UpdateChannel.STABLE) != null }
}
