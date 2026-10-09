package app.ratatoskr.android

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat

/** One replaceable alert for download failures; dismissing it resets the unread count. */
object FailureNotifications {
    const val ID = Int.MAX_VALUE - 1
    private const val PREFS = "download_failure_notice"
    private const val VIBRATE_CHANNEL = "download_failures_vibrate"
    private const val QUIET_CHANNEL = "download_failures_quiet"
    private val lock = Any()

    fun ensureChannels(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(VIBRATE_CHANNEL, context.getString(R.string.failure_channel), NotificationManager.IMPORTANCE_DEFAULT).apply {
            enableVibration(true)
            vibrationPattern = longArrayOf(0, 180, 90, 180)
        })
        manager.createNotificationChannel(NotificationChannel(QUIET_CHANNEL, context.getString(R.string.failure_channel), NotificationManager.IMPORTANCE_DEFAULT).apply {
            enableVibration(false)
        })
    }

    fun publish(context: Context, task: MobileTask) {
        val values = synchronized(lock) {
            val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            val count = (prefs.getInt("count", 0) + 1).coerceAtMost(999)
            prefs.edit().putInt("count", count)
                .putString("title", task.title.ifBlank { task.fileName }.take(120))
                .putString("error", task.error).apply()
            Triple(count, prefs.getString("title", "").orEmpty(), prefs.getString("error", "").orEmpty())
        }
        ensureChannels(context)
        val home = PendingIntent.getActivity(context, 0, Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val dismiss = PendingIntent.getBroadcast(context, ID, Intent(context, DismissFailureNotificationReceiver::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val error = MobileText.error(context, values.third)
        val latest = listOf(values.second.ifBlank { context.getString(R.string.app_name) }, error).joinToString(" · ")
        val vibrationEnabled = MobilePreferences(context).failureVibration
        val builder = NotificationCompat.Builder(context, if (vibrationEnabled) VIBRATE_CHANNEL else QUIET_CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_download)
            .setContentTitle(context.getString(R.string.failure_notice_title))
            .setContentText(latest)
            .setStyle(NotificationCompat.BigTextStyle().bigText(latest))
            .setNumber(values.first)
            .setSubText(context.getString(R.string.failure_notice_count, values.first))
            .setContentIntent(home)
            .setDeleteIntent(dismiss)
            .setAutoCancel(true)
            .setOnlyAlertOnce(true)
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O && vibrationEnabled) builder.setDefaults(NotificationCompat.DEFAULT_VIBRATE)
        val notification = builder.build()
        context.getSystemService(NotificationManager::class.java).notify(ID, notification)
    }

    fun dismissed(context: Context) {
        synchronized(lock) {
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().clear().apply()
            context.getSystemService(NotificationManager::class.java).cancel(ID)
        }
    }
}

class DismissFailureNotificationReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent?) = FailureNotifications.dismissed(context)
}
