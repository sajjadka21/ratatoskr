package app.ratatoskr.android

import android.content.ContentValues
import android.content.Context
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteOpenHelper
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.SQLiteMode
import java.io.File
import java.util.UUID

/** Reopen the real SQLite database between operations rather than using a
 * fake journal, exercising the same persistence boundary as process death. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
@SQLiteMode(SQLiteMode.Mode.NATIVE)
class TaskStoreTest {
    private lateinit var context: Context
    private lateinit var store: TaskStore
    private lateinit var databaseName: String
    private lateinit var partialDirectory: File

    @Before fun openIsolatedDatabase() {
        context = RuntimeEnvironment.getApplication()
        databaseName = "task-store-test-${UUID.randomUUID()}.db"
        partialDirectory = File(context.filesDir, "task-store-test-${UUID.randomUUID()}")
        assertTrue(partialDirectory.mkdirs())
        store = TaskStore(context, databaseName)
    }

    @After fun removeOnlyThisTestDatabaseAndFixture() {
        store.close()
        context.deleteDatabase(databaseName)
        partialDirectory.listFiles()?.forEach { assertTrue(it.delete()) }
        assertTrue(partialDirectory.delete())
    }

    private fun reopen() {
        store.close()
        store = TaskStore(context, databaseName)
    }

    @Test fun queuedJobAndItsOutputOptionsSurviveReopening() {
        val task = store.enqueue(
            "https://www.instagram.com/reel/Stable123/?igsh=share-source",
            540, false, "Video to continue", "media", "1,3",
        )
        assertEquals(TaskState.QUEUED, task.state)
        assertEquals(task.id, UUID.fromString(task.id).toString())
        assertTrue(task.notificationId > 0)
        reopen()
        assertEquals(task, store.get(task.id))
        assertEquals(listOf(task), store.list())
        assertEquals("https://www.instagram.com/reel/Stable123/", store.get(task.id)!!.url)
    }

    @Test fun persistedJobIdentityKeepsPartialFileAssociationAcrossRecovery() {
        val task = store.enqueue("https://example.com/archive.zip", null, false, "Archive", "file")
        val partial = File(partialDirectory, "${task.id}.part")
        val bytes = ByteArray(257) { (it % 251).toByte() }
        partial.writeBytes(bytes)
        store.update(task.id, ContentValues().apply {
            put("state", TaskState.DOWNLOADING.name)
            put("bytes_done", bytes.size.toLong())
            put("total_bytes", 4096L)
            put("validator", "fixture-version-1")
        })
        reopen()
        store.recover()
        val recovered = store.get(task.id)!!
        assertEquals(task.id, recovered.id)
        assertEquals(task.notificationId, recovered.notificationId)
        assertEquals(TaskState.PAUSED, recovered.state)
        assertEquals(bytes.size.toLong(), recovered.bytesDone)
        assertEquals(4096L, recovered.totalBytes)
        assertEquals("fixture-version-1", recovered.validator)
        assertTrue(File(partialDirectory, "${recovered.id}.part").readBytes().contentEquals(bytes))
    }

    @Test fun canonicalInstagramSharesDoNotCreateDuplicateActiveJobs() {
        val first = store.enqueue("https://instagram.com/reels/Repeated123/?igsh=first", 720, false, "First title")
        store.state(first.id, TaskState.WAITING_NETWORK)
        reopen()
        val second = store.enqueue("https://m.instagram.com/reel/Repeated123/?igsh=second#tracking", 720, false, "Shared again")
        assertEquals(first.id, second.id)
        assertEquals(first.notificationId, second.notificationId)
        assertEquals(TaskState.WAITING_NETWORK, second.state)
        assertEquals(1, store.list().size)
    }

    @Test fun qualityAudioAndAlbumSelectionsRemainDistinctUserRequests() {
        val url = "https://www.instagram.com/p/Album123/"
        val first = store.enqueue(url, 720, false, "First", "media", "1,3")
        val same = store.enqueue("$url?igsh=other", 720, false, "Again", "media", "1,3")
        val smaller = store.enqueue(url, 360, false, "Smaller", "media", "1,3")
        val audio = store.enqueue(url, 720, true, "Audio", "media", "1,3")
        val otherItems = store.enqueue(url, 720, false, "Other items", "media", "2")
        val photo = store.enqueue(url, 720, false, "Photo", "photo", "1,3")
        assertEquals(first.id, same.id)
        assertEquals(5, setOf(first.id, smaller.id, audio.id, otherItems.id, photo.id).size)
        assertEquals(5, store.list().size)
        reopen()
        assertEquals("1,3", store.get(first.id)!!.selectedItems)
        assertEquals("2", store.get(otherItems.id)!!.selectedItems)
        assertTrue(store.get(audio.id)!!.audioOnly)
        assertEquals("photo", store.get(photo.id)!!.kind)
    }

    @Test fun recoveryPausesOnlyInterruptedJobsAndIsIdempotent() {
        val original = TaskState.values().associateWith { state ->
            val task = store.enqueue("https://example.com/${state.name}.mp4", null, false, state.name)
            store.state(task.id, state, if (state == TaskState.FAILED) "network_error" else "")
            store.get(task.id)!!
        }
        reopen()
        store.recover()
        original.forEach { (previousState, task) ->
            val actual = store.get(task.id)!!
            val interrupted = previousState in setOf(
                TaskState.PROBING, TaskState.DOWNLOADING, TaskState.MERGING, TaskState.SAVING,
            )
            assertEquals(if (interrupted) TaskState.PAUSED else previousState, actual.state)
            assertEquals(if (interrupted) "interrupted" else task.error, actual.error)
            assertEquals(task.notificationId, actual.notificationId)
            assertEquals(task.createdAt, actual.createdAt)
        }
        val recovered = store.list().associateBy { it.id }
        store.recover()
        assertEquals(recovered, store.list().associateBy { it.id })
        reopen()
        assertEquals(recovered, store.list().associateBy { it.id })
    }

    @Test fun terminalHistoryIsNeverAutomaticallyRequeuedButAnExplicitNewRequestIsAllowed() {
        listOf(TaskState.FAILED, TaskState.CANCELLED, TaskState.COMPLETED).forEach { terminal ->
            val url = "https://example.com/${terminal.name}.mp4"
            val old = store.enqueue(url, null, false, "Old request")
            store.state(old.id, terminal, if (terminal == TaskState.FAILED) "network_error" else "")
            reopen()
            store.recover()
            assertEquals(terminal, store.get(old.id)!!.state)
            // Re-sharing is explicit; it must not rewrite previous history.
            val requested = store.enqueue(url, null, false, "New request")
            assertNotEquals(old.id, requested.id)
            assertEquals(TaskState.QUEUED, requested.state)
            assertEquals(terminal, store.get(old.id)!!.state)
        }
        assertEquals(6, store.list().size)
    }

    @Test fun completedFileUriNameAndMimePersistForOpenAndShareActions() {
        val task = store.enqueue("https://example.com/music.m4a", null, true, "Music")
        store.update(task.id, ContentValues().apply {
            put("uri", "content://media/external/downloads/123")
            put("file_name", "music.m4a")
            put("mime", "audio/mp4")
            put("progress", 100)
            put("state", TaskState.COMPLETED.name)
        })
        reopen()
        store.recover()
        val saved = store.get(task.id)!!
        assertEquals(TaskState.COMPLETED, saved.state)
        assertEquals("content://media/external/downloads/123", saved.uri)
        assertEquals("music.m4a", saved.fileName)
        assertEquals("audio/mp4", saved.mime)
        assertEquals(100, saved.progress)
        assertEquals(null, saved.height)
        assertTrue(saved.audioOnly)
    }

    @Test fun explicitRetryClearsStaleErrorWithoutChangingTaskIdentity() {
        val task = store.enqueue("https://example.com/retry.zip", null, false, "Retry", "file")
        store.state(task.id, TaskState.FAILED, "storage_full")
        reopen()
        assertEquals("storage_full", store.get(task.id)!!.error)
        store.state(task.id, TaskState.QUEUED)
        reopen()
        val retried = store.get(task.id)!!
        assertEquals(TaskState.QUEUED, retried.state)
        assertTrue(retried.error.isEmpty())
        assertEquals(task.notificationId, retried.notificationId)
        assertFalse(retried.audioOnly)
        assertEquals(1, store.list().size)
    }

    @Test fun largeFileCheckpointsRemain64BitAcrossReopening() {
        val task = store.enqueue("https://example.com/large.iso", null, false, "Large file", "file")
        val checkpoint = (1L shl 33) + 1024L
        val total = 1L shl 34
        store.update(task.id, ContentValues().apply {
            put("bytes_done", checkpoint)
            put("total_bytes", total)
            put("state", TaskState.PAUSED.name)
        })
        reopen()
        assertEquals(checkpoint, store.get(task.id)!!.bytesDone)
        assertEquals(total, store.get(task.id)!!.totalBytes)
    }

    @Test fun albumOutputsRetainSourcePositionOrderAcrossReopening() {
        val task = store.enqueue("https://www.instagram.com/p/Outputs123/", 720, false, "Album")
        val first = SavedMedia("content://media/external/downloads/101", "first.jpg", "image/jpeg")
        val second = SavedMedia("content://media/external/downloads/102", "second.mp4", "video/mp4")
        val third = SavedMedia("content://media/external/downloads/103", "third.jpg", "image/jpeg")
        store.recordOutput(task.id, 3, third)
        store.recordOutput(task.id, 1, first)
        store.recordOutput(task.id, 2, second)
        assertEquals(listOf(first, second, third), store.outputs(task.id))

        reopen()

        assertEquals(listOf(first, second, third), store.outputs(task.id))
        assertEquals(task.id, store.get(task.id)!!.id)
    }

    @Test fun recordingAnOutputTwiceIsIdempotentAndReplacesTheSamePosition() {
        val task = store.enqueue("https://www.instagram.com/p/Upsert123/", null, false, "Album")
        val original = SavedMedia("content://media/external/downloads/201", "photo.jpg", "image/jpeg")
        val replacement = SavedMedia("content://media/external/downloads/202", "photo-new.jpg", "image/jpeg")
        store.recordOutput(task.id, 1, original)
        store.recordOutput(task.id, 1, original)
        assertEquals(listOf(original), store.outputs(task.id))
        reopen()

        store.recordOutput(task.id, 1, replacement)
        reopen()

        assertEquals(listOf(replacement), store.outputs(task.id))
    }

    @Test fun outputsForDifferentTasksCannotOverwriteEachOther() {
        val firstTask = store.enqueue("https://www.instagram.com/p/Isolated123/", null, false, "First")
        val secondTask = store.enqueue("https://www.instagram.com/p/Isolated456/", null, false, "Second")
        val first = SavedMedia("content://media/external/downloads/301", "first.jpg", "image/jpeg")
        val second = SavedMedia("content://media/external/downloads/302", "second.jpg", "image/jpeg")
        store.recordOutput(firstTask.id, 1, first)
        store.recordOutput(secondTask.id, 1, second)
        reopen()
        assertEquals(listOf(first), store.outputs(firstTask.id))
        assertEquals(listOf(second), store.outputs(secondTask.id))
    }

    @Test fun interruptedPublicationRecoveryPreservesAlreadyPublishedAlbumOutputs() {
        val task = store.enqueue("https://www.instagram.com/p/Publishing123/", 540, false, "Album", items = "1,3")
        val saved = SavedMedia("content://media/external/downloads/401", "first.jpg", "image/jpeg")
        store.state(task.id, TaskState.SAVING)
        store.recordOutput(task.id, 1, saved)
        reopen()

        store.recover()

        assertEquals(TaskState.PAUSED, store.get(task.id)!!.state)
        assertEquals("1,3", store.get(task.id)!!.selectedItems)
        assertEquals(listOf(saved), store.outputs(task.id))
        store.recover()
        reopen()
        assertEquals(listOf(saved), store.outputs(task.id))
    }

    @Test fun oldVersionOneDatabaseMigratesWithoutDroppingItsDownloadCheckpoint() {
        store.close()
        // A shipped v1 fixture, deliberately independent of TaskStore.onCreate:
        // using the new helper to seed it would not exercise an actual upgrade.
        val legacy = object : SQLiteOpenHelper(context, databaseName, null, 1) {
            override fun onCreate(db: SQLiteDatabase) {
                db.execSQL("""CREATE TABLE tasks (
                    notification_id INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE,
                    url TEXT NOT NULL, title TEXT NOT NULL, state TEXT NOT NULL, height INTEGER,
                    audio INTEGER NOT NULL, kind TEXT NOT NULL, progress INTEGER NOT NULL DEFAULT 0,
                    error TEXT NOT NULL DEFAULT '', uri TEXT NOT NULL DEFAULT '', file_name TEXT NOT NULL DEFAULT '',
                    mime TEXT NOT NULL DEFAULT '', selected_items TEXT NOT NULL DEFAULT '',
                    bytes_done INTEGER NOT NULL DEFAULT 0, total_bytes INTEGER NOT NULL DEFAULT -1,
                    validator TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
                )""")
                db.execSQL("CREATE INDEX task_state ON tasks(state, created_at)")
            }
            override fun onUpgrade(db: SQLiteDatabase, oldVersion: Int, newVersion: Int) = Unit
        }
        val oldId = UUID.randomUUID().toString()
        legacy.use {
            it.writableDatabase.insertOrThrow("tasks", null, ContentValues().apply {
                put("notification_id", 73)
                put("id", oldId)
                put("url", "https://example.com/migration.zip")
                put("title", "Existing download")
                put("state", TaskState.PAUSED.name)
                put("audio", 0)
                put("kind", "file")
                put("bytes_done", 4096L)
                put("total_bytes", 8192L)
                put("validator", "original-version")
                put("created_at", 123456789L)
                put("updated_at", 123456790L)
            })
        }

        store = TaskStore(context, databaseName)
        val migrated = store.get(oldId)!!
        assertEquals("Existing download", migrated.title)
        assertEquals(TaskState.PAUSED, migrated.state)
        assertEquals(73, migrated.notificationId)
        assertEquals(4096L, migrated.bytesDone)
        assertEquals(8192L, migrated.totalBytes)
        assertEquals("original-version", migrated.validator)
        assertEquals(123456789L, migrated.createdAt)
        assertTrue(store.outputs(oldId).isEmpty())
        val output = SavedMedia("content://media/external/downloads/501", "migration.zip", "application/zip")
        store.recordOutput(oldId, 1, output)
        reopen()
        assertEquals(listOf(output), store.outputs(oldId))
        assertEquals(1, store.list().size)
    }

    @Test fun postAndReelAliasesDeduplicateByContentWithoutLosingQualityChoice() {
        val first = store.enqueue("https://www.instagram.com/p/SameContent123/?igsh=first", 720, false, "Post")
        store.state(first.id, TaskState.WAITING_NETWORK)
        reopen()

        val reel = store.enqueue("https://www.instagram.com/reel/SameContent123/?igsh=second", 720, false, "Reel alias")
        val smaller = store.enqueue("https://www.instagram.com/reel/SameContent123/", 360, false, "Different quality")

        assertEquals(first.id, reel.id)
        assertEquals(TaskState.WAITING_NETWORK, reel.state)
        assertNotEquals(first.id, smaller.id)
        assertEquals(2, store.list().size)
    }
}
