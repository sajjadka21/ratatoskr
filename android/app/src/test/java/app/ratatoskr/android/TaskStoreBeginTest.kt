package app.ratatoskr.android

import android.content.ContentValues
import android.content.Context
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
import org.robolectric.annotation.SQLiteMode
import java.util.UUID

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
@SQLiteMode(SQLiteMode.Mode.NATIVE)
class TaskStoreBeginTest {
    private lateinit var context: Context
    private lateinit var store: TaskStore
    private lateinit var databaseName: String

    @Before fun createIsolatedJournal() {
        context = RuntimeEnvironment.getApplication()
        databaseName = "task-begin-${UUID.randomUUID()}.db"
        store = TaskStore(context, databaseName)
    }

    @After fun deleteOnlyThisJournal() {
        store.close()
        context.deleteDatabase(databaseName)
    }

    private fun task(state: TaskState): MobileTask {
        val task = store.enqueue("https://example.com/${UUID.randomUUID()}.zip", null, false, "Checkpoint", "file")
        store.update(task.id, ContentValues().apply {
            put("state", state.name)
            put("progress", 37)
            put("bytes_done", (1L shl 33) + 1024L)
            put("total_bytes", 1L shl 34)
            put("validator", "existing-representation")
            put("error", "old-error")
        })
        return store.get(task.id)!!
    }

    @Test fun queuedAndNetworkWaitingJobsBeginWithoutDiscardingTheirCheckpoint() {
        listOf(TaskState.QUEUED, TaskState.WAITING_NETWORK).forEach { previousState ->
            val previous = task(previousState)
            assertTrue(store.begin(previous.id))
            val actual = store.get(previous.id)!!
            assertEquals(TaskState.PROBING, actual.state)
            assertEquals("", actual.error)
            assertEquals(previous.progress, actual.progress)
            assertEquals(previous.bytesDone, actual.bytesDone)
            assertEquals(previous.totalBytes, actual.totalBytes)
            assertEquals(previous.validator, actual.validator)
            assertEquals(previous.notificationId, actual.notificationId)
        }
    }

    @Test fun beginCannotUndoUserPauseCancellationOrCompletedHistory() {
        listOf(TaskState.PAUSED, TaskState.CANCELLED, TaskState.COMPLETED, TaskState.FAILED, TaskState.NEEDS_SELECTION).forEach { state ->
            val previous = task(state)
            assertFalse(store.begin(previous.id))
            assertEquals(previous, store.get(previous.id))
        }
    }

    @Test fun beginCannotAcquireAJobWhoseExistingWriterIsAlreadyInFlight() {
        listOf(TaskState.PROBING, TaskState.DOWNLOADING, TaskState.MERGING, TaskState.SAVING).forEach { state ->
            val previous = task(state)
            assertFalse(store.begin(previous.id))
            assertEquals(previous, store.get(previous.id))
        }
    }

    @Test fun staleQueueSnapshotCannotOverwriteAMoreRecentUserPause() {
        val captured = task(TaskState.QUEUED)
        store.state(captured.id, TaskState.PAUSED)
        val paused = store.get(captured.id)!!
        assertFalse(store.begin(captured.id))
        assertEquals(paused, store.get(captured.id))
        assertFalse(store.begin(UUID.randomUUID().toString()))
    }
}
