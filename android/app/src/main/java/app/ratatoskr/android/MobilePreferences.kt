package app.ratatoskr.android

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities

class MobilePreferences(context: Context) {
    private val prefs = context.getSharedPreferences("download_preferences", Context.MODE_PRIVATE)
    var networkPolicy: NetworkPolicy
        get() = runCatching { NetworkPolicy.valueOf(prefs.getString("network", NetworkPolicy.ANY.name)!!) }.getOrDefault(NetworkPolicy.ANY)
        set(value) { prefs.edit().putString("network", value.name).apply() }
    var allowRoaming: Boolean
        get() = prefs.getBoolean("roaming", false)
        set(value) { prefs.edit().putBoolean("roaming", value).apply() }
    var quickDownload: Boolean
        get() = prefs.getBoolean("quick", false)
        set(value) { prefs.edit().putBoolean("quick", value).apply() }
    var defaultHeight: Int?
        get() = prefs.getInt("height", 0).takeIf { it > 0 }
        set(value) { prefs.edit().putInt("height", value ?: 0).apply() }
    var defaultAudio: Boolean
        get() = prefs.getBoolean("audio", false)
        set(value) { prefs.edit().putBoolean("audio", value).apply() }
    var concurrency: Int
        get() = prefs.getInt("concurrency", 1).coerceIn(1, 3)
        set(value) { prefs.edit().putInt("concurrency", value.coerceIn(1, 3)).apply() }
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
            !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_ROAMING))
    }
}
