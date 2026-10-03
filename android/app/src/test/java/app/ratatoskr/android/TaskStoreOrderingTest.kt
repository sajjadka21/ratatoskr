package app.ratatoskr.android
import android.content.Context
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.SQLiteMode
import java.util.UUID

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
@SQLiteMode(SQLiteMode.Mode.NATIVE)
class TaskStoreOrderingTest {
    @Test fun versionFourUpgradePreservesDownloadAndStableOrder() {
        val context: Context = RuntimeEnvironment.getApplication()
        val name = "upgrade-${UUID.randomUUID()}.db"
        context.openOrCreateDatabase(name, 0, null).use { db ->
            db.execSQL("""CREATE TABLE tasks (
                notification_id INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE,
                url TEXT NOT NULL, title TEXT NOT NULL, state TEXT NOT NULL, height INTEGER,
                audio INTEGER NOT NULL, kind TEXT NOT NULL, progress INTEGER NOT NULL DEFAULT 0,
                error TEXT NOT NULL DEFAULT '', uri TEXT NOT NULL DEFAULT '', file_name TEXT NOT NULL DEFAULT '',
                mime TEXT NOT NULL DEFAULT '', selected_items TEXT NOT NULL DEFAULT '',
                bytes_done INTEGER NOT NULL DEFAULT 0, total_bytes INTEGER NOT NULL DEFAULT -1,
                validator TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
                start_at INTEGER NOT NULL DEFAULT 0, group_name TEXT NOT NULL DEFAULT ''
            )""")
            db.execSQL("CREATE TABLE outputs (task_id TEXT NOT NULL, position INTEGER NOT NULL, uri TEXT NOT NULL, name TEXT NOT NULL, mime TEXT NOT NULL, PRIMARY KEY(task_id, position))")
            db.execSQL("INSERT INTO tasks (id,url,title,state,audio,kind,bytes_done,validator,created_at,updated_at) VALUES ('old','https://example.com/a.zip','a','PAUSED',0,'file',123,'old-validator',1,1)")
            db.version = 4
        }
        val store = TaskStore(context, name)
        try {
            val task = store.get("old")!!
            assertEquals(TaskState.PAUSED, task.state)
            assertEquals(123L, task.bytesDone)
            assertEquals("old-validator", task.validator)
            assertEquals(task.notificationId.toLong(), task.queuePosition)
            val next = store.enqueue("https://example.com/b.zip", null, false, "b", "file")
            assertTrue(next.queuePosition > task.queuePosition)
        } finally { store.close(); context.deleteDatabase(name) }
    }
    @Test fun waitingOrderPersistsWithoutTouchingCheckpointsOrOtherGroups() {
        val context: Context = RuntimeEnvironment.getApplication()
        val name = "order-${UUID.randomUUID()}.db"
        var store = TaskStore(context, name)
        try {
            fun add(title: String, group: String) = store.enqueue("https://example.com/$title.zip", null, false, title, "file", options = IntakeOptions(IntakeMode.QUEUE, groupName = group))
            val a = add("a", "show"); val b = add("b", "show"); val other = add("c", "other")
            store.update(a.id, android.content.ContentValues().apply { put("bytes_done", 123L); put("validator", "unchanged") })
            assertTrue(store.move(b.id, true))
            assertFalse(store.move(b.id, true))
            assertEquals(listOf(b.id, a.id), store.list().filter { it.groupName == "show" }.map { it.id })
            assertEquals(other.queuePosition, store.get(other.id)!!.queuePosition)
            store.close(); store = TaskStore(context, name)
            assertEquals(listOf(b.id, a.id), store.list().filter { it.groupName == "show" }.map { it.id })
            assertEquals(123L, store.get(a.id)!!.bytesDone)
            assertEquals("unchanged", store.get(a.id)!!.validator)
            store.state(b.id, TaskState.DOWNLOADING)
            assertFalse(store.move(b.id, false))
        } finally { store.close(); context.deleteDatabase(name) }
    }
}
