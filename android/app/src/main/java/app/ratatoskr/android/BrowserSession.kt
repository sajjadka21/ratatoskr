package app.ratatoskr.android

import com.yausername.youtubedl_android.YoutubeDLRequest
import java.net.URI
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

/** A per-download copy of a WebView session for one HTTPS origin, kept only in memory. */
class BrowserSession private constructor(
    private val scheme: String,
    private val host: String,
    private val port: Int,
    private val cookie: String,
) {
    /** yt-dlp scopes Cookie headers to the input URL domain (including redirects) before requests. */
    fun addTo(request: YoutubeDLRequest, url: String): Boolean {
        val secureCookie = secureCookieHeaderFor(url) ?: return false
        // WebView returns request-cookie pairs, without their Secure metadata.
        // Add it back so yt-dlp's cookie jar never sends these on plain HTTP.
        request.addOption("--add-headers", "Cookie: $secureCookie")
        return true
    }

    internal fun secureCookieHeaderFor(url: String): String? = cookie.takeIf { appliesTo(url) }
        ?.split(';')?.joinToString("; ") { "${it.trim()}; Secure" }

    private fun appliesTo(value: String): Boolean {
        val uri = runCatching { URI(value) }.getOrNull() ?: return false
        val candidateHost = uri.host ?: return false
        val candidatePort = uri.port.takeIf { it >= 0 } ?: if (uri.scheme.equals("https", true)) 443 else -1
        return uri.scheme.equals(scheme, ignoreCase = true) &&
            candidateHost.equals(host, ignoreCase = true) && candidatePort == port
    }

    override fun toString() = "BrowserSession(origin=https://$host:$port, cookie=<redacted>)"

    companion object {
        private const val MAX_COOKIE_BYTES = 16 * 1024
        fun create(sourceUrl: String, cookieHeader: String?): BrowserSession? {
            val uri = runCatching { URI(sourceUrl) }.getOrNull() ?: return null
            val host = uri.host?.takeIf { it.isNotBlank() } ?: return null
            if (!uri.scheme.equals("https", ignoreCase = true)) return null
            val cookie = cookieHeader?.trim()?.takeIf { value ->
                value.isNotEmpty() && value.toByteArray(Charsets.UTF_8).size <= MAX_COOKIE_BYTES && value.none { it.isISOControl() }
            } ?: return null
            return BrowserSession("https", host.lowercase(), uri.port.takeIf { it >= 0 } ?: 443, cookie)
        }
    }
}

/** One-time handoff between Ratatoskr's in-app browser and its media picker. */
object BrowserSessionHandoff {
    private const val LIFETIME_MS = 60_000L
    private data class Entry(val session: BrowserSession, val expiresAt: Long)
    private val pending = ConcurrentHashMap<String, Entry>()

    fun stage(session: BrowserSession?): String? {
        session ?: return null
        val now = System.currentTimeMillis()
        pending.entries.removeIf { it.value.expiresAt <= now }
        val id = UUID.randomUUID().toString()
        pending[id] = Entry(session, now + LIFETIME_MS)
        return id
    }

    fun take(id: String?): BrowserSession? {
        val entry = id?.let(pending::remove) ?: return null
        return entry.session.takeIf { entry.expiresAt > System.currentTimeMillis() }
    }
}

/** In-memory sessions tied to a single queued task; never added to TaskStore or a file. */
object BrowserSessionVault {
    private const val LIFETIME_MS = 10 * 60 * 1000L
    private data class Entry(val session: BrowserSession, val expiresAt: Long)
    private val tasks = ConcurrentHashMap<String, Entry>()

    fun attach(taskId: String, session: BrowserSession) {
        tasks[taskId] = Entry(session, System.currentTimeMillis() + LIFETIME_MS)
    }

    fun forTask(taskId: String): BrowserSession? {
        val entry = tasks[taskId] ?: return null
        if (entry.expiresAt <= System.currentTimeMillis()) {
            tasks.remove(taskId, entry)
            return null
        }
        return entry.session
    }

    fun forget(taskId: String) { tasks.remove(taskId) }
    fun clear() { tasks.clear() }
}
