package app.ratatoskr.android

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.job.JobParameters
import android.app.job.JobService
import android.content.ContentValues
import android.content.Intent
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import androidx.core.app.NotificationCompat
import kotlinx.coroutines.*
import java.util.concurrent.ConcurrentHashMap

/** Regular OS jobs have a finite execution window. Checkpoint and reschedule
 * after five minutes instead of borrowing the foreground service's six hours. */
class NetworkJobService : JobService() {
    internal var taskStoreProvider: (android.content.Context) -> TaskStore = TaskStore::get
    internal var networkSnapshotProvider: (android.content.Context) -> NetworkSnapshot = MobileNetwork::snapshot
    internal var transferRunner: (android.content.Context, MobileTask, TransferControl, (TaskState) -> Unit, (Float) -> Unit) -> List<SavedMedia> = Engine::download
    internal var cancelTransfer: (String) -> Unit = Engine::cancel
    internal var discardTransfer: (android.content.Context, String) -> Unit = Engine::discard
    internal var ioDispatcher: CoroutineDispatcher = Dispatchers.IO
    internal var runBudgetMillis: Long = 5 * 60 * 1000
    internal var finishJob: (JobParameters, Boolean) -> Unit = { parameters, retry -> jobFinished(parameters, retry) }
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val active = ConcurrentHashMap<String, TransferControl>()
    private var stopped = false
    private var generation = 0L
    private lateinit var store: TaskStore
    private lateinit var prefs: MobilePreferences
    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onLost(network: Network) { scope.launch { enforceNetwork() } }
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) { scope.launch { enforceNetwork() } }
    }
    private fun allowed() = TaskPolicy.mayRun(prefs.networkPolicy, networkSnapshotProvider(this), prefs.allowRoaming) && Schedule.now(prefs.window)
    override fun onCreate() {
        super.onCreate(); store = taskStoreProvider(this); prefs = MobilePreferences(this); MobileRuntime.initialize(store)
        getSystemService(NotificationManager::class.java).createNotificationChannel(NotificationChannel("downloads", getString(R.string.notification_channel), NotificationManager.IMPORTANCE_LOW))
        getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(callback)
    }
    override fun onStartJob(params: JobParameters): Boolean {
        stopped = false
        val run = ++generation
        scope.launch {
            val deadline = android.os.SystemClock.elapsedRealtime() + runBudgetMillis
            if (!allowed()) { if (!stopped && run == generation) finishJob(params, true); return@launch }
            val jobs = store.list().filter { it.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) && !MobileRuntime.busy(it.id) && Schedule.isDue(it.startAt, System.currentTimeMillis()) }
            // Background slices are deliberately sequential; foreground user work
            // follows the selected 1–3 concurrency limit.
            for (task in jobs) {
                if (stopped || run != generation || !allowed() || android.os.SystemClock.elapsedRealtime() >= deadline) break
                val control = TransferControl { allowed() && !stopped && run == generation }
                var claimed = MobileRuntime.claim(task.id, control, prefs.concurrency)
                // An OS restart can overlap the old writer's bounded cleanup.
                // Give it time to release its slot without starting a duplicate.
                val waitUntil = minOf(deadline, android.os.SystemClock.elapsedRealtime() + 2000)
                while (!claimed && !stopped && run == generation && allowed() && android.os.SystemClock.elapsedRealtime() < waitUntil) {
                    delay(50)
                    claimed = MobileRuntime.claim(task.id, control, prefs.concurrency)
                }
                if (!claimed) continue
                if (!store.begin(task.id)) { MobileRuntime.release(task.id); continue }
                active[task.id] = control
                notifyTask(store.get(task.id)!!)
                val timer = launch {
                    delay((deadline - android.os.SystemClock.elapsedRealtime()).coerceAtLeast(1))
                    MobileRuntime.stop(task.id)
                    store.transitionActive(task.id, TaskState.PAUSED, "chunk_restart")
                    cancelTransfer(task.id)
                }
                try {
                    withContext(ioDispatcher) {
                        transferRunner(this@NetworkJobService, task, control, { state ->
                            control.check(); if (!store.transitionActive(task.id, state)) throw TransferFailure("interrupted")
                        }, { percent ->
                            control.check()
                            if (!store.updateActive(task.id, ContentValues().apply { put("progress", percent.toInt().coerceIn(0, 99)) })) throw TransferFailure("interrupted")
                            notifyTask(store.get(task.id)!!)
                        })
                        control.check()
                        store.updateActive(task.id, ContentValues().apply { put("state", TaskState.COMPLETED.name); put("progress", 100); put("error", "") })
                    }
                } catch (error: Exception) {
                    val code = DownloadService.errorCode(error)
                    store.transitionActive(task.id, if (code == "waiting_network") TaskState.WAITING_NETWORK else if (error is CancellationException) TaskState.PAUSED else TaskState.FAILED, code)
                } finally {
                    timer.cancel(); active.remove(task.id)
                    val resumed = MobileRuntime.release(task.id)
                    val current = store.get(task.id)!!
                    if (current.state == TaskState.PAUSED && (current.error == "chunk_restart" || resumed)) store.state(task.id, if (allowed()) TaskState.QUEUED else TaskState.WAITING_NETWORK)
                    if (current.state == TaskState.CANCELLED) withContext(NonCancellable + ioDispatcher) { discardTransfer(this@NetworkJobService, task.id) }
                    notifyTask(store.get(task.id)!!)
                }
            }
            if (!stopped && run == generation) {
                // Tasks waiting for a start time or the download window are woken by their own timer, not by retrying.
                val now = System.currentTimeMillis()
                finishJob(params, store.list().any { it.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) && Schedule.isDue(it.startAt, now) && Schedule.now(prefs.window) })
                NetworkJobs.schedule(this@NetworkJobService, store)
            }
        }
        return true
    }
    private fun enforceNetwork() {
        if (!allowed()) for ((id, control) in active) {
            control.stop(); store.transitionActive(id, TaskState.WAITING_NETWORK); cancelTransfer(id)
        }
    }
    internal fun networkChanged() = enforceNetwork()
    override fun onStopJob(params: JobParameters): Boolean {
        stopped = true
        for ((id, control) in active) {
            control.stop(); store.transitionActive(id, TaskState.PAUSED, "chunk_restart"); cancelTransfer(id)
        }
        // Preserve ownership until each blocking native call has actually exited.
        return store.list().any { it.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) || it.error == "chunk_restart" }
    }
    private fun notifyTask(task: MobileTask) {
        val home = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val notice = NotificationCompat.Builder(this, "downloads").setSmallIcon(R.drawable.ic_stat_download)
            .setContentTitle(task.title.ifEmpty { "Ratatoskr" }).setContentText(MobileText.state(this, task))
            .setContentIntent(home).setOnlyAlertOnce(true).setAutoCancel(task.state == TaskState.COMPLETED)
        if (task.state in TaskPolicy.inFlight) notice.setProgress(100, task.progress, task.progress == 0)
        getSystemService(NotificationManager::class.java).notify(task.notificationId, notice.build())
    }
    override fun onDestroy() {
        stopped = true
        for ((id, control) in active) { control.stop(); store.transitionActive(id, TaskState.PAUSED, "chunk_restart"); cancelTransfer(id) }
        runCatching { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(callback) }
        scope.cancel(); super.onDestroy()
    }
}
