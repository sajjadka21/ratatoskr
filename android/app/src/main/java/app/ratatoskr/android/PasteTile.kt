package app.ratatoskr.android

import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.service.quicksettings.TileService

/** Quick Settings tile: copy a link, open the panel, tap — the quality dialog opens. */
class PasteTile : TileService() {
    override fun onClick() {
        val intent = Intent(this, ShareActivity::class.java)
            .putExtra(ShareActivity.EXTRA_FROM_CLIPBOARD, true)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= 34) {
            startActivityAndCollapse(
                PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE),
            )
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }
}
