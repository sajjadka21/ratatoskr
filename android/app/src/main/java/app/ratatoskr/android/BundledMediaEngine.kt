package app.ratatoskr.android

import android.content.Context
import com.yausername.youtubedl_android.YoutubeDL
import java.io.File
import java.security.MessageDigest

/** A pinned official extractor ships with the APK, so the first download needs no engine update. */
object BundledMediaEngine {
    const val VERSION = "2026.08.19"
    const val SHA256 = "1fa6733c37ea6fb51c99ad8fe785e7b7e5f3246c9b980230329d4fb72ed8d4d6"
    @Synchronized fun install(context: Context) {
        val directory = File(File(context.noBackupFilesDir, YoutubeDL.baseName), YoutubeDL.ytdlpDirName)
        val binary = File(directory, YoutubeDL.ytdlpBin)
        val prefs = context.getSharedPreferences("bundled_media_engine", Context.MODE_PRIVATE)
        val updated = YoutubeDL.getInstance().version(context).orEmpty()
        if (binary.isFile && (prefs.getString("installed", "") == VERSION || updated == VERSION || AppUpdate.isNewer(updated, VERSION))) return
        directory.mkdirs()
        val temporary = File(directory, "bundled.tmp")
        try {
            val digest = MessageDigest.getInstance("SHA-256")
            context.resources.openRawResource(R.raw.ytdlp).use { input -> temporary.outputStream().use { output ->
                val buffer = ByteArray(64 * 1024)
                while (true) { val count = input.read(buffer); if (count < 0) break; digest.update(buffer, 0, count); output.write(buffer, 0, count) }
                output.fd.sync()
            } }
            check(digest.digest().joinToString("") { "%02x".format(it) } == SHA256) { "invalid_engine" }
            java.nio.file.Files.move(temporary.toPath(), binary.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING, java.nio.file.StandardCopyOption.ATOMIC_MOVE)
            prefs.edit().putString("installed", VERSION).commit()
        } finally { temporary.delete() }
    }
}
