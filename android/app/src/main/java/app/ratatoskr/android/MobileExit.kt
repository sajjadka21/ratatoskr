package app.ratatoskr.android

import android.app.job.JobScheduler
import android.content.Context
import android.content.Intent

/** Preserve checkpoints, disarm scheduled work and stop both execution surfaces. */
object MobileExit {
    fun stop(context: Context, store: TaskStore = TaskStore.get(context)) {
        store.list().filter { it.state in TaskPolicy.inFlight || it.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) }.forEach {
            MobileRuntime.clearResume(it.id)
            MobileRuntime.stop(it.id)
            store.state(it.id, TaskState.PAUSED)
            Engine.cancel(it.id)
        }
        context.getSystemService(JobScheduler::class.java).apply { cancel(NetworkJobs.JOB_ID); cancel(NetworkJobs.TIMER_JOB_ID) }
        context.stopService(Intent(context, DownloadService::class.java))
    }
}
