package app.ratatoskr.android

import android.content.Context
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.util.UUID

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
class RefreshLinkTest {
    private lateinit var context: Context
    private lateinit var name: String
    private lateinit var store: TaskStore

    @Before fun open() { context = RuntimeEnvironment.getApplication(); name = "refresh-${UUID.randomUUID()}.db"; store = TaskStore(context, name) }
    @After fun close() { store.close(); context.deleteDatabase(name) }

    @Test fun aRefreshedSignedLinkIsTheSameResource() {
        assertTrue(LinkUtils.sameResource("https://cdn.example.com/f/a.zip?sig=old", "https://CDN.example.com/f/a.zip?sig=new"))
        assertFalse(LinkUtils.sameResource("https://cdn.example.com/f/a.zip?sig=1", "https://cdn.example.com/f/b.zip?sig=1"))
        assertFalse(LinkUtils.sameResource("https://cdn.example.com/f/a.zip", "https://other.example.com/f/a.zip"))
        assertFalse(LinkUtils.sameResource("not a url", "https://cdn.example.com/f/a.zip"))
    }

    @Test fun anUnfinishedTaskTakesTheNewAddressAndIsQueuedAgain() {
        val task = store.enqueue("https://cdn.example.com/f/a.zip?sig=old", null, false, "", "file")
        store.state(task.id, TaskState.FAILED, "auth_required")
        assertTrue(store.updateUrl(task.id, "https://cdn.example.com/f/a.zip?sig=new"))
        val changed = store.get(task.id)!!
        assertEquals("https://cdn.example.com/f/a.zip?sig=new", changed.url)
        assertEquals(TaskState.QUEUED, changed.state)
        assertEquals("", changed.error)
    }

    @Test fun finishedRunningAndUnsafeAddressesAreRefused() {
        val task = store.enqueue("https://cdn.example.com/f/a.zip", null, false, "", "file")
        assertFalse(store.updateUrl(task.id, "http://127.0.0.1/a.zip"))
        assertFalse(store.updateUrl(task.id, "file:///etc/passwd"))
        store.state(task.id, TaskState.DOWNLOADING)
        assertFalse(store.updateUrl(task.id, "https://cdn.example.com/f/b.zip"))
        store.state(task.id, TaskState.COMPLETED)
        assertFalse(store.updateUrl(task.id, "https://cdn.example.com/f/b.zip"))
        assertFalse(store.updateUrl("missing", "https://cdn.example.com/f/b.zip"))
    }
}
