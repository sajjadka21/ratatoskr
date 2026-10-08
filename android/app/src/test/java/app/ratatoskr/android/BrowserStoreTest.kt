package app.ratatoskr.android

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
class BrowserStoreTest {
    private val context: Context = RuntimeEnvironment.getApplication()

    @Before fun clearState() {
        context.getSharedPreferences("ratatoskr_browser", Context.MODE_PRIVATE).edit().clear().commit()
    }

    @Test fun historyKeepsOnlyPublicPagesAndMovesRepeatedPagesToTheTop() {
        BrowserStore.recordHistory(context, "file:///private.txt", "private")
        BrowserStore.recordHistory(context, "https://example.com/first", "First")
        BrowserStore.recordHistory(context, "https://example.com/second", "Second")
        BrowserStore.recordHistory(context, "https://example.com/first", "First updated")

        assertEquals(listOf("https://example.com/first", "https://example.com/second"), BrowserStore.history(context).map { it.url })
        assertEquals("First updated", BrowserStore.history(context).first().title)
    }

    @Test fun bookmarksToggleAndHistoryCanBeClearedIndependently() {
        val url = "https://example.com/page"
        assertTrue(BrowserStore.toggleBookmark(context, url, "Example"))
        assertTrue(BrowserStore.isBookmarked(context, url))
        assertFalse(BrowserStore.toggleBookmark(context, url, "Example"))
        assertFalse(BrowserStore.isBookmarked(context, url))

        BrowserStore.recordHistory(context, url, "Example")
        BrowserStore.toggleBookmark(context, url, "Example")
        BrowserStore.clearHistory(context)
        assertTrue(BrowserStore.history(context).isEmpty())
        assertTrue(BrowserStore.isBookmarked(context, url))
        BrowserStore.clearBookmarks(context)
        assertTrue(BrowserStore.bookmarks(context).isEmpty())
    }

    @Test fun historyHasAFixedBound() {
        (0..110).forEach { BrowserStore.recordHistory(context, "https://example.com/$it", "Page $it") }
        assertEquals(100, BrowserStore.history(context).size)
        assertEquals("https://example.com/110", BrowserStore.history(context).first().url)
    }
}

