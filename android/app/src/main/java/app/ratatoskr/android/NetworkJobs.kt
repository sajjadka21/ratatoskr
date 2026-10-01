package app.ratatoskr.android

import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import android.net.NetworkCapabilities
import android.net.NetworkRequest

/** Sleeping queues consume no dataSync foreground budget. The OS wakes a
 * bounded, checkpointable slice when the user's network constraint is met. */
object NetworkJobs {
    const val JOB_ID = 0x524154
    const val TIMER_JOB_ID = 0x524155
    fun schedule(context: Context, store: TaskStore = TaskStore.get(context)) {
        val scheduler = context.getSystemService(JobScheduler::class.java)
        val tasks = store.list()
        val prefs = MobilePreferences(context)
        val now = System.currentTimeMillis()
        val waiting = tasks.filter { it.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) }
        // A task with a start time, or any task while the download window is closed, is woken by a timer.
        val component = ComponentName(context, NetworkJobService::class.java)
        val wake = Schedule.wake(waiting.map { it.startAt }, prefs.window, now)
        if (wake != null && wake > 0) {
            scheduler.schedule(JobInfo.Builder(TIMER_JOB_ID, component).setMinimumLatency(wake).setOverrideDeadline(wake + 60_000).setPersisted(true).build())
        } else scheduler.cancel(TIMER_JOB_ID)
        val runnable = waiting.filter { Schedule.isDue(it.startAt, now) } .takeIf { Schedule.now(prefs.window) }.orEmpty()
        if (runnable.isEmpty()) {
            if (tasks.none { MobileRuntime.busy(it.id) }) scheduler.cancel(JOB_ID)
            return
        }
        val network = NetworkRequest.Builder().addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
        when (prefs.networkPolicy) {
            NetworkPolicy.WIFI_ONLY -> network.addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            NetworkPolicy.UNMETERED -> network.addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
            NetworkPolicy.ANY -> Unit
        }
        if (!prefs.allowRoaming) network.addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_ROAMING)
        val requirement = network.build()
        val existing = scheduler.getPendingJob(JOB_ID)
        val modern = android.os.Build.VERSION.SDK_INT >= 28
        if (existing?.service == component && (!modern || existing.requiredNetwork == requirement)) return
        val job = JobInfo.Builder(JOB_ID, component).setPersisted(true).setBackoffCriteria(30000, JobInfo.BACKOFF_POLICY_EXPONENTIAL)
        // Android 9 can ask for an exact network description; Android 8 only for "any" or "unmetered".
        if (modern) job.setRequiredNetwork(requirement)
        else job.setRequiredNetworkType(if (prefs.networkPolicy == NetworkPolicy.ANY) JobInfo.NETWORK_TYPE_ANY else JobInfo.NETWORK_TYPE_UNMETERED)
        scheduler.schedule(job.build())
    }
}
