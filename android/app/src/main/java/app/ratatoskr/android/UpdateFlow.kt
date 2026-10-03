package app.ratatoskr.android

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.widget.ProgressBar
import android.widget.Toast
import androidx.appcompat.app.AlertDialog
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/** App updates: look for a newer release, ask first, download and verify, then let Android install it. */
object UpdateFlow {
    /** Looks once a day (or right now when [manual]); nothing is downloaded until the user says yes. */
    fun check(activity: MobileActivity, manual: Boolean) {
        if (activity.packageName.endsWith(".localreview")) return
        val prefs = MobilePreferences(activity)
        if (!manual && (!prefs.autoUpdateCheck || System.currentTimeMillis() - prefs.lastUpdateCheck < 24L * 3600 * 1000)) return
        prefs.lastUpdateCheck = System.currentTimeMillis()
        val current = activity.packageManager.getPackageInfo(activity.packageName, 0).versionName.orEmpty()
        activity.lifecycleScope.launch {
            val result = runCatching { withContext(Dispatchers.IO) {
                val connection = SafeHttp.open(AppUpdate.LATEST, mapOf("Accept" to "application/vnd.github+json"))
                try {
                    SafeHttp.requireSuccess(connection.responseCode)
                    AppUpdate.parse(LinkUtils.readText(connection.inputStream, 512 * 1024), Build.SUPPORTED_ABIS.toList(), current)
                } finally { connection.disconnect() }
            } }
            val offer = result.getOrNull()
            when {
                result.isFailure -> if (manual) Toast.makeText(activity, R.string.update_check_failed, Toast.LENGTH_LONG).show()
                offer == null -> if (manual) Toast.makeText(activity, R.string.update_up_to_date, Toast.LENGTH_LONG).show()
                !manual && offer.version == prefs.skippedUpdate -> Unit
                else -> offer(activity, offer, prefs)
            }
        }
    }

    private fun offer(activity: MobileActivity, offer: UpdateOffer, prefs: MobilePreferences) {
        val size = if (offer.size > 0) Format.bytes(offer.size) else "?"
        AlertDialog.Builder(activity).setTitle(activity.getString(R.string.update_available, offer.version))
            .setMessage((if (offer.notes.isNotBlank()) offer.notes + "\n\n" else "") + activity.getString(R.string.update_consent))
            .setPositiveButton(activity.getString(R.string.update_now, size)) { _, _ -> start(activity, offer) }
            .setNeutralButton(R.string.update_skip) { _, _ -> prefs.skippedUpdate = offer.version }
            .setNegativeButton(R.string.update_later, null).show()
    }

    private fun start(activity: MobileActivity, offer: UpdateOffer) {
        if (offer.sha256.isEmpty()) {
            Toast.makeText(activity, R.string.update_unverifiable, Toast.LENGTH_LONG).show()
            activity.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(AppUpdate.RELEASES_PAGE))); return
        }
        if (!UpdateInstaller.canInstall(activity)) {
            Toast.makeText(activity, R.string.update_allow_install, Toast.LENGTH_LONG).show()
            UpdateInstaller.requestPermission(activity); return
        }
        val control = TransferControl { true }
        val bar = ProgressBar(activity, null, android.R.attr.progressBarStyleHorizontal).apply { max = 100 }
        val box = activity.column().apply { setPadding(activity.dp(20), activity.dp(12), activity.dp(20), activity.dp(12)); addView(activity.label(activity.getString(R.string.update_downloading), 14f)); addView(bar) }
        val dialog = AlertDialog.Builder(activity).setView(box).setCancelable(false).setNegativeButton(R.string.cancel) { _, _ -> control.stop() }.show()
        val target = File(activity.cacheDir, "updates/ratatoskr-${offer.version}.apk")
        activity.lifecycleScope.launch {
            val failure = runCatching { withContext(Dispatchers.IO) {
                AppUpdate.download(offer, target, control, { done, total -> if (total > 0) activity.runOnUiThread { bar.progress = (done * 100 / total).toInt() } })
                UpdateInstaller.install(activity, target)
            } }.exceptionOrNull()
            dialog.dismiss()
            if (failure != null && !(failure is TransferFailure && failure.code == "interrupted"))
                Toast.makeText(activity, if (failure is TransferFailure && failure.code == "checksum_mismatch") R.string.update_bad_file else R.string.update_install_failed, Toast.LENGTH_LONG).show()
        }
    }
}
