package app.ratatoskr.android

import android.content.ContentProvider
import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.content.pm.ProviderInfo
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.provider.MediaStore
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowContentResolver
import java.io.File
import java.util.UUID

/** Real ContentResolver calls route through isolated provider rows. Native
 * extractors and device-wide media collections are never opened by the fixture. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
class EngineDiscardTest {
    private lateinit var context: Context
    private lateinit var taskId: String
    private lateinit var workspace: File
    private lateinit var media: RecordingProvider
    private lateinit var unrelated: RecordingProvider
    private lateinit var unrelatedFile: File

    class RecordingProvider : ContentProvider() {
        val rows = mutableMapOf<Uri, ContentValues>()
        val queried = mutableListOf<Uri>()
        val deleted = mutableListOf<Uri>()
        private var nextId = 1L
        override fun onCreate() = true
        override fun getType(uri: Uri): String? = rows[uri]?.getAsString(MediaStore.MediaColumns.MIME_TYPE)
        override fun insert(uri: Uri, values: ContentValues?): Uri {
            val target = ContentUris.withAppendedId(uri, nextId++)
            rows[target] = ContentValues(values ?: ContentValues())
            return target
        }
        override fun query(uri: Uri, projection: Array<out String>?, selection: String?,
            selectionArgs: Array<out String>?, sortOrder: String?): Cursor {
            queried.add(uri)
            val columns = projection ?: arrayOf(MediaStore.MediaColumns.IS_PENDING,
                MediaStore.MediaColumns.DISPLAY_NAME, MediaStore.MediaColumns.MIME_TYPE)
            return MatrixCursor(columns).apply {
                rows[uri]?.let { values -> addRow(columns.map { values.get(it) }) }
            }
        }
        override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int {
            val row = rows[uri] ?: return 0
            if (values != null) row.putAll(values)
            return 1
        }
        override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int {
            deleted.add(uri)
            return if (rows.remove(uri) != null) 1 else 0
        }
    }

    @Before fun createIsolatedMediaAndWorkspace() {
        context = RuntimeEnvironment.getApplication()
        taskId = UUID.randomUUID().toString()
        workspace = Engine.work(context, taskId)
        assertTrue(workspace.mkdirs())
        media = provider("media")
        unrelated = provider("unrelated.fixture")
        unrelatedFile = File(context.filesDir, "unrelated-discard-test-${UUID.randomUUID()}.txt")
        unrelatedFile.writeText("Other data must survive cancellation")
    }

    private fun provider(authority: String): RecordingProvider = RecordingProvider().also {
        it.attachInfo(context, ProviderInfo().apply { this.authority = authority })
        ShadowContentResolver.registerProviderInternal(authority, it)
    }

    @After fun removeOnlyOwnedFixtures() {
        assertEquals(File(context.filesDir, "download-work").canonicalFile, workspace.canonicalFile.parentFile)
        if (workspace.exists()) {
            workspace.listFiles()?.forEach { assertTrue(it.delete()) }
            assertTrue(workspace.delete())
        }
        if (unrelatedFile.exists()) assertTrue(unrelatedFile.delete())
        media.rows.clear()
        unrelated.rows.clear()
    }

    private fun row(collection: Uri, pending: Int): Uri {
        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.IS_PENDING, pending)
            put(MediaStore.MediaColumns.DISPLAY_NAME, "owned-fixture.bin")
            put(MediaStore.MediaColumns.MIME_TYPE, "application/octet-stream")
            put(MediaStore.MediaColumns.OWNER_PACKAGE_NAME, context.packageName)
        }
        return checkNotNull(context.contentResolver.insert(collection, values))
    }

    private fun journal(index: Int, target: Uri) {
        File(workspace, "pending-$index.uri").writeText(target.toString())
    }

    @Test fun cancellingAfterProcessDeathDeletesPendingDownloadsAndPhotos() {
        val download = row(MediaStore.Downloads.EXTERNAL_CONTENT_URI, 1)
        val image = row(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, 1)
        journal(1, download)
        journal(2, image)
        File(workspace, "transfer.part").writeText("private partial")

        Engine.discard(context, taskId)

        assertFalse("Pending download row must not remain orphaned", media.rows.containsKey(download))
        assertFalse("Pending photo row must not remain orphaned", media.rows.containsKey(image))
        assertEquals(setOf(download, image), media.deleted.toSet())
        assertFalse("Private journal and partial are discarded", workspace.exists())
    }

    @Test fun aStaleJournalNeverDeletesAlreadyPublishedMedia() {
        val pending = row(MediaStore.Downloads.EXTERNAL_CONTENT_URI, 1)
        val published = row(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, 0)
        journal(1, pending)
        // Simulate process death after clearing IS_PENDING but before removing
        // the publication journal. The committed photo belongs to the user.
        journal(2, published)

        Engine.discard(context, taskId)

        assertFalse(media.rows.containsKey(pending))
        assertTrue("Committed media must survive cancellation", media.rows.containsKey(published))
        assertEquals(0, media.rows[published]!!.getAsInteger(MediaStore.MediaColumns.IS_PENDING).toInt())
        assertFalse(media.deleted.contains(published))
        assertFalse(workspace.exists())
    }

    @Test fun aJournalCannotMakeCancellationAccessAnUnrelatedProviderOrFile() {
        val foreign = row(Uri.parse("content://unrelated.fixture/documents"), 1)
        journal(1, foreign)
        journal(2, Uri.fromFile(unrelatedFile))

        Engine.discard(context, taskId)

        assertTrue(unrelated.rows.containsKey(foreign))
        assertTrue("No reads through an untrusted journal authority", unrelated.queried.isEmpty())
        assertTrue("No deletes through an untrusted journal authority", unrelated.deleted.isEmpty())
        assertTrue(unrelatedFile.exists())
        assertEquals("Other data must survive cancellation", unrelatedFile.readText())
        assertFalse(workspace.exists())
    }
}
