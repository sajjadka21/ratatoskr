package app.ratatoskr.android

import android.content.Context
import android.content.Intent
import android.os.PowerManager
import android.provider.Settings

/** Many phones stop background apps to save battery; excluding the app keeps long downloads alive. */
object Power {
    fun excluded(context: Context) = context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)
    /** Opens Android's own list (no special permission needed); the user chooses. */
    fun openSettings(context: Context) {
        context.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}
