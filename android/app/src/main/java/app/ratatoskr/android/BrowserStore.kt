package app.ratatoskr.android

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

/** Local-only browser history and bookmarks. No page credentials or cookies are stored here. */
data class BrowserEntry(val title: String, val url: String, val time: Long)

object BrowserStore {
    private const val PREFS = "ratatoskr_browser"
    private const val HISTORY = "history"
    private const val BOOKMARKS = "bookmarks"
    private const val MAX_HISTORY = 100

    fun history(context: Context): List<BrowserEntry> = read(context, HISTORY)
    fun bookmarks(context: Context): List<BrowserEntry> = read(context, BOOKMARKS)
    fun isBookmarked(context: Context, url: String): Boolean = bookmarks(context).any { it.url == url }

    fun recordHistory(context: Context, url: String, title: String) {
        if (!LinkUtils.isPublicHttpUrl(url)) return
        val items = history(context).filterNot { it.url == url }.toMutableList()
        items.add(0, BrowserEntry(title.take(120), url, System.currentTimeMillis()))
        write(context, HISTORY, items.take(MAX_HISTORY))
    }

    /** Returns true when the bookmark is present after this operation. */
    fun toggleBookmark(context: Context, url: String, title: String): Boolean {
        if (!LinkUtils.isPublicHttpUrl(url)) return false
        val items = bookmarks(context).toMutableList()
        val existing = items.indexOfFirst { it.url == url }
        if (existing >= 0) items.removeAt(existing)
        else items.add(0, BrowserEntry(title.take(120), url, System.currentTimeMillis()))
        write(context, BOOKMARKS, items)
        return existing < 0
    }

    fun clearHistory(context: Context) = write(context, HISTORY, emptyList())
    fun clearBookmarks(context: Context) = write(context, BOOKMARKS, emptyList())

    private fun read(context: Context, key: String): List<BrowserEntry> {
        val raw = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(key, "[]") ?: "[]"
        return runCatching {
            val array = JSONArray(raw)
            (0 until array.length()).mapNotNull { index ->
                val item = array.optJSONObject(index) ?: return@mapNotNull null
                val url = item.optString("url")
                if (!LinkUtils.isPublicHttpUrl(url)) return@mapNotNull null
                BrowserEntry(item.optString("title"), url, item.optLong("time"))
            }
        }.getOrDefault(emptyList())
    }

    private fun write(context: Context, key: String, entries: List<BrowserEntry>) {
        val array = JSONArray()
        entries.forEach { item ->
            array.put(JSONObject().put("title", item.title).put("url", item.url).put("time", item.time))
        }
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(key, array.toString()).apply()
    }
}

