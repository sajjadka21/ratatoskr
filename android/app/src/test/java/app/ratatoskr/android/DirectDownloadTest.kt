package app.ratatoskr.android

import android.content.Context
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.SQLiteMode
import java.io.ByteArrayInputStream
import java.io.File
import java.io.InputStream
import java.net.URL
import java.util.UUID

/** Exercise real journals, files and SQLite, injecting only HTTP responses. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
@SQLiteMode(SQLiteMode.Mode.NATIVE)
class DirectDownloadTest {
    private val source = "https://downloads.example.org/archive.zip"
    private val tag = "\"original-file\""
    private lateinit var context: Context
    private lateinit var directory: File
    private lateinit var databaseName: String
    private lateinit var store: TaskStore
    private lateinit var task: MobileTask
    private var previousStore: Any? = null
    private val instanceField = TaskStore::class.java.getDeclaredField("instance").apply { isAccessible = true }

    @Before fun isolatedJournalAndDatabase() {
        context = RuntimeEnvironment.getApplication()
        databaseName = "direct-transfer-test-${UUID.randomUUID()}.db"
        store = TaskStore(context, databaseName)
        previousStore = instanceField.get(null)
        instanceField.set(null, store)
        task = store.enqueue(source, null, false, "Archive", "file")
        directory = File(context.filesDir, "direct-transfer-test-${UUID.randomUUID()}")
        assertTrue(directory.mkdirs())
        MobilePreferences(context).speedLimit = 0
    }

    @After fun removeOnlyOwnedFixtures() {
        instanceField.set(null, previousStore)
        store.close()
        context.deleteDatabase(databaseName)
        assertEquals(context.filesDir.canonicalFile, directory.canonicalFile.parentFile)
        directory.walkBottomUp().forEach { assertTrue(it.delete()) }
    }

    private class Response(
        override val responseCode: Int,
        bytes: ByteArray,
        private val headers: Map<String, String> = emptyMap(),
        override val contentLengthLong: Long = bytes.size.toLong(),
        override val contentType: String? = "application/octet-stream",
        override val url: URL = URL("https://downloads.example.org/archive.zip"),
    ) : HttpConnection {
        private val data = ByteArrayInputStream(bytes)
        var disconnected = false
        var bodyOpened = false
        override val inputStream: InputStream get() { bodyOpened = true; return data }
        override fun getHeaderField(name: String): String? = headers.entries
            .firstOrNull { it.key.equals(name, ignoreCase = true) }?.value
        override fun disconnect() { disconnected = true; data.close() }
    }

    private fun seedPartial(bytes: String, total: Long = 6) {
        File(directory, "transfer.part").writeText(bytes)
        File(directory, "transfer.json").writeText(JSONObject().put("source", source)
            .put("name", "archive.zip").put("etag", tag).put("modified", "").put("total", total).toString())
    }

    private fun journal() = JSONObject(File(directory, "transfer.json").readText())

    private fun fetch(open: (String, Map<String, String>, () -> Unit) -> HttpConnection): File =
        DirectDownload.fetch(context, task, source, TransferControl { true }, {}, directory, openConnection = open)

    @Test fun truncatedBodyPreservesItsPartialButNeverPublishesACompletedFile() {
        val response = Response(200, "short".toByteArray(), mapOf("ETag" to tag), contentLengthLong = 10)
        val error = assertThrows(TransferFailure::class.java) { fetch { _, _, check -> check(); response } }
        assertEquals("incomplete", error.code)
        assertFalse(File(directory, "archive.zip").exists())
        assertEquals("short", File(directory, "transfer.part").readText())
        assertFalse(journal().optBoolean("completed"))
        assertTrue(response.disconnected)
    }

    @Test fun bodyOverrunCannotBePromotedEvenIfTheConnectionEndsNormally() {
        val response = Response(200, "too-long".toByteArray(), mapOf("ETag" to tag), contentLengthLong = 3)
        val error = assertThrows(TransferFailure::class.java) { fetch { _, _, _ -> response } }
        assertEquals("invalid_range", error.code)
        assertFalse(File(directory, "archive.zip").exists())
        assertFalse(journal().optBoolean("completed"))
        assertTrue(response.disconnected)
    }

    @Test fun a206BodyMustRespectItsRangeEvenWhenContentLengthIsMissing() {
        seedPartial("abc")
        // The advertised range contains two bytes, but a three-byte body would
        // coincidentally make the total file size match and used to be accepted.
        val response = Response(206, "def".toByteArray(),
            mapOf("ETag" to tag, "Content-Range" to "bytes 3-4/6"), contentLengthLong = -1)
        val error = assertThrows(TransferFailure::class.java) { fetch { _, _, _ -> response } }
        assertEquals("invalid_range", error.code)
        assertFalse(File(directory, "archive.zip").exists())
        assertFalse(journal().optBoolean("completed"))
        assertTrue(response.disconnected)
    }

    @Test fun valid206AppendsOnlyTheMatchingRemainingBytesThenPromotesAtomically() {
        seedPartial("abc")
        var calls = 0
        val response = Response(206, "def".toByteArray(), mapOf("ETag" to tag, "Content-Range" to "bytes 3-5/6"))
        val output = fetch { address, headers, check ->
            calls++; check(); assertEquals(source, address)
            assertEquals("bytes=3-", headers["Range"]); assertEquals(tag, headers["If-Range"])
            response
        }
        assertEquals(1, calls)
        assertEquals("abcdef", output.readText())
        assertFalse(File(directory, "transfer.part").exists())
        assertTrue(journal().getBoolean("completed"))
        assertEquals(6, journal().getLong("completed_bytes"))
        assertEquals(6, store.get(task.id)!!.bytesDone)
        assertTrue(response.disconnected)
    }

    @Test fun ignoredRangeRestartsWithoutAppendingASecondWholeRepresentation() {
        seedPartial("abc")
        val response = Response(200, "UVWXYZ".toByteArray(), mapOf("ETag" to tag))
        val output = fetch { _, headers, _ -> assertEquals("bytes=3-", headers["Range"]); response }
        assertEquals("UVWXYZ", output.readText())
        assertEquals(6, output.length())
        assertTrue(journal().getBoolean("completed"))
    }

    @Test fun changedValidatorDiscardsTheOldPartialAndRepeatsAFullRequest() {
        seedPartial("abc")
        val newTag = "\"replacement-file\""
        val ranged = Response(206, "XYZ".toByteArray(), mapOf("ETag" to newTag, "Content-Range" to "bytes 3-5/6"))
        val full = Response(200, "UVWXYZ".toByteArray(), mapOf("ETag" to newTag))
        val requests = mutableListOf<Map<String, String>>()
        val output = fetch { _, headers, _ -> requests.add(headers); if (requests.size == 1) ranged else full }
        assertEquals(2, requests.size)
        assertEquals("bytes=3-", requests[0]["Range"])
        assertFalse(requests[1].containsKey("Range"))
        assertEquals("UVWXYZ", output.readText())
        assertEquals(newTag, journal().getString("etag"))
        assertTrue(ranged.disconnected && full.disconnected)
        assertFalse(ranged.bodyOpened)
    }

    @Test fun aCompletedJournalReusesTheExactOutputWithoutOpeningTheNetwork() {
        val bytes = "abcdef".toByteArray()
        val first = fetch { _, _, _ -> Response(200, bytes, mapOf("ETag" to tag)) }
        val recovered = fetch { _, _, _ -> throw AssertionError("Completed local output must not be downloaded again") }
        assertEquals(first, recovered)
        assertArrayEquals(bytes, recovered.readBytes())
    }

    @Test fun aServerFileNamedTransferJsonCannotBeOverwrittenByTheCompletionJournal() {
        val bytes = "The server's exact transfer.json content".toByteArray()
        val output = fetch { _, _, _ -> Response(200, bytes, mapOf(
            "ETag" to tag, "Content-Disposition" to "attachment; filename=\"transfer.json\"")) }

        assertEquals("transfer.json", output.name)
        assertArrayEquals(bytes, output.readBytes())
        assertTrue(journal().getBoolean("completed"))
        assertEquals(bytes.size.toLong(), journal().getLong("completed_bytes"))

        val recovered = fetch { _, _, _ -> throw AssertionError("The completed file must be reused without another request") }
        assertEquals(output, recovered)
        assertArrayEquals(bytes, recovered.readBytes())
    }

    @Test fun aMissingCompletedOutputRequiresAFreshTransfer() {
        val first = fetch { _, _, _ -> Response(200, "abcdef".toByteArray(), mapOf("ETag" to tag)) }
        assertTrue(first.delete())
        var opened = false
        val recovered = fetch { _, headers, _ ->
            opened = true; assertFalse(headers.containsKey("Range"))
            Response(200, "UVWXYZ".toByteArray(), mapOf("ETag" to tag))
        }
        assertTrue(opened)
        assertEquals("UVWXYZ", recovered.readText())
    }

    @Test fun validated416PromotesAnAlreadyCompletePartialWithoutReadingAnErrorBody() {
        seedPartial("abcdef")
        val response = Response(416, ByteArray(0), mapOf("ETag" to tag, "Content-Range" to "bytes */6"))
        val output = fetch { _, headers, _ -> assertEquals("bytes=6-", headers["Range"]); response }
        assertEquals("abcdef", output.readText())
        assertTrue(journal().getBoolean("completed"))
        assertFalse(response.bodyOpened)
        assertTrue(response.disconnected)
    }
}
