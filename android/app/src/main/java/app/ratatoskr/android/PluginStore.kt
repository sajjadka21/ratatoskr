package app.ratatoskr.android

import android.content.Context
import java.io.File

/** Plugin files in the app's private folder; which ones are switched off lives in preferences. */
class PluginStore(private val folder: File, private val prefs: MobilePreferences?) {
    constructor(context: Context) : this(File(context.filesDir, "plugins"), MobilePreferences(context))

    private val off: MutableSet<String> get() = prefs?.disabledPlugins?.toMutableSet() ?: mutableSetOf()

    fun all(): List<Plugin> = folder.listFiles { file -> file.extension == "json" && file.length() <= Plugins.MAX_BYTES }.orEmpty()
        .mapNotNull { file -> runCatching { Plugins.parse(file.readText()) }.getOrNull() }.sortedBy { it.id }
    fun enabled(id: String) = id !in off
    fun active(): List<Plugin> = all().filter { enabled(it.id) }

    fun import(text: String): Plugin {
        val plugin = Plugins.parse(text)
        folder.mkdirs()
        val temp = File(folder, "${plugin.id}.json.tmp")
        temp.writeText(text)
        if (!temp.renameTo(File(folder, "${plugin.id}.json"))) { File(folder, "${plugin.id}.json").writeText(text); temp.delete() }
        return plugin
    }
    fun remove(id: String) {
        if (!Regex("[a-z0-9-]{1,40}").matches(id)) return
        File(folder, "$id.json").delete()
    }
    fun setEnabled(id: String, on: Boolean) {
        val set = off
        if (on) set.remove(id) else set.add(id)
        prefs?.disabledPlugins = set
    }

    companion object {
        /** Cheap enough to read on every link; at most a few small files. */
        fun active(context: Context): List<Plugin> = runCatching { PluginStore(context).active() }.getOrDefault(emptyList())
    }
}
