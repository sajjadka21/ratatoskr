package app.ratatoskr.android

import android.content.Context
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
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
class MobileRuntimeTest {
    private lateinit var context: Context
    private lateinit var store: TaskStore
    private lateinit var databaseName: String
    private val ownedIds = mutableListOf<String>()

    @Before fun createIsolatedJournal() {
        context = RuntimeEnvironment.getApplication()
        databaseName = "runtime-test-${UUID.randomUUID()}.db"
        store = TaskStore(context, databaseName)
    }

    @After fun releaseTestOwnersAndDeleteOnlyItsJournal() {
        ownedIds.forEach { MobileRuntime.clearResume(it); MobileRuntime.release(it) }
        store.close()
        context.deleteDatabase(databaseName)
    }

    private fun newId(): String = UUID.randomUUID().toString().also { ownedIds.add(it) }

    @Test fun foregroundAndScheduledWorkerCannotOwnTheSameTaskTogether() {
        val id = newId()
        val foreground = TransferControl { true }
        val worker = TransferControl { true }
        assertTrue(MobileRuntime.claim(id, foreground))
        assertTrue(MobileRuntime.busy(id))
        assertFalse(MobileRuntime.claim(id, worker))
        foreground.check()
        worker.check()
        assertFalse(MobileRuntime.release(id))
        assertFalse(MobileRuntime.busy(id))
        assertTrue(MobileRuntime.claim(id, worker))
    }

    @Test fun stopFromAnotherComponentInterruptsTheOriginalOwnersControl() {
        val id = newId()
        val original = TransferControl { true }
        assertTrue(MobileRuntime.claim(id, original))
        MobileRuntime.stop(id)
        assertEquals("interrupted", assertThrows(TransferFailure::class.java) { original.check() }.code)
        // Ownership stays held until the old runner has unwound. A resume may
        // not start a second writer while the old writer is still exiting.
        assertTrue(MobileRuntime.busy(id))
        assertFalse(MobileRuntime.claim(id, TransferControl { true }))
        MobileRuntime.release(id)
        assertTrue(MobileRuntime.claim(id, TransferControl { true }))
    }

    @Test fun rapidResumeIsDeferredUntilOwnerExitAndConsumedOnlyOnce() {
        val id = newId()
        assertTrue(MobileRuntime.claim(id, TransferControl { true }))
        MobileRuntime.stop(id)
        MobileRuntime.requestResume(id)
        MobileRuntime.requestResume(id)
        assertFalse(MobileRuntime.claim(id, TransferControl { true }))
        assertTrue(MobileRuntime.release(id))
        assertFalse(MobileRuntime.release(id))
        assertFalse(MobileRuntime.busy(id))
        assertTrue(MobileRuntime.claim(id, TransferControl { true }))
        assertFalse(MobileRuntime.release(id))
    }

    @Test fun explicitCancelClearsDeferredResume() {
        val id = newId()
        assertTrue(MobileRuntime.claim(id, TransferControl { true }))
        MobileRuntime.requestResume(id)
        MobileRuntime.clearResume(id)
        MobileRuntime.stop(id)
        assertFalse(MobileRuntime.release(id))
        assertFalse(MobileRuntime.busy(id))
    }

    @Test fun repeatedInitializationCannotPauseAWorkerRunningInThisProcess() {
        MobileRuntime.initialize(store)
        val task = store.enqueue("https://example.com/active.zip", null, false, "Active", "file")
        ownedIds.add(task.id)
        assertTrue(MobileRuntime.claim(task.id, TransferControl { true }))
        store.state(task.id, TaskState.DOWNLOADING)
        // A service or activity may open a second helper for the same journal.
        val reopened = TaskStore(context, databaseName)
        try {
            MobileRuntime.initialize(reopened)
            assertEquals(TaskState.DOWNLOADING, reopened.get(task.id)!!.state)
            assertTrue(MobileRuntime.busy(task.id))
        } finally { reopened.close() }
    }

    @Test fun firstInitializationRecoversAnInterruptedJournalButOnlyOnce() {
        val task = store.enqueue("https://example.com/interrupted.zip", null, false, "Interrupted", "file")
        store.state(task.id, TaskState.SAVING)
        store.close()
        store = TaskStore(context, databaseName)
        MobileRuntime.initialize(store)
        assertEquals(TaskState.PAUSED, store.get(task.id)!!.state)
        assertEquals("interrupted", store.get(task.id)!!.error)
        // Once initialization completed, a new live transition must survive
        // another initialization even when no UI owns that task.
        store.state(task.id, TaskState.PROBING)
        MobileRuntime.initialize(store)
        assertEquals(TaskState.PROBING, store.get(task.id)!!.state)
    }

    @Test fun foregroundAndScheduledOwnersShareTheSameSingleTransferLimit() {
        val foregroundId = newId()
        val scheduledId = newId()
        assertTrue(MobileRuntime.claim(foregroundId, TransferControl { true }, limit = 1))
        assertFalse(MobileRuntime.claim(scheduledId, TransferControl { true }, limit = 1))
        assertTrue(MobileRuntime.busy(foregroundId))
        assertFalse(MobileRuntime.busy(scheduledId))
        MobileRuntime.release(foregroundId)
        assertTrue(MobileRuntime.claim(scheduledId, TransferControl { true }, limit = 1))
        assertFalse(MobileRuntime.claim(foregroundId, TransferControl { true }, limit = 1))
    }

    @Test fun thirdOwnerFillsSharedLimitAndReleaseReopensExactlyOneSlot() {
        val ids = List(4) { newId() }
        ids.take(3).forEach { assertTrue(MobileRuntime.claim(it, TransferControl { true }, limit = 3)) }
        assertFalse(MobileRuntime.claim(ids[3], TransferControl { true }, limit = 3))
        // A duplicate claim must neither replace the owner nor consume a slot.
        assertFalse(MobileRuntime.claim(ids[0], TransferControl { true }, limit = 3))
        MobileRuntime.release(ids[1])
        assertTrue(MobileRuntime.claim(ids[3], TransferControl { true }, limit = 3))
        assertFalse(MobileRuntime.claim(ids[1], TransferControl { true }, limit = 3))
    }

    @Test fun reducingConcurrencyDoesNotPermitNewWritersUntilExistingOwnersDrain() {
        val ids = List(4) { newId() }
        ids.take(3).forEach { assertTrue(MobileRuntime.claim(it, TransferControl { true }, limit = 3)) }
        assertFalse(MobileRuntime.claim(ids[3], TransferControl { true }, limit = 1))
        MobileRuntime.release(ids[0])
        MobileRuntime.release(ids[1])
        assertFalse(MobileRuntime.claim(ids[3], TransferControl { true }, limit = 1))
        assertTrue(MobileRuntime.busy(ids[2]))
        MobileRuntime.release(ids[2])
        assertTrue(MobileRuntime.claim(ids[3], TransferControl { true }, limit = 1))
    }
}
