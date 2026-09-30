package ir.ratatosk.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import androidx.core.app.NotificationCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import java.util.concurrent.ConcurrentHashMap

/**
 * Runs downloads in the foreground so they go on when the share dialog
 * closes and the user returns to the app they came from.
 */
class DownloadService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val jobs = ConcurrentHashMap<String, Job>()
    private var nextId = 1000

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        createChannel()
        when (intent?.action) {
            ACTION_STOP -> {
                intent.getStringExtra(EXTRA_PROCESS)?.let { Engine.cancel(it); jobs.remove(it)?.cancel() }
                stopIfIdle()
            }
            else -> intent?.let { start(it) }
        }
        return START_NOT_STICKY
    }

    private fun start(intent: Intent) {
        val url = intent.getStringExtra(EXTRA_URL) ?: return
        val height = intent.getIntExtra(EXTRA_HEIGHT, 0).takeIf { it > 0 }
        val audio = intent.getBooleanExtra(EXTRA_AUDIO, false)
        val title = intent.getStringExtra(EXTRA_TITLE).orEmpty()
        val process = "dl-${nextId}"
        val notificationId = nextId++

        startForeground(
            notificationId,
            progressNotification(process, title, null),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
        )
        jobs[process] = scope.launch {
            val manager = getSystemService(NotificationManager::class.java)
            try {
                val name = Engine.download(this@DownloadService, url, height, audio, process) { percent ->
                    manager.notify(notificationId, progressNotification(process, title, percent))
                }
                manager.notify(notificationId, finishedNotification(getString(R.string.done, name)))
            } catch (e: Exception) {
                manager.notify(notificationId, finishedNotification(getString(R.string.failed)))
            } finally {
                jobs.remove(process)
                stopIfIdle()
            }
        }
    }

    private fun stopIfIdle() {
        if (jobs.isEmpty()) stopForeground(STOP_FOREGROUND_DETACH).also { stopSelf() }
    }

    private fun createChannel() {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.notification_channel), NotificationManager.IMPORTANCE_LOW),
        )
    }

    private fun progressNotification(process: String, title: String, percent: Float?): Notification {
        val stop = PendingIntent.getService(
            this, process.hashCode(),
            Intent(this, DownloadService::class.java).setAction(ACTION_STOP).putExtra(EXTRA_PROCESS, process),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val builder = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(title.ifEmpty { getString(R.string.downloading) })
            .setOnlyAlertOnce(true)
            .setOngoing(true)
            .addAction(0, getString(R.string.stop), stop)
        if (percent == null || percent < 0) builder.setProgress(0, 0, true)
        else builder.setProgress(100, percent.toInt().coerceIn(0, 100), false)
        return builder.build()
    }

    private fun finishedNotification(message: String): Notification =
        NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_download_done)
            .setContentTitle(message)
            .setContentText(getString(R.string.saved_in))
            .setAutoCancel(true)
            .build()

    override fun onDestroy() {
        scope.coroutineContext[Job]?.cancel()
        super.onDestroy()
    }

    companion object {
        private const val CHANNEL = "downloads"
        const val ACTION_STOP = "ir.ratatosk.app.STOP"
        const val EXTRA_URL = "url"
        const val EXTRA_HEIGHT = "height"
        const val EXTRA_AUDIO = "audio"
        const val EXTRA_TITLE = "title"
        const val EXTRA_PROCESS = "process"

        fun start(context: Context, url: String, height: Int?, audio: Boolean, title: String) {
            context.startForegroundService(
                Intent(context, DownloadService::class.java)
                    .putExtra(EXTRA_URL, url)
                    .putExtra(EXTRA_HEIGHT, height ?: 0)
                    .putExtra(EXTRA_AUDIO, audio)
                    .putExtra(EXTRA_TITLE, title),
            )
        }
    }
}
