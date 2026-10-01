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
    fun schedule(context: Context, store: TaskStore = TaskStore.get(context)) {
        val scheduler = context.getSystemService(JobScheduler::class.java)
        if (store.list().none { it.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) }) { scheduler.cancel(JOB_ID); return }
        val prefs = MobilePreferences(context)
        val network = NetworkRequest.Builder().addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
        when (prefs.networkPolicy) {
            NetworkPolicy.WIFI_ONLY -> network.addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            NetworkPolicy.UNMETERED -> network.addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
            NetworkPolicy.ANY -> Unit
        }
        if (!prefs.allowRoaming) network.addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_ROAMING)
        scheduler.schedule(JobInfo.Builder(JOB_ID, ComponentName(context, NetworkJobService::class.java))
            .setRequiredNetwork(network.build()).setPersisted(true).setBackoffCriteria(30000, JobInfo.BACKOFF_POLICY_EXPONENTIAL).build())
    }
}
