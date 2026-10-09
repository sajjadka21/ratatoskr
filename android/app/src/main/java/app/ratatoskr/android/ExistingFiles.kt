package app.ratatoskr.android

import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.SystemClock
import android.provider.DocumentsContract
import android.provider.MediaStore
import java.io.File

data class FolderFileInspection(val matches: List<String> = emptyList(), val limited: Boolean = false)
/** Bounded metadata inspection; no contents, mutations, or download decisions. */
object ExistingFiles {
    fun inspect(context: Context, urls: List<String>): FolderFileInspection {
        val names = urls.take(200).mapNotNull { runCatching { Uri.parse(it).lastPathSegment?.takeIf { name -> name.contains('.') } }.getOrNull() }.distinct()
        if (names.isEmpty()) return FolderFileInspection()
        val found = linkedSetOf<String>(); var count = 0; var limited = false; var comparisons = 0
        val started = SystemClock.elapsedRealtime()
        fun withinBudget() = count < 3000 && found.size < 64 && comparisons < 50000 && SystemClock.elapsedRealtime() - started < 2000
        fun see(name: String) {
            count++
            for (candidate in names) {
                if (!withinBudget()) { limited = true; break }
                comparisons++
                if (candidate.equals(name, true) || FileNameSimilarity.similar(candidate, name)) { found.add(name); break }
            }
        }
        try {
            val treeString = MobilePreferences(context).saveTree
            if (treeString.isNotEmpty()) {
                val tree = Uri.parse(treeString)
                val todo = java.util.ArrayDeque<Pair<String, Int>>()
                val seen = hashSetOf<String>(); todo.add(DocumentsContract.getTreeDocumentId(tree) to 0)
                while (todo.isNotEmpty() && withinBudget()) {
                    val (id, depth) = todo.removeFirst()
                    if (!seen.add(id)) continue
                    val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id)
                    context.contentResolver.query(children, arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE), null, null, null)?.use { cursor ->
                        while (withinBudget() && cursor.moveToNext()) {
                            if (cursor.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR) {
                                count++; if (depth < 2) todo.add(cursor.getString(0) to depth + 1) else limited = true
                            } else see(cursor.getString(1).orEmpty())
                        }
                    } ?: run { limited = true }
                }
                if (todo.isNotEmpty()) limited = true
            } else if (Build.VERSION.SDK_INT >= 29) {
                limited = true // Scoped storage may hide files belonging to other apps.
                for ((collection, path) in listOf(MediaStore.Downloads.EXTERNAL_CONTENT_URI to "${Environment.DIRECTORY_DOWNLOADS}/%", MediaStore.Images.Media.EXTERNAL_CONTENT_URI to "${Environment.DIRECTORY_PICTURES}/Ratatoskr/%")) {
                    if (!withinBudget()) break
                    context.contentResolver.query(collection, arrayOf(MediaStore.MediaColumns.DISPLAY_NAME),
                        "${MediaStore.MediaColumns.RELATIVE_PATH} LIKE ? AND ${MediaStore.MediaColumns.IS_PENDING}=0", arrayOf(path), null)?.use { cursor ->
                        while (withinBudget() && cursor.moveToNext()) see(cursor.getString(0).orEmpty())
                    }
                }
            } else {
                val todo = java.util.ArrayDeque<Pair<File, Int>>(); todo.add(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS) to 0)
                todo.add(File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_PICTURES), "Ratatoskr") to 0)
                while (todo.isNotEmpty() && withinBudget()) {
                    val (folder, depth) = todo.removeFirst()
                    if (!folder.exists()) continue
                    val children = folder.listFiles()
                    if (children == null) { limited = true; continue }
                    for (file in children) {
                        if (!withinBudget()) { limited = true; break }
                        if (file.canonicalFile != file.absoluteFile) { limited = true; continue }
                        if (file.isDirectory) { count++; if (depth < 2) todo.add(file to depth + 1) else limited = true }
                        else if (file.isFile) see(file.name)
                    }
                }
                if (todo.isNotEmpty()) limited = true
            }
        } catch (_: Exception) { limited = true }
        return FolderFileInspection(found.toList(), limited || !withinBudget())
    }
}
