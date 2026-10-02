package app.ratatoskr.android

import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteOpenHelper
import java.util.UUID

data class MobileTask(
    val id: String, val url: String, val title: String, val state: TaskState,
    val height: Int? = null, val audioOnly: Boolean = false, val kind: String = "media",
    val progress: Int = 0, val notificationId: Int = 0, val error: String = "",
    val uri: String = "", val fileName: String = "", val mime: String = "",
    val selectedItems: String = "", val bytesDone: Long = 0, val totalBytes: Long = -1,
    val validator: String = "", val createdAt: Long = System.currentTimeMillis(),
    /** Epoch millis before which the task must not start; 0 = as soon as allowed. */
    val startAt: Long = 0,
)

/** The mobile backend's job journal. A job exists before any network request. */
class TaskStore internal constructor(context: Context, databaseName: String = "downloads.db") : SQLiteOpenHelper(context, databaseName, null, 3) {
    override fun onCreate(db: SQLiteDatabase) {
        db.execSQL("""CREATE TABLE tasks (
            notification_id INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE,
            url TEXT NOT NULL, title TEXT NOT NULL, state TEXT NOT NULL, height INTEGER,
            audio INTEGER NOT NULL, kind TEXT NOT NULL, progress INTEGER NOT NULL DEFAULT 0,
            error TEXT NOT NULL DEFAULT '', uri TEXT NOT NULL DEFAULT '', file_name TEXT NOT NULL DEFAULT '',
            mime TEXT NOT NULL DEFAULT '', selected_items TEXT NOT NULL DEFAULT '',
            bytes_done INTEGER NOT NULL DEFAULT 0, total_bytes INTEGER NOT NULL DEFAULT -1,
            validator TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
            start_at INTEGER NOT NULL DEFAULT 0
        )""")
        db.execSQL("CREATE INDEX task_state ON tasks(state, created_at)")
        createOutputs(db)
    }
    private fun createOutputs(db: SQLiteDatabase) = db.execSQL("CREATE TABLE IF NOT EXISTS outputs (task_id TEXT NOT NULL, position INTEGER NOT NULL, uri TEXT NOT NULL, name TEXT NOT NULL, mime TEXT NOT NULL, PRIMARY KEY(task_id, position))")
    override fun onUpgrade(db: SQLiteDatabase, oldVersion: Int, newVersion: Int) {
        if (oldVersion < 2) createOutputs(db)
        if (oldVersion < 3) db.execSQL("ALTER TABLE tasks ADD COLUMN start_at INTEGER NOT NULL DEFAULT 0")
    }

    @Synchronized fun recordOutput(id: String, index: Int, result: SavedMedia) {
        writableDatabase.insertWithOnConflict("outputs", null, ContentValues().apply {
            put("task_id", id); put("position", index); put("uri", result.uri); put("name", result.name); put("mime", result.mime)
        }, SQLiteDatabase.CONFLICT_REPLACE)
        update(id, ContentValues().apply { put("uri", result.uri); put("file_name", result.name); put("mime", result.mime) })
    }
    @Synchronized fun outputs(id: String): List<SavedMedia> = readableDatabase.query("outputs", null, "task_id=?", arrayOf(id), null, null, "position ASC").use {
        buildList { while (it.moveToNext()) add(SavedMedia(it.getString(it.getColumnIndexOrThrow("uri")), it.getString(it.getColumnIndexOrThrow("name")), it.getString(it.getColumnIndexOrThrow("mime")))) }
    }
    @Synchronized fun outputAt(id: String, index: Int): SavedMedia? = readableDatabase.query("outputs", null, "task_id=? AND position=?", arrayOf(id, index.toString()), null, null, null).use {
        if (it.moveToFirst()) SavedMedia(it.getString(it.getColumnIndexOrThrow("uri")), it.getString(it.getColumnIndexOrThrow("name")), it.getString(it.getColumnIndexOrThrow("mime"))) else null
    }

    @Synchronized fun enqueue(url: String, height: Int?, audio: Boolean, title: String, kind: String = "media", items: String = ""): MobileTask {
        val canonical = LinkUtils.canonicalUrl(url)
        val existing = list().firstOrNull { LinkUtils.contentIdentity(it.url) == LinkUtils.contentIdentity(canonical) && it.height == height && it.audioOnly == audio &&
            it.kind == kind && it.selectedItems == items && it.state !in setOf(TaskState.COMPLETED, TaskState.CANCELLED, TaskState.FAILED) }
        if (existing != null) return existing
        val now = System.currentTimeMillis()
        val id = UUID.randomUUID().toString()
        val values = ContentValues().apply {
            put("id", id); put("url", canonical); put("title", title.take(300)); put("state", TaskState.QUEUED.name)
            if (height != null) put("height", height)
            put("audio", if (audio) 1 else 0); put("kind", kind); put("selected_items", items)
            put("created_at", now); put("updated_at", now)
        }
        writableDatabase.insertOrThrow("tasks", null, values)
        return get(id)!!
    }

    @Synchronized fun get(id: String): MobileTask? = readableDatabase.query("tasks", null, "id=?", arrayOf(id), null, null, null).use {
        if (it.moveToFirst()) read(it) else null
    }
    @Synchronized fun list(): List<MobileTask> = readableDatabase.query("tasks", null, null, null, null, null, "created_at ASC").use {
        buildList { while (it.moveToNext()) add(read(it)) }
    }
    @Synchronized fun update(id: String, values: ContentValues) {
        values.put("updated_at", System.currentTimeMillis())
        writableDatabase.update("tasks", values, "id=?", arrayOf(id))
    }
    @Synchronized fun begin(id: String): Boolean {
        val current = get(id) ?: return false
        if (current.state !in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK)) return false
        state(id, TaskState.PROBING)
        return true
    }
    @Synchronized fun updateActive(id: String, values: ContentValues): Boolean {
        val current = get(id) ?: return false
        if (current.state !in TaskPolicy.inFlight && current.state != TaskState.QUEUED) return false
        update(id, values)
        return true
    }
    fun transitionActive(id: String, state: TaskState, error: String = ""): Boolean = updateActive(id, ContentValues().apply { put("state", state.name); put("error", error) })
    fun state(id: String, state: TaskState, error: String = "") = update(id, ContentValues().apply {
        put("state", state.name); put("error", error)
    })
    /** Forget finished jobs; files already saved to Downloads are never touched. */
    @Synchronized fun remove(id: String) {
        val task = get(id) ?: return
        if (task.state in TaskPolicy.inFlight || task.state == TaskState.QUEUED) return
        writableDatabase.delete("outputs", "task_id=?", arrayOf(id))
        writableDatabase.delete("tasks", "id=?", arrayOf(id))
    }
    @Synchronized fun clearFinished() = list().filter { it.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED, TaskState.FAILED) }.forEach { remove(it.id) }
    /** A new address for an unfinished task (an expired signed link), kept as the same task so what was downloaded is reused.
     * Returns false for a finished or running task, or an address that is not a public web link. */
    @Synchronized fun updateUrl(id: String, url: String): Boolean {
        val task = get(id) ?: return false
        if (task.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED) || task.state in TaskPolicy.inFlight) return false
        val canonical = runCatching { LinkUtils.canonicalUrl(url) }.getOrNull() ?: return false
        update(id, ContentValues().apply { put("url", canonical); put("state", TaskState.QUEUED.name); put("error", "") })
        return true
    }
    /** Start the task at [startAt] (epoch millis; 0 clears it). A paused or failed task is queued again so the time can take effect. */
    @Synchronized fun schedule(id: String, startAt: Long) {
        val task = get(id) ?: return
        if (task.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED) || task.state in TaskPolicy.inFlight) return
        update(id, ContentValues().apply {
            put("start_at", startAt.coerceAtLeast(0))
            if (task.state in setOf(TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK)) { put("state", TaskState.QUEUED.name); put("error", "") }
        })
    }
    @Synchronized fun recover() {
        writableDatabase.beginTransaction()
        try {
            for (task in list()) {
                val recovered = if (task.state == TaskState.PAUSED && task.error == "chunk_restart") TaskState.QUEUED else TaskPolicy.recover(task.state)
                if (recovered != task.state) state(task.id, recovered, "interrupted")
            }
            writableDatabase.setTransactionSuccessful()
        } finally { writableDatabase.endTransaction() }
    }
    private fun read(c: Cursor): MobileTask {
        fun s(name: String) = c.getString(c.getColumnIndexOrThrow(name))
        fun n(name: String) = c.getLong(c.getColumnIndexOrThrow(name))
        return MobileTask(s("id"), s("url"), s("title"), TaskState.valueOf(s("state")),
            if (c.isNull(c.getColumnIndexOrThrow("height"))) null else n("height").toInt(), n("audio") == 1L,
            s("kind"), n("progress").toInt(), n("notification_id").toInt(), s("error"), s("uri"), s("file_name"),
            s("mime"), s("selected_items"), n("bytes_done"), n("total_bytes"), s("validator"), n("created_at"), n("start_at"))
    }
    companion object {
        @Volatile private var instance: TaskStore? = null
        fun get(context: Context): TaskStore = instance ?: synchronized(this) {
            instance ?: TaskStore(context.applicationContext).also { instance = it }
        }
    }
}
