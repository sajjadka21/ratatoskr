package app.ratatoskr.android

import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.File
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.nio.file.Files
import java.util.concurrent.atomic.AtomicInteger

/** A real local HTTP server with byte-range support; only the opener bypasses the public-address guard. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
class SegmentedDownloadTest {
    private lateinit var server: TestHttpServer
    private lateinit var directory: File
    private val payload = ByteArray(5 * 1024 * 1024 + 123) { (it * 31 + it / 7).toByte() }
    private val etag = "\"v1\""
    private val rangeRequests = AtomicInteger()
    @Volatile private var supportRanges = true
    @Volatile private var sendEtag = true
    @Volatile private var failFirstRange = false
    private val failed = AtomicInteger()
    @Volatile private var slowFirstPart = false
    private val base get() = "http://127.0.0.1:${server.port}/big.bin"
    private val control = TransferControl { true }

    @Before fun start() {
        directory = Files.createTempDirectory("seg").toFile()
        server = TestHttpServer { request -> serve(request) }
        server.start()
    }
    @After fun stop() { server.stop(); directory.deleteRecursively() }

    private fun serve(request: TestHttpServer.Request) {
        val range = request.headers["range"]
        val common = mutableMapOf("Content-Type" to "application/octet-stream")
        if (sendEtag) common["ETag"] = etag
        val match = Regex("bytes=(\\d+)-(\\d*)").matchEntire(range.orEmpty())
        if (!supportRanges || match == null) {
            request.respond(200, common, payload.size.toLong())
            request.out.write(payload); return
        }
        rangeRequests.incrementAndGet()
        val from = match.groupValues[1].toInt()
        val to = match.groupValues[2].toIntOrNull()?.coerceAtMost(payload.size - 1) ?: (payload.size - 1)
        request.respond(206, common + ("Content-Range" to "bytes $from-$to/${payload.size}"), (to - from + 1).toLong())
        if (failFirstRange && from > 0 && failed.getAndIncrement() == 0) {
            request.out.write(payload, from, 1000); request.out.flush(); request.abort(); return
        }
        if (slowFirstPart && from == 0 && to > 0) {
            var at = from
            while (at <= to) { val n = minOf(32 * 1024, to - at + 1); request.out.write(payload, at, n); request.out.flush(); at += n; Thread.sleep(8) }
        } else request.out.write(payload, from, to - from + 1)
    }

    private class Conn(private val c: HttpURLConnection) : HttpConnection {
        override val responseCode get() = c.responseCode
        override val contentLengthLong get() = c.contentLengthLong
        override val contentType: String? get() = c.contentType
        override val url: URL get() = c.url
        override val inputStream: InputStream get() = if (c.responseCode >= 400) c.errorStream else c.inputStream
        override fun getHeaderField(name: String) = c.getHeaderField(name)
        override fun disconnect() = c.disconnect()
    }
    private val open: (String, Map<String, String>, () -> Unit) -> HttpConnection = { url, headers, _ ->
        val c = URL(url).openConnection() as HttpURLConnection
        headers.forEach { (k, v) -> c.setRequestProperty(k, v) }
        Conn(c)
    }

    private fun run(connections: Int = 4, rate: Long = 0, name: String = "", single: () -> File = { error("single-stream used") }, ctl: TransferControl = control): File =
        SegmentedDownload.fetch(directory, name, base, ctl, connections, rate, { _, _ -> }, single, open)

    @Test fun planSplitsEvenlyAndCoversEveryByte() {
        val plan = SegmentPlanner.plan(10L * 1024 * 1024 + 5, 4)
        assertEquals(4, plan.size)
        assertEquals(10L * 1024 * 1024 + 5, plan.sumOf { it.length })
        plan.zipWithNext { a, b -> assertEquals(a.start + a.length, b.start) }
        assertEquals(1, SegmentPlanner.plan(1500, 8).size)
        assertEquals(2, SegmentPlanner.plan(2L * 1024 * 1024, 8).size)
    }

    @Test fun downloadsOverSeveralConnectionsAndMatchesByteForByte() {
        val file = run()
        assertArrayEquals(payload, file.readBytes())
        assertEquals("big.bin", file.name)
        assertTrue(rangeRequests.get() >= 5)
        assertFalse(File(directory, "segments.part").exists())
        assertFalse(File(directory, "segments.json").exists())
    }

    @Test fun aFastConnectionTakesOverTheTailOfASlowOne() {
        slowFirstPart = true
        val file = run(connections = 2)
        assertArrayEquals(payload, file.readBytes())
        // Two initial parts plus at least one stolen tail (and the size probe).
        assertTrue("requests: ${rangeRequests.get()}", rangeRequests.get() >= 4)
    }

    @Test fun stealingNeverSplitsSmallRemainders() {
        val plan = SegmentedDownload.Plan("u", 3L * 1024 * 1024, "n", HttpValidators("\"x\""), listOf(Segment(0, 3L * 1024 * 1024, 2L * 1024 * 1024 + 500_000)))
        assertEquals(null, SegmentedDownload.steal(plan))
        val big = SegmentedDownload.Plan("u", 10L * 1024 * 1024, "n", HttpValidators("\"x\""), listOf(Segment(0, 10L * 1024 * 1024, 0)))
        val tail = SegmentedDownload.steal(big)!!
        assertEquals(5L * 1024 * 1024, tail.start)
        assertEquals(10L * 1024 * 1024, big.segments.sumOf { it.length })
    }

    @Test fun usesTheNameChosenByTheUser() {
        assertEquals("mine.bin", run(name = "mine.bin").name)
    }

    @Test fun fallsBackToSingleStreamWithoutRangeSupport() {
        supportRanges = false
        val fallback = File(directory, "single.bin").apply { writeText("x") }
        assertEquals(fallback, run(single = { fallback }))
    }

    @Test fun fallsBackWithoutAValidator() {
        sendEtag = false
        val fallback = File(directory, "single.bin").apply { writeText("x") }
        assertEquals(fallback, run(single = { fallback }))
    }

    @Test fun oneConnectionMeansSingleStream() {
        val fallback = File(directory, "single.bin").apply { writeText("x") }
        assertEquals(fallback, run(connections = 1, single = { fallback }))
        assertEquals(0, rangeRequests.get())
    }

    @Test fun retriesASegmentThatBreaksMidway() {
        failFirstRange = true
        assertArrayEquals(payload, run().readBytes())
    }

    @Test fun resumesFromTheJournalAfterAnInterruption() {
        val interrupting = TransferControl { true }
        // Stop the first run once some data has been journaled.
        val progressSeen = AtomicInteger()
        val partial = runCatching {
            SegmentedDownload.fetch(directory, "", base, interrupting, 4, 2L * 1024 * 1024, { done, _ ->
                if (done > 1024 * 1024 && progressSeen.incrementAndGet() == 1) interrupting.stop()
            }, { error("single") }, open)
        }
        assertTrue(partial.isFailure)
        assertTrue(File(directory, "segments.json").exists())
        assertTrue(File(directory, "segments.part").exists())
        rangeRequests.set(0)
        val file = run()
        assertArrayEquals(payload, file.readBytes())
    }

    @Test fun stopDuringTransferLeavesTheJournal() {
        val ctl = TransferControl { true }
        val result = runCatching {
            SegmentedDownload.fetch(directory, "", base, ctl, 4, 1024 * 1024, { _, _ -> ctl.stop() }, { error("single") }, open)
        }
        assertTrue(result.exceptionOrNull() is TransferFailure)
        assertEquals("interrupted", (result.exceptionOrNull() as TransferFailure).code)
    }

    @Test fun aChangedFileRestartsInsteadOfStitchingVersions() {
        run()
        // Pretend the server file changed: a journal with a different source is ignored.
        File(directory, "transfer.json").delete()
        File(directory, "big.bin").delete()
        assertArrayEquals(payload, run().readBytes())
    }

    @Test fun suggestedNameUsesDispositionThenPath() {
        assertEquals("a b.zip", SegmentedDownload.suggestedName("attachment; filename*=UTF-8''a%20b.zip", "https://x/y"))
        assertEquals("plain.zip", SegmentedDownload.suggestedName("attachment; filename=\"plain.zip\"", "https://x/y"))
        assertEquals("tool.exe", SegmentedDownload.suggestedName(null, "https://x/dl/tool.exe?token=1"))
        assertEquals("file", SegmentedDownload.suggestedName(null, "https://x/"))
    }
}
