package app.ratatoskr.android

import android.app.job.JobParameters
import android.content.Context
import android.os.Bundle
import android.os.Looper
import android.os.PersistableBundle
import kotlinx.coroutines.asCoroutineDispatcher
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ServiceController
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode
import org.robolectric.annotation.SQLiteMode
import java.io.File
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/** OS-job lifecycle with real SQLite and controlled blocking IO. In particular,
 * onStopJob does not mean the old native writer has already left its file. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
@LooperMode(LooperMode.Mode.PAUSED)
@SQLiteMode(SQLiteMode.Mode.NATIVE)
class NetworkJobServiceTest {
    private lateinit var context: Context
    private lateinit var store: TaskStore
    private lateinit var databaseName: String
    private lateinit var controller: ServiceController<NetworkJobService>
    private lateinit var service: NetworkJobService
    private lateinit var partial: File
    private val executor = Executors.newSingleThreadExecutor()
    private val io = executor.asCoroutineDispatcher()
    private val entered = CountDownLatch(1)
    private val release = CountDownLatch(1)
    private val returned = CountDownLatch(1)
    private val launches = AtomicInteger(0)
    private val discards = AtomicInteger(0)
    private val finishes = mutableListOf<Pair<JobParameters, Boolean>>()
    @Volatile private var network = NetworkSnapshot(true, true, false)
    private var afterRelease: ((TransferControl, (TaskState) -> Unit, (Float) -> Unit) -> Unit)? = null
    private var destroyed = false

    @Before fun prepareWorkerWithoutNativeExtractorOrSockets() {
        context = RuntimeEnvironment.getApplication()
        databaseName = "job-service-${UUID.randomUUID()}.db"
        store = TaskStore(context, databaseName)
        context.getSharedPreferences("download_preferences", Context.MODE_PRIVATE).edit().clear().commit()
        partial = File(context.filesDir, "job-service-${UUID.randomUUID()}.part")
        partial.writeText("saved partial bytes")
        controller = Robolectric.buildService(NetworkJobService::class.java)
        service = controller.get()
        service.taskStoreProvider = { store }
        service.networkSnapshotProvider = { network }
        service.cancelTransfer = { Unit }
        service.discardTransfer = { _, _ -> discards.incrementAndGet(); Unit }
        service.ioDispatcher = io
        service.runBudgetMillis = 60_000
        service.finishJob = { params, retry -> finishes.add(params to retry) }
        service.transferRunner = { _, _, control, onState, onProgress ->
            launches.incrementAndGet()
            try {
                onState(TaskState.DOWNLOADING)
                entered.countDown()
                assertTrue("release job transfer fixture", release.await(5, TimeUnit.SECONDS))
                afterRelease?.invoke(control, onState, onProgress)
                emptyList()
            } finally { returned.countDown() }
        }
        controller.create()
    }

    @After fun releaseOnlyTestWorkerAndItsFixture() {
        release.countDown()
        if (!destroyed) controller.destroy()
        shadowOf(Looper.getMainLooper()).idle()
        io.close()
        assertTrue("job test worker exits", executor.awaitTermination(5, TimeUnit.SECONDS))
        shadowOf(Looper.getMainLooper()).idle()
        store.list().forEach { MobileRuntime.clearResume(it.id); MobileRuntime.release(it.id) }
        store.close()
        context.deleteDatabase(databaseName)
        assertTrue(partial.delete())
        context.getSharedPreferences("download_preferences", Context.MODE_PRIVATE).edit().clear().commit()
    }

    private fun params(): JobParameters {
        // The public hidden constructor varies across Android SDK releases.
        // Select SDK29's callback constructor by type instead of binding tests
        // to an unrelated newer constructor signature or adding Mockito.
        val constructor = JobParameters::class.java.declaredConstructors.first {
            it.parameterTypes.any { type -> type == android.os.IBinder::class.java }
        }
        constructor.isAccessible = true
        val args = constructor.parameterTypes.map { type ->
            when (type) {
                Int::class.javaPrimitiveType -> 1
                Boolean::class.javaPrimitiveType -> false
                Long::class.javaPrimitiveType -> 0L
                Bundle::class.java -> Bundle()
                PersistableBundle::class.java -> PersistableBundle()
                else -> if (type.isArray) java.lang.reflect.Array.newInstance(type.componentType, 0) else null
            }
        }.toTypedArray()
        return constructor.newInstance(*args) as JobParameters
    }

    private fun awaitMain(message: String, condition: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        do {
            shadowOf(Looper.getMainLooper()).idleFor(10, TimeUnit.MILLISECONDS)
            if (condition()) return
            Thread.sleep(5)
        } while (System.nanoTime() < deadline)
        assertTrue(message, condition())
    }

    private fun start(): Pair<MobileTask, JobParameters> {
        val task = store.enqueue("https://example.com/${UUID.randomUUID()}.zip", null, false, "Worker", "file")
        val parameters = params()
        assertTrue(service.onStartJob(parameters))
        awaitMain("background worker begins") { entered.count == 0L }
        return task to parameters
    }

    private fun finishTransfer() {
        release.countDown()
        awaitMain("transfer fixture returns") { returned.count == 0L }
        repeat(3) {
            executor.submit(Runnable {}).get(5, TimeUnit.SECONDS)
            shadowOf(Looper.getMainLooper()).idle()
        }
    }

    @Test fun userPauseWinsOverLateProgressAndStateCallbacks() {
        afterRelease = { _, onState, onProgress -> onProgress(80f); onState(TaskState.MERGING) }
        val (task, _) = start()
        store.state(task.id, TaskState.PAUSED)
        MobileRuntime.stop(task.id)
        finishTransfer()
        assertEquals(TaskState.PAUSED, store.get(task.id)!!.state)
        assertEquals(0, store.get(task.id)!!.progress)
        assertEquals("", store.get(task.id)!!.error)
        assertEquals(0, discards.get())
        assertTrue(partial.exists())
    }

    @Test fun chunkDeadlineDoesNotTurnAnExplicitUserPauseIntoAutomaticRetry() {
        service.runBudgetMillis = 1_000
        afterRelease = { control, _, _ -> control.check() }
        val (task, _) = start()
        store.state(task.id, TaskState.PAUSED)
        MobileRuntime.stop(task.id)
        shadowOf(Looper.getMainLooper()).idleFor(1_100, TimeUnit.MILLISECONDS)
        finishTransfer()
        assertEquals(TaskState.PAUSED, store.get(task.id)!!.state)
        assertEquals("", store.get(task.id)!!.error)
        assertEquals(0, discards.get())
        assertTrue(finishes.none { it.second })
    }

    @Test fun boundedSliceCheckpointsAndRequestsAnotherOsWindow() {
        service.runBudgetMillis = 1_000
        afterRelease = { control, _, _ -> control.check() }
        val (task, parameters) = start()
        shadowOf(Looper.getMainLooper()).idleFor(1_100, TimeUnit.MILLISECONDS)
        assertTrue("old writer remains owned until it returns", MobileRuntime.busy(task.id))
        finishTransfer()
        assertEquals(TaskState.QUEUED, store.get(task.id)!!.state)
        assertFalse(MobileRuntime.busy(task.id))
        assertTrue(finishes.any { it.first === parameters && it.second })
        assertEquals(0, discards.get())
        assertTrue(partial.exists())
    }

    @Test fun switchingFromWifiToCellularPreservesWaitingNetworkAcrossOsStop() {
        MobilePreferences(context).networkPolicy = NetworkPolicy.WIFI_ONLY
        afterRelease = { control, _, _ -> control.check() }
        val (task, parameters) = start()
        network = NetworkSnapshot(true, false, true)
        service.networkChanged()
        service.onStopJob(parameters)
        finishTransfer()
        assertEquals(TaskState.WAITING_NETWORK, store.get(task.id)!!.state)
        assertEquals(1, launches.get())
        assertEquals(0, discards.get())
        assertTrue(finishes.none { it.first === parameters })
    }

    @Test fun aNewOsStartCannotLetTheOldRunFinishTheNewParameters() {
        afterRelease = { control, _, _ -> control.check() }
        val (first, oldParams) = start()
        service.onStopJob(oldParams)
        assertTrue(MobileRuntime.busy(first.id))
        val second = store.enqueue("https://example.com/next-window.zip", null, false, "Next window", "file")
        val newParams = params()
        assertTrue(service.onStartJob(newParams))
        shadowOf(Looper.getMainLooper()).idle()
        finishTransfer()
        awaitMain("new window completes independently") { finishes.any { it.first === newParams } }
        assertTrue(finishes.none { it.first === oldParams })
        assertEquals(2, launches.get())
        assertEquals(TaskState.COMPLETED, store.get(second.id)!!.state)
        assertEquals(0, discards.get())
    }

    @Test fun recreatingForegroundServiceJournalDoesNotRecoverALiveScheduledWriter() {
        afterRelease = { control, _, _ -> control.check() }
        val (task, _) = start()
        TaskStore(context, databaseName).use { reopened ->
            MobileRuntime.initialize(reopened)
            assertEquals(TaskState.DOWNLOADING, reopened.get(task.id)!!.state)
        }
        assertTrue(MobileRuntime.busy(task.id))
        finishTransfer()
        assertEquals(TaskState.COMPLETED, store.get(task.id)!!.state)
    }

    @Test fun userCancellationCleansPartialOnlyAfterOldWriterExits() {
        afterRelease = { control, _, _ -> control.check() }
        val (task, _) = start()
        store.state(task.id, TaskState.CANCELLED)
        MobileRuntime.clearResume(task.id)
        MobileRuntime.stop(task.id)
        assertEquals(0, discards.get())
        assertTrue(MobileRuntime.busy(task.id))
        finishTransfer()
        assertEquals(TaskState.CANCELLED, store.get(task.id)!!.state)
        assertEquals("", store.get(task.id)!!.error)
        assertEquals(1, discards.get())
    }
}
