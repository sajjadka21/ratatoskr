package app.ratatoskr.android

import java.io.InputStream
import java.security.MessageDigest

/** Checks a downloaded file against the hash a site publishes (MD5, SHA-1 or SHA-256). Pure, so it runs as a unit test. */
object Checksums {
    private val hex = Regex("[0-9a-fA-F]+")

    /** The first word of what was pasted, lower-case and without a `sha256:` style prefix. */
    fun normalize(text: String): String = text.trim().split(Regex("\\s+")).firstOrNull().orEmpty().substringAfter(':').lowercase()

    /** The algorithm that produces a hash of this length, or null when it is not a hash at all. */
    fun algorithm(hash: String): String? = if (!hex.matches(hash)) null else when (hash.length) {
        32 -> "MD5"; 40 -> "SHA-1"; 64 -> "SHA-256"; else -> null
    }

    fun compute(stream: InputStream, algorithm: String, check: () -> Unit = {}): String {
        val digest = MessageDigest.getInstance(algorithm)
        val buffer = ByteArray(64 * 1024)
        stream.use { input ->
            while (true) { check(); val count = input.read(buffer); if (count < 0) break; digest.update(buffer, 0, count) }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }
}
