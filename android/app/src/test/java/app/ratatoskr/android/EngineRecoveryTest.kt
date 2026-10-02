package app.ratatoskr.android

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.content.pm.ProviderInfo
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.SQLiteMode
import org.robolectric.shadows.ShadowContentResolver
import java.io.File
import java.util.UUID

/** A real recorded output and readable ContentResolver URI survive task recovery. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
@SQLiteMode(SQLiteMode.Mode.NATIVE)
class EngineRecoveryTest {
    private lateinit var context: Context
    private lateinit var store: TaskStore
    private lateinit var databaseName: String
    private lateinit var fixture: File
    private lateinit var task: MobileTask
    private lateinit var provider: PublishedFileProvider
    private var previousStore: Any? = null
    private val instanceField = TaskStore::class.java.getDeclaredField("instance").apply { isAccessible = true }

    class PublishedFileProvider : ContentProvider() {
        lateinit var file: File
        var mutations = 0
        override fun onCreate() = true
        override fun getType(uri: Uri) = "application/octet-stream"
        override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
            check(mode == "r") { "Recovery must only read the committed output" }
            return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
        }
        override fun query(uri: Uri, projection: Array<out String>?, selection: String?,
            selectionArgs: Array<out String>?, sortOrder: String?): Cursor? = null
        override fun insert(uri: Uri, values: ContentValues?): Uri? {
            mutations++
            throw AssertionError("Recovery must not publish a second file")
        }
        override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int {
            mutations++
            return 0
        }
        override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int {
            mutations++
            return 0
        }
    }

    @Before fun createIsolatedCommittedOutput() {
        context = RuntimeEnvironment.getApplication()
        databaseName = "file-recovery-${UUID.randomUUID()}.db"
        store = TaskStore(context, databaseName)
        previousStore = instanceField.get(null)
        instanceField.set(null, store)
        task = store.enqueue("https://downloads.example.org/archive.bin", null, false, "Archive", "file")
        store.state(task.id, TaskState.PAUSED)
        fixture = File(context.filesDir, "published-recovery-${UUID.randomUUID()}.bin")
        fixture.writeText("The exact already committed file")
        val authority = "published.recovery.${UUID.randomUUID()}"
        provider = PublishedFileProvider().also {
            it.file = fixture
            it.attachInfo(context, ProviderInfo().apply { this.authority = authority })
            ShadowContentResolver.registerProviderInternal(authority, it)
        }
        store.recordOutput(task.id, 1, SavedMedia("content://$authority/output/1", "archive.bin", "application/octet-stream"))
    }

    @After fun removeOnlyOwnedFixtures() {
        instanceField.set(null, previousStore)
        store.close()
        context.deleteDatabase(databaseName)
        assertTrue(fixture.delete())
        val workspace = Engine.work(context, task.id)
        assertEquals(File(context.filesDir, "download-work").canonicalFile, workspace.canonicalFile.parentFile)
        if (workspace.exists()) {
            workspace.listFiles()?.forEach { assertTrue(it.delete()) }
            assertTrue(workspace.delete())
        }
    }

    @Test fun aPausedFileTaskReusesItsPublishedOutputWhenTheSourceIsNoLongerAccessible() {
        // Process death after recordOutput and publication-journal deletion can
        // leave a PAUSED task even though its file has already been committed.
        // An invalid source makes any attempted fresh transfer fail immediately,
        // without DNS or a network request.
        store.update(task.id, ContentValues().apply { put("url", "https://127.0.0.1/archive.bin") })
        val recovering = checkNotNull(store.get(task.id))
        val committed = checkNotNull(store.outputAt(task.id, 1))
        val states = mutableListOf<TaskState>()

        val result = Engine.download(context, recovering, TransferControl { true }, { states.add(it) }, {})

        assertEquals(listOf(committed), result)
        assertEquals(listOf(committed), store.outputs(task.id))
        assertEquals("The exact already committed file", context.contentResolver.openInputStream(Uri.parse(committed.uri))!!.bufferedReader().use { it.readText() })
        assertEquals(0, provider.mutations)
        assertTrue("Recovery must not start downloading or publishing again", states.none { it in setOf(TaskState.PROBING, TaskState.DOWNLOADING, TaskState.SAVING) })
    }
}
