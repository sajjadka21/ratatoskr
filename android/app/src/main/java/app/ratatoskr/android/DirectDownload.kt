package app.ratatoskr.android

import android.content.ContentValues
import android.content.Context
import org.json.JSONObject
import java.io.BufferedInputStream
import java.io.File
import java.io.FileOutputStream
import java.net.URI

/** Single-stream HTTP with identity-checked resume; never append an ignored Range. */
object DirectDownload {
    fun fetch(context: Context, task: MobileTask, url: String, control: TransferControl,
              progress: (Float) -> Unit, directory: File = Engine.work(context, task.id),
              openConnection: (String, Map<String, String>, () -> Unit) -> HttpConnection = SafeHttp::open): File {
        directory.mkdirs()
        val partial = File(directory, "transfer.part")
        val journal = File(directory, "transfer.json")
        var saved = runCatching { JSONObject(journal.readText()) }.getOrNull()
        if (saved != null && LinkUtils.sameResource(saved.optString("source"), url) && saved.optBoolean("completed")) {
            val name = LinkUtils.safeFileName(saved.optString("name"))
            val output = if (saved.optBoolean("output_subdirectory")) outputFile(directory, name)
                else File(directory, name.takeUnless { it in setOf("transfer.json", "transfer.part", "segments.json", "segments.part", "transfer.json.tmp") } ?: "output/$name")
            if (output.isFile && output.length() == saved.optLong("completed_bytes", -1)) return output
        }
        var offset = if (saved != null && LinkUtils.sameResource(saved.optString("source"), url)) partial.length() else 0L
        var validators = HttpValidators(saved?.optString("etag")?.takeIf { it.isNotEmpty() }, saved?.optString("modified")?.takeIf { it.isNotEmpty() })
        if (HttpResumePolicy.ifRange(validators) == null) offset = 0
        var connection = openConnection(url, if (offset > 0) mapOf("Range" to "bytes=$offset-", "If-Range" to HttpResumePolicy.ifRange(validators)!!) else emptyMap(), control::check)
        fun metadata() = HttpResponseMetadata(connection.responseCode, connection.getHeaderField("Content-Range"),
            connection.contentLengthLong.takeIf { it >= 0 }, HttpValidators(connection.getHeaderField("ETag"), connection.getHeaderField("Last-Modified")))
        try {
            var response = metadata()
            val request = ResumeRequest(offset, saved?.optLong("total", -1)?.takeIf { it >= 0 }, validators)
            var decision = HttpResumePolicy.evaluate(request, response)
            if (decision == ResumeDecision.RESTART && response.status != 200) {
                connection.disconnect(); connection = openConnection(url, emptyMap(), control::check); response = metadata()
                decision = HttpResumePolicy.evaluate(ResumeRequest(0), response)
            }
            if (decision == ResumeDecision.COMPLETE) {
                val name = saved?.optString("name")?.takeIf { it.isNotEmpty() } ?: "file"
                return complete(partial, outputFile(directory, LinkUtils.safeFileName(name)), journal, saved ?: JSONObject())
            }
            SafeHttp.requireSuccess(response.status)
            if (decision == ResumeDecision.REJECT || (decision == ResumeDecision.RESTART && response.status != 200)) throw TransferFailure("invalid_range")
            if (decision == ResumeDecision.RESTART) offset = 0
            val mime = connection.contentType.orEmpty().substringBefore(';').lowercase()
            val input = BufferedInputStream(connection.inputStream)
            input.mark(512)
            val prefix = ByteArray(512)
            val prefixLength = input.read(prefix).coerceAtLeast(0)
            input.reset()
            if (LinkPlan.isHtmlResponse(mime, prefix.copyOf(prefixLength))) throw TransferFailure("not_a_file")
            val rawName = Regex("filename\\*=UTF-8''([^;]+)", RegexOption.IGNORE_CASE).find(connection.getHeaderField("Content-Disposition").orEmpty())?.groupValues?.get(1)
                ?.let { runCatching { java.net.URLDecoder.decode(it.replace("+", "%2B"), "UTF-8") }.getOrNull() }
                ?: Regex("filename=\"?([^\";]+)", RegexOption.IGNORE_CASE).find(connection.getHeaderField("Content-Disposition").orEmpty())?.groupValues?.get(1)
                ?: URI(connection.url.toString()).path.substringAfterLast('/').ifEmpty { "file" }
            var name = LinkUtils.safeFileName(task.fileName.ifEmpty { rawName })
            if (directory != Engine.work(context, task.id) && mime.startsWith("image/")) {
                val extension = android.webkit.MimeTypeMap.getSingleton().getExtensionFromMimeType(mime) ?: "jpg"
                name = name.substringBeforeLast('.') + ".$extension"
            }
            val range = Regex("bytes (\\d+)-(\\d+)/(\\d+)").matchEntire(response.contentRange.orEmpty())
            val total = range?.groupValues?.get(3)?.toLongOrNull() ?: response.contentLength
            val expectedBody = response.contentLength ?: range?.let { it.groupValues[2].toLong() - it.groupValues[1].toLong() + 1 }
            Engine.ensureSpace(directory, ((total ?: (32L * 1024 * 1024)) - offset).coerceAtLeast(0))
            saved = JSONObject().put("source", url).put("name", name).put("etag", response.validators.etag.orEmpty())
                .put("modified", response.validators.lastModified.orEmpty()).put("total", total ?: -1)
            // A journal without a matching partial is harmless; a partial without a journal restarts.
            val temp = File(directory, "transfer.json.tmp"); temp.writeText(saved.toString())
            if (!temp.renameTo(journal)) { journal.writeText(saved.toString()); temp.delete() }
            val store = TaskStore.get(context)
            val start = android.os.SystemClock.elapsedRealtime()
            var received = 0L; var checkpoint = start
            val rate = MobilePreferences(context).speedLimit
            FileOutputStream(partial, offset > 0).use { output -> input.use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    control.check(); val count = input.read(buffer); if (count < 0) break
                    output.write(buffer, 0, count); received += count
                    if (expectedBody != null && received > expectedBody) throw TransferFailure("invalid_range")
                    if (rate > 0) {
                        val target = received * 1000 / rate
                        while (target > android.os.SystemClock.elapsedRealtime() - start) { control.check(); Thread.sleep(minOf(100, target - (android.os.SystemClock.elapsedRealtime() - start)).coerceAtLeast(1)) }
                    }
                    val now = android.os.SystemClock.elapsedRealtime()
                    if (now - checkpoint >= 500) {
                        output.fd.sync(); checkpoint = now
                        store.update(task.id, ContentValues().apply { put("bytes_done", offset + received); put("total_bytes", total ?: -1); put("validator", HttpResumePolicy.ifRange(response.validators).orEmpty()) })
                        progress(if (total != null && total > 0) ((offset + received).toDouble() / total * 98).toFloat() else 0f)
                        Engine.ensureSpace(directory, 1024 * 1024)
                    }
                }
                output.fd.sync()
            } }
            control.check()
            if (!HttpResumePolicy.transferFinished(expectedBody, received) || !HttpResumePolicy.transferFinished(total, partial.length())) throw TransferFailure("incomplete")
            store.update(task.id, ContentValues().apply { put("bytes_done", partial.length()); put("total_bytes", total ?: partial.length()); put("file_name", name) })
            return complete(partial, outputFile(directory, name), journal, saved)
        } finally { connection.disconnect() }
    }
    private fun complete(partial: File, output: File, journal: File, metadata: JSONObject): File {
        if (!partial.renameTo(output)) throw TransferFailure("cannot_write")
        metadata.put("completed", true).put("completed_bytes", output.length()).put("output_subdirectory", true)
        val temp = File(journal.parentFile, "transfer.json.tmp")
        temp.writeText(metadata.toString())
        if (!temp.renameTo(journal)) { journal.writeText(metadata.toString()); temp.delete() }
        return output
    }
    internal fun outputFile(directory: File, name: String): File =
        File(File(directory, "output").apply { if (!isDirectory && !mkdirs()) throw TransferFailure("cannot_write") }, name)
}
