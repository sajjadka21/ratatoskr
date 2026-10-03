package app.ratatoskr.android

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities

class MobilePreferences(context: Context) {
    private val prefs = context.getSharedPreferences("download_preferences", Context.MODE_PRIVATE)
    var namedQueues: Set<String>
        get() = prefs.getStringSet("named_queues", emptySet())?.toSet().orEmpty()
        set(value) { prefs.edit().putStringSet("named_queues", value.toSet()).apply() }
    var calendarType: String
        get() = prefs.getString("calendar", if (language == "fa") "persian" else "gregorian")!!
        set(value) { prefs.edit().putString("calendar", value).apply() }
    var networkPolicy: NetworkPolicy
        get() = runCatching { NetworkPolicy.valueOf(prefs.getString("network", NetworkPolicy.ANY.name)!!) }.getOrDefault(NetworkPolicy.ANY)
        set(value) { prefs.edit().putString("network", value.name).apply() }
    var allowRoaming: Boolean
        get() = prefs.getBoolean("roaming", false)
        set(value) { prefs.edit().putBoolean("roaming", value).apply() }
    var quickDownload: Boolean
        get() = prefs.getBoolean("quick", false)
        set(value) { prefs.edit().putBoolean("quick", value).apply() }
    var failureVibration: Boolean
        get() = prefs.getBoolean("failure_vibration", true)
        set(value) { prefs.edit().putBoolean("failure_vibration", value).apply() }
    var defaultHeight: Int?
        get() = prefs.getInt("height", 0).takeIf { it > 0 }
        set(value) { prefs.edit().putInt("height", value ?: 0).apply() }
    var defaultAudio: Boolean
        get() = prefs.getBoolean("audio", false)
        set(value) { prefs.edit().putBoolean("audio", value).apply() }
    var concurrency: Int
        get() = prefs.getInt("concurrency", 1).coerceIn(1, 3)
        set(value) { prefs.edit().putInt("concurrency", value.coerceIn(1, 3)).apply() }
    /** Connections used for one file; 1 keeps the single-stream path. */
    var connections: Int
        get() = prefs.getInt("connections", 4).coerceIn(1, 8)
        set(value) { prefs.edit().putInt("connections", value.coerceIn(1, 8)).apply() }
    /** Sort finished files into Video, Music, Archives... under Downloads/Ratatoskr. */
    var categoryFolders: Boolean
        get() = prefs.getBoolean("category_folders", true)
        set(value) { prefs.edit().putBoolean("category_folders", value).apply() }
    var watchClipboard: Boolean
        get() = prefs.getBoolean("watch_clipboard", true)
        set(value) { prefs.edit().putBoolean("watch_clipboard", value).apply() }
    var disabledPlugins: Set<String>
        get() = prefs.getStringSet("plugins_off", emptySet()) ?: emptySet()
        set(value) { prefs.edit().putStringSet("plugins_off", value.toSet()).apply() }
    var autoUpdateCheck: Boolean
        get() = prefs.getBoolean("auto_update_check", true)
        set(value) { prefs.edit().putBoolean("auto_update_check", value).apply() }
    var lastUpdateCheck: Long
        get() = prefs.getLong("last_update_check", 0)
        set(value) { prefs.edit().putLong("last_update_check", value).apply() }
    var skippedUpdate: String
        get() = prefs.getString("skipped_update", "")!!
        set(value) { prefs.edit().putString("skipped_update", value).apply() }
    /** "Only download between…": a daily window that may pass midnight. */
    var window: DownloadWindow
        get() = DownloadWindow(prefs.getBoolean("window_on", false), prefs.getInt("window_start", 2 * 60).coerceIn(0, 1439), prefs.getInt("window_end", 7 * 60).coerceIn(0, 1439))
        set(value) { prefs.edit().putBoolean("window_on", value.enabled).putInt("window_start", value.startMinute).putInt("window_end", value.endMinute).apply() }
    /** A folder the user picked (a persistable tree URI); empty means Downloads/Ratatoskr. */
    var saveTree: String
        get() = prefs.getString("save_tree", "")!!
        set(value) { prefs.edit().putString("save_tree", value).apply() }
    var onboarded: Boolean
        get() = prefs.getBoolean("onboarded", false)
        set(value) { prefs.edit().putBoolean("onboarded", value).apply() }
    var speedLimit: Long
        get() = prefs.getLong("speed", 0).coerceAtLeast(0)
        set(value) { prefs.edit().putLong("speed", value.coerceAtLeast(0)).apply() }
    var mode: String
        get() = prefs.getString("mode", "system")!!
        set(value) { prefs.edit().putString("mode", value).apply() }
    var brand: String
        get() = prefs.getString("brand", "ember-forge")!!
        set(value) { prefs.edit().putString("brand", value).apply() }
    var language: String
        get() = prefs.getString("language", "")!!
        set(value) { prefs.edit().putString("language", value).apply() }
}

object MobileNetwork {
    fun snapshot(context: Context): NetworkSnapshot {
        val manager = context.getSystemService(ConnectivityManager::class.java)
        val caps = manager.getNetworkCapabilities(manager.activeNetwork) ?: return NetworkSnapshot(false, false, true)
        return NetworkSnapshot(caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) &&
            caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED),
            caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI), manager.isActiveNetworkMetered,
            // The roaming capability only exists from Android 9; before that, assume not roaming.
            android.os.Build.VERSION.SDK_INT >= 28 && !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_ROAMING))
    }
}
