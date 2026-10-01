package app.ratatoskr.android

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.widget.Toast
import java.io.File

/** Hands a verified APK to Android's own installer; Android still asks the user to confirm. */
object UpdateInstaller {
    const val ACTION = "app.ratatoskr.android.INSTALL_RESULT"

    fun canInstall(context: Context) = context.packageManager.canRequestPackageInstalls()

    /** Opens the system page where the user allows this app to install updates. */
    fun requestPermission(context: Context) {
        context.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    fun install(context: Context, apk: File) {
        val installer = context.packageManager.packageInstaller
        val id = installer.createSession(PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL))
        installer.openSession(id).use { session ->
            apk.inputStream().use { input -> session.openWrite("ratatoskr.apk", 0, apk.length()).use { output ->
                input.copyTo(output); session.fsync(output)
            } }
            val flags = PendingIntent.FLAG_UPDATE_CURRENT or if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0
            val result = PendingIntent.getBroadcast(context, id, Intent(context, UpdateReceiver::class.java).setAction(ACTION), flags)
            session.commit(result.intentSender)
        }
    }

    fun cleanup(context: Context) { File(context.cacheDir, "updates").deleteRecursively() }
}

/** Receives the installer's answer: shows Android's confirmation screen, or says why it failed. */
class UpdateReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                @Suppress("DEPRECATION")
                val confirm = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java) else intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)
                confirm?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)?.let { context.startActivity(it) }
            }
            PackageInstaller.STATUS_SUCCESS -> UpdateInstaller.cleanup(context)
            else -> Toast.makeText(context, R.string.update_install_failed, Toast.LENGTH_LONG).show()
        }
    }
}
