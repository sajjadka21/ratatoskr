package app.ratatoskr.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.IBinder
import androidx.core.app.NotificationCompat
import kotlinx.coroutines.*

/** SQLite owns every job; this service only schedules and executes persisted work. */
class DownloadService : Service() {
    internal var taskStoreProvider: (Context) -> TaskStore = TaskStore::get
    internal var networkSnapshotProvider: (Context) -> NetworkSnapshot = MobileNetwork::snapshot
    internal var transferRunner: (Context, MobileTask, TransferControl, (TaskState) -> Unit, (Float) -> Unit) -> List<SavedMedia> = Engine::download
    internal var cancelTransfer: (String) -> Unit = Engine::cancel
    internal var discardTransfer: (Context, String) -> Unit = Engine::discard
    internal var ioDispatcher: CoroutineDispatcher = Dispatchers.IO
    private var shuttingDown = false
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val jobs = mutableMapOf<String, Pair<Job, TransferControl>>()
    private val pendingResume = mutableSetOf<String>()
    private lateinit var store: TaskStore
    private lateinit var prefs: MobilePreferences
    private val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) { scope.launch { networkChanged() } }
        override fun onLost(network: Network) { scope.launch { networkChanged() } }
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) { scope.launch { networkChanged() } }
    }
    override fun onCreate() {
        super.onCreate(); running = true
        store = taskStoreProvider(this); prefs = MobilePreferences(this); MobileRuntime.initialize(store)
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.notification_channel), NotificationManager.IMPORTANCE_LOW))
        getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(networkCallback)
    }
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(SUMMARY_ID, notification(null), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        val id = intent?.getStringExtra(EXTRA_PROCESS)
        if (id != null) when (intent.action) {
            ACTION_PAUSE -> halt(id, TaskState.PAUSED)
            ACTION_CANCEL -> halt(id, TaskState.CANCELLED)
            ACTION_RESUME -> if (store.get(id)?.state in setOf(TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK)) {
                if (id in jobs) pendingResume.add(id)
                else if (MobileRuntime.busy(id)) MobileRuntime.requestResume(id)
                else store.state(id, TaskState.QUEUED)
            }
        }
        schedule()
        return START_NOT_STICKY
    }
    private fun allowed() = TaskPolicy.mayRun(prefs.networkPolicy, networkSnapshotProvider(this), prefs.allowRoaming)
    private fun halt(id: String, state: TaskState) {
        val task = store.get(id) ?: return
        if (task.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED)) return
        pendingResume.remove(id)
        MobileRuntime.clearResume(id); MobileRuntime.stop(id)
        store.state(id, state)
        cancelTransfer(id)
        if (!MobileRuntime.busy(id) && state == TaskState.CANCELLED) discardTransfer(this, id)
        getSystemService(NotificationManager::class.java).notify(task.notificationId, notification(store.get(id)))
    }
    internal fun networkChanged() {
        if (shuttingDown) return
        if (!allowed()) jobs.keys.toList().filter { store.get(it)?.state in TaskPolicy.inFlight }.forEach { halt(it, TaskState.WAITING_NETWORK) }
        schedule()
    }
    private fun schedule() {
        if (shuttingDown) return
        val candidates = store.list().filter { it.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) && !MobileRuntime.busy(it.id) }
        if (!allowed()) candidates.forEach { store.state(it.id, TaskState.WAITING_NETWORK) }
        else for (task in candidates.take((prefs.concurrency - jobs.size).coerceAtLeast(0))) {
            val control = TransferControl { allowed() }
            if (!MobileRuntime.claim(task.id, control)) continue
            if (!store.begin(task.id)) { MobileRuntime.release(task.id); continue }
            val job = scope.launch(start = CoroutineStart.LAZY) {
                try {
                    withContext(ioDispatcher) {
                        transferRunner(this@DownloadService, task, control, { state ->
                            control.check(); if (!store.transitionActive(task.id, state)) throw TransferFailure("interrupted")
                        }, { percent ->
                            control.check()
                            if (!store.updateActive(task.id, ContentValues().apply { put("progress", percent.toInt().coerceIn(0, 99)) })) throw TransferFailure("interrupted")
                            getSystemService(NotificationManager::class.java).notify(task.notificationId, notification(store.get(task.id)))
                        })
                        control.check()
                        store.updateActive(task.id, ContentValues().apply { put("state", TaskState.COMPLETED.name); put("progress", 100); put("error", "") })
                    }
                } catch (error: Exception) {
                    val current = store.get(task.id)
                    if (current != null && (current.state in TaskPolicy.inFlight || current.state == TaskState.QUEUED)) {
                        val code = errorCode(error)
                        store.transitionActive(task.id, if (code == "waiting_network") TaskState.WAITING_NETWORK else if (error is CancellationException) TaskState.PAUSED else TaskState.FAILED, code)
                    }
                } finally {
                    jobs.remove(task.id)
                    MobileRuntime.release(task.id)
                    if (pendingResume.remove(task.id) && !shuttingDown && store.get(task.id)?.state in setOf(TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK)) store.state(task.id, TaskState.QUEUED)
                    if (store.get(task.id)?.state == TaskState.CANCELLED) withContext(NonCancellable + ioDispatcher) { discardTransfer(this@DownloadService, task.id) }
                    getSystemService(NotificationManager::class.java).notify(task.notificationId, notification(store.get(task.id)))
                    schedule()
                }
            }
            jobs[task.id] = job to control; job.start()
        }
        val pending = store.list().any { it.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) }
        if (jobs.isEmpty()) {
            if (pending) NetworkJobs.schedule(this, store)
            stopForeground(STOP_FOREGROUND_REMOVE); stopSelf()
        }
        else getSystemService(NotificationManager::class.java).notify(SUMMARY_ID, notification(null))
    }
    private fun notification(task: MobileTask?): Notification {
        val home = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val builder = NotificationCompat.Builder(this, CHANNEL).setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(task?.title?.ifEmpty { getString(R.string.app_name) } ?: getString(R.string.app_name))
            .setContentText(task?.let { MobileText.state(this, it) } ?: getString(R.string.queue_running))
            .setContentIntent(home).setOnlyAlertOnce(true)
        val active = task == null || task.state in TaskPolicy.inFlight || task.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK)
        builder.setOngoing(active).setAutoCancel(!active)
        if (task != null) {
            if (task.state in TaskPolicy.inFlight) builder.setProgress(100, task.progress, task.progress == 0)
            val action = if (active) ACTION_PAUSE else ACTION_RESUME
            if (task.state !in setOf(TaskState.COMPLETED, TaskState.CANCELLED)) {
                val pending = PendingIntent.getService(this, task.notificationId,
                    Intent(this, DownloadService::class.java).setAction(action).putExtra(EXTRA_PROCESS, task.id),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
                builder.addAction(0, getString(if (active) R.string.pause else R.string.resume), pending)
            }
            if (task.state == TaskState.COMPLETED && task.uri.isNotEmpty()) {
                val open = MediaActions.openIntent(SavedMedia(task.uri, task.fileName, task.mime))
                val pending = PendingIntent.getActivity(this, task.notificationId, open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
                builder.setContentIntent(pending).addAction(0, getString(R.string.open_file), pending)
                val share = PendingIntent.getActivity(this, task.notificationId + 100000,
                    Intent.createChooser(MediaActions.shareIntent(listOf(SavedMedia(task.uri, task.fileName, task.mime))), getString(R.string.share_file)),
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
                builder.addAction(0, getString(R.string.share_file), share)
            }
        }
        return builder.build()
    }
    override fun onTimeout(startId: Int, fgsType: Int) {
        shuttingDown = true
        jobs.keys.toList().forEach { halt(it, TaskState.PAUSED); store.state(it, TaskState.PAUSED, "system_timeout") }
        store.list().filter { it.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) }.forEach { store.state(it.id, TaskState.PAUSED, "system_timeout") }
        stopForeground(STOP_FOREGROUND_REMOVE); stopSelf()
    }
    override fun onDestroy() {
        shuttingDown = true
        running = false
        runCatching { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(networkCallback) }
        jobs.keys.toList().filter { store.get(it)?.state in TaskPolicy.inFlight }.forEach { halt(it, TaskState.PAUSED) }
        jobs.values.forEach { it.second.stop() }
        scope.cancel(); super.onDestroy()
    }
    companion object {
        @Volatile var running = false; private set
        private const val CHANNEL = "downloads"
        private const val SUMMARY_ID = Int.MAX_VALUE
        const val ACTION_PAUSE = "app.ratatoskr.android.PAUSE"
        const val ACTION_CANCEL = "app.ratatoskr.android.CANCEL"
        const val ACTION_RESUME = "app.ratatoskr.android.RESUME"
        const val EXTRA_PROCESS = "process"
        fun start(context: Context, url: String, height: Int?, audio: Boolean, title: String, kind: String = "media", items: String = "") {
            MobileRuntime.initialize(TaskStore.get(context))
            TaskStore.get(context).enqueue(url, height, audio, title, kind, items)
            wake(context)
        }
        fun wake(context: Context) { context.startForegroundService(Intent(context, DownloadService::class.java)) }
        fun command(context: Context, id: String, action: String) {
            context.startForegroundService(Intent(context, DownloadService::class.java).setAction(action).putExtra(EXTRA_PROCESS, id))
        }
        internal fun errorCode(error: Exception): String {
            if (error is TransferFailure) return error.code
            val message = error.message.orEmpty().lowercase()
            return when {
                "no space" in message || "enospc" in message -> "no_space"
                "429" in message || "rate limit" in message -> "rate_limited"
                "login" in message || "private" in message || "403" in message || "401" in message -> "auth_required"
                "404" in message || "removed" in message -> "not_found"
                "unsupported" in message || "no video" in message -> "unsupported_media"
                "timed out" in message -> "network"
                else -> "download_failed"
            }
        }
    }
}
