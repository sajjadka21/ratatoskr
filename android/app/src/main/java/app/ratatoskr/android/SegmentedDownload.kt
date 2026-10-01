package app.ratatoskr.android

import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.io.RandomAccessFile
import java.net.URI
import java.util.concurrent.atomic.AtomicLong

/** One byte range of the file. [done] counts bytes that are on disk and synced. */
class Segment(val start: Long, val length: Long, @Volatile var done: Long = 0) {
    val end get() = start + length - 1
    val finished get() = done >= length
}

object SegmentPlanner {
    const val MIN_SEGMENT = 1L * 1024 * 1024
    /** Never more connections than the file can use: every segment gets at least [MIN_SEGMENT]. */
    fun plan(total: Long, connections: Int): List<Segment> {
        require(total > 0)
        val count = (total / MIN_SEGMENT).coerceIn(1, connections.coerceIn(1, 8).toLong()).toInt()
        val base = total / count
        return List(count) { i ->
            val start = base * i
            Segment(start, if (i == count - 1) total - start else base)
        }
    }
}

/** Splits one file over several connections (the "accelerator" of other download managers),
 * with a journal so every segment resumes where it stopped. Falls back to the single-stream
 * [DirectDownload] whenever the server cannot serve byte ranges safely. */
object SegmentedDownload {
    /** Files smaller than this are not worth extra connections. */
    const val MIN_TOTAL = 2L * 1024 * 1024
    private const val PART = "segments.part"
    private const val JOURNAL = "segments.json"

    fun fetch(
        directory: File, fileName: String, url: String, control: TransferControl,
        connections: Int, rate: Long,
        progress: (done: Long, total: Long) -> Unit,
        single: () -> File,
        openConnection: (String, Map<String, String>, () -> Unit) -> HttpConnection = SafeHttp::open,
    ): File {
        directory.mkdirs()
        val journal = File(directory, JOURNAL)
        // Work already started or finished in single-stream mode stays in that mode.
        if (File(directory, "transfer.json").exists() || connections <= 1) return single()
        val saved = readJournal(journal, url)
        val plan = saved ?: probe(url, connections, control, openConnection) ?: return single()
        val segments = plan.segments
        val part = File(directory, PART)
        if (saved == null || !part.isFile || part.length() != plan.total) {
            segments.forEach { it.done = 0 }
            if (directory.usableSpace < plan.total + 16L * 1024 * 1024) throw TransferFailure("no_space")
            RandomAccessFile(part, "rw").use { it.setLength(plan.total) }
        }
        writeJournal(journal, plan)
        val received = AtomicLong(segments.sumOf { it.done })
        val begin = System.nanoTime()
        val startBytes = received.get()
        val failure = java.util.concurrent.atomic.AtomicReference<Throwable?>(null)
        val checkpoint = Any()
        var lastReport = 0L

        fun report(force: Boolean = false) = synchronized(checkpoint) {
            val now = System.nanoTime() / 1_000_000
            if (!force && now - lastReport < 500) return@synchronized
            lastReport = now
            writeJournal(journal, plan)
            progress(segments.sumOf { it.done }, plan.total)
        }

        val workers = segments.filter { !it.finished }.map { segment ->
            Thread {
                try {
                    runSegment(plan, segment, part, url, control, openConnection, received, begin, startBytes, rate) { report() }
                } catch (error: Throwable) {
                    failure.compareAndSet(null, error)
                }
            }.apply { isDaemon = true; name = "segment-${segment.start}" }
        }
        workers.forEach { it.start() }
        try {
            while (workers.any { it.isAlive }) {
                workers.firstOrNull { it.isAlive }?.join(200)
                if (failure.get() != null) break
                control.check()
            }
        } catch (error: Throwable) {
            failure.compareAndSet(null, error)
        }
        if (failure.get() != null) workers.forEach { it.interrupt() }
        workers.forEach { it.join() }
        report(true)
        failure.get()?.let { throw if (it is Exception) it else IOException(it) }
        control.check()
        if (segments.any { !it.finished } || received.get() < plan.total) throw TransferFailure("incomplete")

        val output = File(directory, LinkUtils.safeFileName(fileName.ifEmpty { plan.name }))
        if (!part.renameTo(output)) throw TransferFailure("cannot_write")
        // Same shape as the single-stream journal, so a finished download is recognised either way.
        val finished = JSONObject().put("source", url).put("name", output.name).put("completed", true)
            .put("completed_bytes", output.length()).put("total", plan.total)
            .put("etag", plan.validators.etag.orEmpty()).put("modified", plan.validators.lastModified.orEmpty())
        File(directory, "transfer.json").writeText(finished.toString())
        journal.delete()
        return output
    }

    class Plan(val source: String, val total: Long, val name: String, val validators: HttpValidators, val segments: List<Segment>)

    /** Ask for the first byte: a 206 with a total proves ranges work and gives size, name and identity. */
    internal fun probe(url: String, connections: Int, control: TransferControl, open: (String, Map<String, String>, () -> Unit) -> HttpConnection): Plan? {
        val connection = open(url, mapOf("Range" to "bytes=0-0"), control::check)
        try {
            if (connection.responseCode != 206) {
                if (connection.responseCode == 200 || connection.responseCode == 416) return null
                SafeHttp.requireSuccess(connection.responseCode)
                return null
            }
            val range = Regex("bytes 0-0/(\\d+)").matchEntire(connection.getHeaderField("Content-Range").orEmpty()) ?: return null
            val total = range.groupValues[1].toLongOrNull() ?: return null
            if (total < MIN_TOTAL) return null
            val mime = connection.contentType.orEmpty().substringBefore(';').lowercase()
            if (mime in setOf("text/html", "application/xhtml+xml")) throw TransferFailure("not_a_file")
            val validators = HttpValidators(connection.getHeaderField("ETag"), connection.getHeaderField("Last-Modified"))
            // Without a validator a changed file could be stitched from two versions.
            if (HttpResumePolicy.ifRange(validators) == null) return null
            val name = LinkUtils.safeFileName(suggestedName(connection.getHeaderField("Content-Disposition"), connection.url.toString()))
            return Plan(url, total, name, validators, SegmentPlanner.plan(total, connections))
        } finally { connection.disconnect() }
    }

    fun suggestedName(disposition: String?, finalUrl: String): String {
        val header = disposition.orEmpty()
        return Regex("filename\\*=UTF-8''([^;]+)", RegexOption.IGNORE_CASE).find(header)?.groupValues?.get(1)
            ?.let { runCatching { java.net.URLDecoder.decode(it.replace("+", "%2B"), "UTF-8") }.getOrNull() }
            ?: Regex("filename=\"?([^\";]+)", RegexOption.IGNORE_CASE).find(header)?.groupValues?.get(1)
            ?: runCatching { java.net.URLDecoder.decode(URI(finalUrl).path.substringAfterLast('/'), "UTF-8") }.getOrNull()?.ifEmpty { null }
            ?: "file"
    }

    private fun runSegment(
        plan: Plan, segment: Segment, part: File, url: String, control: TransferControl,
        open: (String, Map<String, String>, () -> Unit) -> HttpConnection,
        received: AtomicLong, begin: Long, startBytes: Long, rate: Long, report: () -> Unit,
    ) {
        var attempts = 0
        while (!segment.finished) {
            control.check()
            try {
                val from = segment.start + segment.done
                val headers = mutableMapOf("Range" to "bytes=$from-${segment.end}")
                headers["If-Range"] = HttpResumePolicy.ifRange(plan.validators)!!
                val connection = open(url, headers, control::check)
                try {
                    if (connection.responseCode == 200) throw TransferFailure("invalid_range")
                    SafeHttp.requireSuccess(connection.responseCode)
                    val range = Regex("bytes (\\d+)-(\\d+)/(\\d+)").matchEntire(connection.getHeaderField("Content-Range").orEmpty())
                    if (connection.responseCode != 206 || range == null || range.groupValues[1].toLong() != from ||
                        range.groupValues[3].toLong() != plan.total || range.groupValues[2].toLong() < segment.end) throw TransferFailure("invalid_range")
                    RandomAccessFile(part, "rw").use { file ->
                        file.seek(from)
                        val buffer = ByteArray(64 * 1024)
                        var sinceSync = 0L
                        var lastSync = System.nanoTime()
                        var pending = 0L
                        connection.inputStream.use { input ->
                            while (segment.done + pending < segment.length) {
                                control.check()
                                val want = minOf(buffer.size.toLong(), segment.length - segment.done - pending).toInt()
                                val count = input.read(buffer, 0, want)
                                if (count < 0) throw IOException("short_read")
                                file.write(buffer, 0, count)
                                pending += count; sinceSync += count
                                val total = received.addAndGet(count.toLong())
                                if (rate > 0) throttle(total - startBytes, rate, begin, control)
                                if (System.nanoTime() - lastSync >= 500_000_000L) {
                                    file.fd.sync(); segment.done += pending; pending = 0; lastSync = System.nanoTime(); report()
                                }
                            }
                        }
                        file.fd.sync(); segment.done += pending
                    }
                    report()
                } finally { connection.disconnect() }
            } catch (error: TransferFailure) {
                throw error
            } catch (error: IOException) {
                // Bytes since the last sync are re-requested; counters follow what is really on disk.
                if (++attempts > 4) throw error
                Thread.sleep(500L * attempts)
            }
        }
    }

    /** One shared limit for every connection of the file. */
    private fun throttle(bytes: Long, rate: Long, begin: Long, control: TransferControl) {
        val target = bytes * 1000 / rate
        while (true) {
            val elapsed = (System.nanoTime() - begin) / 1_000_000
            if (target <= elapsed) return
            control.check(); Thread.sleep(minOf(100, target - elapsed).coerceAtLeast(1))
        }
    }

    private fun readJournal(file: File, url: String): Plan? {
        val json = runCatching { JSONObject(file.readText()) }.getOrNull() ?: return null
        if (json.optString("source") != url) return null
        val rows = json.optJSONArray("segments") ?: return null
        val segments = (0 until rows.length()).map {
            val row = rows.getJSONArray(it)
            Segment(row.getLong(0), row.getLong(1), row.getLong(2).coerceIn(0, row.getLong(1)))
        }
        val total = json.optLong("total", -1)
        if (total <= 0 || segments.isEmpty() || segments.sumOf { it.length } != total) return null
        return Plan(url, total, json.optString("name", "file"), HttpValidators(json.optString("etag").ifEmpty { null }, json.optString("modified").ifEmpty { null }), segments)
    }

    @Synchronized private fun writeJournal(file: File, plan: Plan) {
        val rows = JSONArray()
        plan.segments.forEach { rows.put(JSONArray().put(it.start).put(it.length).put(it.done)) }
        val json = JSONObject().put("source", plan.source).put("total", plan.total).put("name", plan.name)
            .put("etag", plan.validators.etag.orEmpty()).put("modified", plan.validators.lastModified.orEmpty()).put("segments", rows)
        val temp = File(file.parentFile, "$JOURNAL.tmp")
        temp.writeText(json.toString())
        if (!temp.renameTo(file)) { file.writeText(json.toString()); temp.delete() }
    }
}
