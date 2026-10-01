package app.ratatoskr.android

import org.json.JSONObject
import java.io.File
import java.security.MessageDigest

data class UpdateOffer(val version: String, val notes: String, val url: String, val sha256: String, val size: Long)

/** Finds a newer release on GitHub. Nothing is downloaded until the user agrees, and a download is
 * installed only if its SHA-256 matches the one GitHub publishes for that file (Android also refuses an
 * APK that is not signed with this app's own key). */
object AppUpdate {
    const val LATEST = "https://api.github.com/repos/sajjadka21/ratatoskr/releases/latest"
    private const val DOWNLOADS = "https://github.com/sajjadka21/ratatoskr/releases/download/"
    const val RELEASES_PAGE = "https://github.com/sajjadka21/ratatoskr/releases/latest"

    /** True when [candidate] is a higher dotted number than [current]; text after a `-` is ignored. */
    fun isNewer(candidate: String, current: String): Boolean {
        fun parts(value: String) = value.trim().removePrefix("v").substringBefore('-').substringBefore('+').split('.').map { it.toIntOrNull() ?: 0 }
        val a = parts(candidate); val b = parts(current)
        for (index in 0 until maxOf(a.size, b.size)) {
            val left = a.getOrElse(index) { 0 }; val right = b.getOrElse(index) { 0 }
            if (left != right) return left > right
        }
        return false
    }

    /** The APK for this phone's CPU from a "latest release" answer, or null when there is nothing newer. */
    fun parse(json: String, abis: List<String>, current: String): UpdateOffer? {
        val release = JSONObject(json)
        if (release.optBoolean("draft") || release.optBoolean("prerelease")) return null
        val version = release.optString("tag_name").removePrefix("v")
        if (version.isEmpty() || !isNewer(version, current)) return null
        val assets = release.optJSONArray("assets") ?: return null
        val rows = (0 until assets.length()).mapNotNull { assets.optJSONObject(it) }
        val wanted = abis.map { "Ratatoskr-android-$it.apk" } + "Ratatoskr-android.apk"
        val asset = wanted.firstNotNullOfOrNull { name -> rows.firstOrNull { it.optString("name") == name } } ?: return null
        val url = asset.optString("browser_download_url")
        if (!url.startsWith(DOWNLOADS)) return null
        val digest = asset.optString("digest").lowercase().removePrefix("sha256:").takeIf { Regex("[0-9a-f]{64}").matches(it) } ?: ""
        return UpdateOffer(version, release.optString("body").take(600), url, digest, asset.optLong("size", -1))
    }

    /** Streams the APK into [target] and checks its SHA-256; a file that does not match is deleted. */
    fun download(offer: UpdateOffer, target: File, control: TransferControl, progress: (Long, Long) -> Unit,
                 open: (String, Map<String, String>, () -> Unit) -> HttpConnection = SafeHttp::open) {
        if (offer.sha256.isEmpty()) throw TransferFailure("unverifiable")
        target.parentFile?.mkdirs()
        val connection = open(offer.url, emptyMap(), control::check)
        try {
            SafeHttp.requireSuccess(connection.responseCode)
            val digest = MessageDigest.getInstance("SHA-256")
            val total = connection.contentLengthLong.takeIf { it > 0 } ?: offer.size
            var done = 0L
            var last = 0L
            target.outputStream().use { output -> connection.inputStream.use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    control.check()
                    val count = input.read(buffer)
                    if (count < 0) break
                    output.write(buffer, 0, count); digest.update(buffer, 0, count); done += count
                    val now = System.nanoTime()
                    if (now - last > 200_000_000L) { progress(done, total); last = now }
                }
            } }
            progress(done, total)
            val actual = digest.digest().joinToString("") { "%02x".format(it) }
            if (actual != offer.sha256) { target.delete(); throw TransferFailure("checksum_mismatch") }
        } catch (error: Exception) {
            if (error !is TransferFailure || error.code != "checksum_mismatch") target.delete()
            throw error
        } finally { connection.disconnect() }
    }
}
