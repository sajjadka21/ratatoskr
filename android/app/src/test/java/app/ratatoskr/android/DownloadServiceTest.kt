package app.ratatoskr.android

import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.os.Looper
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.asCoroutineDispatcher
import org.junit.After
import org.junit.Assert.assertEquals
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
import java.io.IOException
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/** Exercise real lifecycle, scheduling, SQLite and notifications, while the
 * injected transfer/network seams keep native extractors and sockets unused. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
@LooperMode(LooperMode.Mode.PAUSED)
@SQLiteMode(SQLiteMode.Mode.NATIVE)
class DownloadServiceTest {
    private lateinit var context: Context
    private lateinit var store: TaskStore
    private lateinit var databaseName: String
    private lateinit var controller: ServiceController<DownloadService>
    private lateinit var service: DownloadService
    private lateinit var partial: File
    private var destroyed = false
    private val workerThread = AtomicReference<Thread>()
    private val executor = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "test-transfer").also { workerThread.set(it) }
    }
    private val io = executor.asCoroutineDispatcher()
    private val started = CountDownLatch(1)
    private val releaseRunner = CountDownLatch(1)
    private val checkEntered = CountDownLatch(1)
    private val releaseCheck = CountDownLatch(1)
    private val runnerFinished = CountDownLatch(1)
    private val blockNextWorkerCheck = AtomicBoolean(false)
    private val launches = AtomicInteger(0)
    private val cancellations = AtomicInteger(0)
    private val discards = AtomicInteger(0)
    @Volatile private var network = NetworkSnapshot(true, true, false)
    private var afterRelease: ((TransferControl, (TaskState) -> Unit, (Float) -> Unit) -> Unit)? = null

    @Before fun createServiceWithIsolatedJournalAndDeterministicTransfer() {
        context = RuntimeEnvironment.getApplication()
        databaseName = "service-test-${UUID.randomUUID()}.db"
        store = TaskStore(context, databaseName)
        context.getSharedPreferences("download_preferences", Context.MODE_PRIVATE).edit().clear().commit()
        MobilePreferences(context).concurrency = 1
        partial = File(context.filesDir, "service-test-${UUID.randomUUID()}.part")
        partial.writeText("already transferred bytes")
        controller = Robolectric.buildService(DownloadService::class.java)
        service = controller.get()
        service.taskStoreProvider = { store }
        service.networkSnapshotProvider = {
            val captured = network
            // Coroutine debugging appends its identifier to the thread name
            // under Gradle's assertions-enabled JVM; object identity is stable.
            if (Thread.currentThread() === workerThread.get() && blockNextWorkerCheck.compareAndSet(true, false)) {
                checkEntered.countDown()
                assertTrue("release blocked policy check", releaseCheck.await(5, TimeUnit.SECONDS))
            }
            captured
        }
        service.cancelTransfer = { cancellations.incrementAndGet(); Unit }
        service.discardTransfer = { _, _ -> discards.incrementAndGet(); Unit }
        service.ioDispatcher = io
        service.transferRunner = { _, _, control, onState, onProgress ->
            launches.incrementAndGet()
            try {
                onState(TaskState.DOWNLOADING)
                started.countDown()
                assertTrue("release transfer fixture", releaseRunner.await(5, TimeUnit.SECONDS))
                afterRelease?.invoke(control, onState, onProgress)
                emptyList()
            } finally { runnerFinished.countDown() }
        }
        controller.create()
    }

    @After fun stopServiceAndRemoveOnlyTestFixtures() {
        releaseCheck.countDown()
        releaseRunner.countDown()
        if (!destroyed) controller.destroy()
        shadowOf(Looper.getMainLooper()).idle()
        io.close()
        assertTrue("test worker terminates", executor.awaitTermination(5, TimeUnit.SECONDS))
        shadowOf(Looper.getMainLooper()).idle()
        store.close()
        context.deleteDatabase(databaseName)
        assertTrue(partial.delete())
        context.getSharedPreferences("download_preferences", Context.MODE_PRIVATE).edit().clear().commit()
    }

    private fun awaitMain(message: String, condition: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        do {
            shadowOf(Looper.getMainLooper()).idle()
            if (condition()) return
            Thread.sleep(5)
        } while (System.nanoTime() < deadline)
        assertTrue(message, condition())
    }

    private fun start(): MobileTask {
        val task = store.enqueue("https://example.com/service-test.zip", null, false, "Test job", "file")
        store.update(task.id, ContentValues().apply { put("bytes_done", partial.length()) })
        service.onStartCommand(Intent(context, DownloadService::class.java), 0, 1)
        awaitMain("transfer starts") { started.count == 0L }
        return task
    }

    private fun command(id: String, action: String) {
        service.onStartCommand(Intent(context, DownloadService::class.java).setAction(action)
            .putExtra(DownloadService.EXTRA_PROCESS, id), 0, 2)
    }

    private fun finish() {
        releaseCheck.countDown()
        releaseRunner.countDown()
        awaitMain("transfer fixture returns") { runnerFinished.count == 0L }
        // Wait for withContext's completion and its main-thread finally block,
        // rather than treating the fake runner's return as service completion.
        // Drain both directions: IO completion -> main finally -> optional IO
        // discard -> main scheduler. A single idle can miss the cleanup hop.
        repeat(3) {
            executor.submit(Runnable {}).get(5, TimeUnit.SECONDS)
            shadowOf(Looper.getMainLooper()).idle()
        }
    }

    @Test fun pauseWinsAgainstAStateCallbackThatAlreadyEnteredItsPolicyCheck() {
        afterRelease = { _, onState, _ -> onState(TaskState.MERGING) }
        val task = start()
        blockNextWorkerCheck.set(true)
        releaseRunner.countDown()
        assertTrue("worker enters callback policy check", checkEntered.await(5, TimeUnit.SECONDS))

        command(task.id, DownloadService.ACTION_PAUSE)
        finish()

        assertEquals(TaskState.PAUSED, store.get(task.id)!!.state)
        assertTrue(partial.exists())
        assertEquals(partial.length(), store.get(task.id)!!.bytesDone)
        assertEquals(0, discards.get())
    }

    @Test fun pauseWinsAgainstAnAlreadyCheckedProgressUpdate() {
        afterRelease = { _, _, onProgress -> onProgress(80f) }
        val task = start()
        blockNextWorkerCheck.set(true)
        releaseRunner.countDown()
        assertTrue("worker enters progress policy check", checkEntered.await(5, TimeUnit.SECONDS))

        command(task.id, DownloadService.ACTION_PAUSE)
        finish()

        assertEquals(TaskState.PAUSED, store.get(task.id)!!.state)
        assertEquals(0, store.get(task.id)!!.progress)
        assertEquals(0, discards.get())
    }

    @Test fun userCancellationCannotBecomeFailedWhenTheStoppedEngineThrows() {
        afterRelease = { _, _, _ -> throw IOException("403 after cancellation") }
        val task = start()

        command(task.id, DownloadService.ACTION_CANCEL)
        finish()

        assertEquals(TaskState.CANCELLED, store.get(task.id)!!.state)
        assertEquals("", store.get(task.id)!!.error)
        assertTrue(cancellations.get() > 0)
        assertEquals(1, discards.get())
    }

    @Test fun immediateResumeIsNotLostToThePreviousPausedWorkersFailure() {
        afterRelease = { control, _, _ -> control.check() }
        val task = start()
        command(task.id, DownloadService.ACTION_PAUSE)
        command(task.id, DownloadService.ACTION_RESUME)

        finish()

        assertEquals(2, launches.get())
        assertEquals(TaskState.COMPLETED, store.get(task.id)!!.state)
        assertEquals("", store.get(task.id)!!.error)
        assertEquals(0, discards.get())
    }

    @Test fun wifiOnlyTransferWaitsAfterSwitchingToCellularAndDoesNotRestartThere() {
        MobilePreferences(context).networkPolicy = NetworkPolicy.WIFI_ONLY
        afterRelease = { control, _, _ -> control.check() }
        val task = start()

        network = NetworkSnapshot(true, false, true)
        service.networkChanged()
        finish()

        assertEquals(TaskState.WAITING_NETWORK, store.get(task.id)!!.state)
        assertEquals(1, launches.get())
        assertEquals(0, discards.get())
        assertTrue(partial.exists())
    }

    @Test fun timeoutPreservesPartialAndCannotScheduleNewWorkFromFinally() {
        afterRelease = { control, _, _ -> control.check() }
        val active = start()
        val pending = store.enqueue("https://example.com/pending.zip", null, false, "Pending", "file")

        service.onTimeout(1, android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        // A connectivity callback already queued on the main looper must not
        // turn timeout pauses into automatically resumable network waits.
        network = NetworkSnapshot(false, false, true)
        service.networkChanged()
        val later = store.enqueue("https://example.com/later.zip", null, false, "Added after timeout", "file")
        finish()

        assertEquals(TaskState.PAUSED, store.get(active.id)!!.state)
        assertEquals("system_timeout", store.get(active.id)!!.error)
        assertEquals(TaskState.PAUSED, store.get(pending.id)!!.state)
        assertEquals("system_timeout", store.get(pending.id)!!.error)
        assertEquals(TaskState.QUEUED, store.get(later.id)!!.state)
        assertEquals(1, launches.get())
        assertEquals(0, discards.get())
        assertTrue(partial.exists())
    }

    @Test fun networkCallbackCannotUndoAnExplicitUserPauseWhileWorkerExits() {
        afterRelease = { control, _, _ -> control.check() }
        val task = start()
        command(task.id, DownloadService.ACTION_PAUSE)

        network = NetworkSnapshot(false, false, true)
        service.networkChanged()
        finish()

        assertEquals(TaskState.PAUSED, store.get(task.id)!!.state)
        assertEquals(0, discards.get())
    }

    @Test fun destructionPreservesWaitingNetworkInsteadOfTurningItIntoUserPause() {
        MobilePreferences(context).networkPolicy = NetworkPolicy.WIFI_ONLY
        afterRelease = { control, _, _ -> control.check() }
        val task = start()
        network = NetworkSnapshot(true, false, true)
        service.networkChanged()

        controller.destroy()
        destroyed = true
        finish()

        assertEquals(TaskState.WAITING_NETWORK, store.get(task.id)!!.state)
        assertEquals(0, discards.get())
    }

    @Test fun returningToWifiResumesTheWaitingJobWithoutFailingItsFirstStateChange() {
        MobilePreferences(context).networkPolicy = NetworkPolicy.WIFI_ONLY
        afterRelease = { control, _, _ -> control.check() }
        val task = start()
        network = NetworkSnapshot(true, false, true)
        service.networkChanged()
        finish()
        assertEquals(TaskState.WAITING_NETWORK, store.get(task.id)!!.state)
        network = NetworkSnapshot(true, true, false)
        service.networkChanged()
        awaitMain("network recovery completes the second run") { store.get(task.id)!!.state == TaskState.COMPLETED }
        assertEquals(2, launches.get())
        assertEquals("", store.get(task.id)!!.error)
    }

    @Test fun coroutineCancellationLeavesRecoverablePauseRatherThanFailedHistory() {
        afterRelease = { _, _, _ -> throw CancellationException("fixture cancellation") }
        val task = start()
        finish()
        assertEquals(TaskState.PAUSED, store.get(task.id)!!.state)
        assertEquals(0, discards.get())
        assertTrue(partial.exists())
    }
}
