package app.ratatoskr.android

import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.Context
import android.net.NetworkCapabilities
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertNotSame
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
class NetworkJobsTest {
    private lateinit var context: Context
    private lateinit var store: TaskStore
    private lateinit var databaseName: String
    private lateinit var scheduler: JobScheduler
    private lateinit var preferences: MobilePreferences
    private val ownedIds = mutableListOf<String>()

    @Before fun createIsolatedNetworkQueue() {
        context = RuntimeEnvironment.getApplication()
        databaseName = "network-job-test-${UUID.randomUUID()}.db"
        store = TaskStore(context, databaseName)
        scheduler = context.getSystemService(JobScheduler::class.java)
        scheduler.cancelAll()
        context.getSharedPreferences("download_preferences", Context.MODE_PRIVATE).edit().clear().commit()
        preferences = MobilePreferences(context)
    }

    @After fun removeOnlyTestQueueAndPreferences() {
        ownedIds.forEach { MobileRuntime.clearResume(it); MobileRuntime.release(it) }
        scheduler.cancelAll()
        store.close()
        context.deleteDatabase(databaseName)
        context.getSharedPreferences("download_preferences", Context.MODE_PRIVATE).edit().clear().commit()
    }

    private fun queuedTask(state: TaskState = TaskState.QUEUED): MobileTask {
        val task = store.enqueue("https://example.com/${UUID.randomUUID()}.zip", null, false, "Queue", "file")
        store.state(task.id, state)
        return task
    }

    private fun scheduledJob(): JobInfo {
        NetworkJobs.schedule(context, store)
        val job = scheduler.getPendingJob(NetworkJobs.JOB_ID)
        assertNotNull("A durable network job must replace an idle foreground service", job)
        return job!!
    }

    @Test fun wifiOnlyAllowsMeteredWifiButNeverMatchesCellular() {
        queuedTask(TaskState.WAITING_NETWORK)
        preferences.networkPolicy = NetworkPolicy.WIFI_ONLY
        val job = scheduledJob()
        val required = job.requiredNetwork!!
        assertTrue(required.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET))
        assertTrue(required.hasTransport(NetworkCapabilities.TRANSPORT_WIFI))
        assertFalse(required.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED))
        assertTrue(job.isPersisted)
        assertEquals(NetworkJobService::class.java.name, job.service.className)
    }

    @Test fun unmeteredPolicyDoesNotRequireWifiTransport() {
        queuedTask()
        preferences.networkPolicy = NetworkPolicy.UNMETERED
        val required = scheduledJob().requiredNetwork!!
        assertTrue(required.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET))
        assertTrue(required.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED))
        assertFalse(required.hasTransport(NetworkCapabilities.TRANSPORT_WIFI))
    }

    @Test fun anyNetworkStillExcludesRoamingUntilExplicitlyAllowed() {
        queuedTask()
        preferences.networkPolicy = NetworkPolicy.ANY
        preferences.allowRoaming = false
        val protected = scheduledJob().requiredNetwork!!
        assertTrue(protected.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET))
        assertTrue(protected.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_ROAMING))
        assertFalse(protected.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED))
        assertFalse(protected.hasTransport(NetworkCapabilities.TRANSPORT_WIFI))
        preferences.allowRoaming = true
        val allowed = scheduledJob().requiredNetwork!!
        assertFalse(allowed.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_ROAMING))
    }

    @Test fun queuedAndWaitingTasksShareOneScheduledWakeup() {
        queuedTask(TaskState.QUEUED)
        queuedTask(TaskState.WAITING_NETWORK)
        val first = scheduledJob()
        NetworkJobs.schedule(context, store)
        assertEquals(1, scheduler.allPendingJobs.size)
        assertEquals(first.id, scheduler.allPendingJobs.single().id)
    }

    @Test fun userPausedAndTerminalTasksDoNotScheduleAutomaticRetries() {
        listOf(TaskState.PAUSED, TaskState.FAILED, TaskState.COMPLETED, TaskState.CANCELLED, TaskState.NEEDS_SELECTION).forEach { queuedTask(it) }
        NetworkJobs.schedule(context, store)
        assertNull(scheduler.getPendingJob(NetworkJobs.JOB_ID))
    }

    @Test fun pausingLastWaitingTaskCancelsItsPreviouslyScheduledWakeup() {
        val task = queuedTask(TaskState.WAITING_NETWORK)
        scheduledJob()
        store.state(task.id, TaskState.PAUSED)
        NetworkJobs.schedule(context, store)
        assertNull(scheduler.getPendingJob(NetworkJobs.JOB_ID))
        assertEquals(TaskState.PAUSED, store.get(task.id)!!.state)
    }

    @Test fun changingPolicyReplacesOldJobConstraintsWithoutDuplicatingJobs() {
        queuedTask(TaskState.WAITING_NETWORK)
        preferences.networkPolicy = NetworkPolicy.ANY
        scheduledJob()
        preferences.networkPolicy = NetworkPolicy.WIFI_ONLY
        NetworkJobs.schedule(context, store)
        assertEquals(1, scheduler.allPendingJobs.size)
        assertTrue(scheduler.getPendingJob(NetworkJobs.JOB_ID)!!.requiredNetwork!!.hasTransport(NetworkCapabilities.TRANSPORT_WIFI))
    }

    @Test fun reopeningTheAppDoesNotCancelTheOnlyDownloadWhileItsWorkerOwnsIt() {
        MobileRuntime.initialize(store)
        val task = queuedTask(TaskState.WAITING_NETWORK)
        val originalJob = scheduledJob()
        val control = TransferControl { true }
        ownedIds.add(task.id)
        assertTrue(MobileRuntime.claim(task.id, control))
        store.state(task.id, TaskState.DOWNLOADING)

        // MainActivity calls schedule again when reopened. No queued task is
        // left, but the running job must retain the worker that owns its file.
        NetworkJobs.schedule(context, store)

        assertSame(originalJob, scheduler.getPendingJob(NetworkJobs.JOB_ID))
        assertTrue(MobileRuntime.busy(task.id))
        assertEquals(TaskState.DOWNLOADING, store.get(task.id)!!.state)
        control.check()
    }

    @Test fun anotherQueuedDownloadDoesNotReplaceTheRunningJobWithUnchangedPolicy() {
        MobileRuntime.initialize(store)
        val active = queuedTask(TaskState.WAITING_NETWORK)
        val originalJob = scheduledJob()
        ownedIds.add(active.id)
        assertTrue(MobileRuntime.claim(active.id, TransferControl { true }))
        store.state(active.id, TaskState.DOWNLOADING)
        val next = queuedTask()

        NetworkJobs.schedule(context, store)

        assertSame("Scheduling the next item must not replace and stop its current worker",
            originalJob, scheduler.getPendingJob(NetworkJobs.JOB_ID))
        assertEquals(1, scheduler.allPendingJobs.size)
        assertTrue(MobileRuntime.busy(active.id))
        assertEquals(TaskState.QUEUED, store.get(next.id)!!.state)
    }

    @Test fun anExplicitPolicyChangeStillReplacesTheOldJobWhileAWorkerIsLive() {
        MobileRuntime.initialize(store)
        val active = queuedTask(TaskState.WAITING_NETWORK)
        preferences.networkPolicy = NetworkPolicy.ANY
        val originalJob = scheduledJob()
        ownedIds.add(active.id)
        assertTrue(MobileRuntime.claim(active.id, TransferControl { true }))
        store.state(active.id, TaskState.DOWNLOADING)
        queuedTask()
        preferences.networkPolicy = NetworkPolicy.WIFI_ONLY

        NetworkJobs.schedule(context, store)

        val changed = scheduler.getPendingJob(NetworkJobs.JOB_ID)!!
        assertNotSame(originalJob, changed)
        assertTrue(changed.requiredNetwork!!.hasTransport(NetworkCapabilities.TRANSPORT_WIFI))
        assertEquals(1, scheduler.allPendingJobs.size)
    }

    @Test fun terminalTasksWithNoLiveOwnerCancelTheirObsoleteScheduledJob() {
        listOf(TaskState.COMPLETED, TaskState.CANCELLED).forEach { terminal ->
            val task = queuedTask(TaskState.WAITING_NETWORK)
            scheduledJob()
            assertFalse(MobileRuntime.busy(task.id))
            store.state(task.id, terminal)

            NetworkJobs.schedule(context, store)

            assertNull(scheduler.getPendingJob(NetworkJobs.JOB_ID))
            assertEquals(terminal, store.get(task.id)!!.state)
        }
    }
}
