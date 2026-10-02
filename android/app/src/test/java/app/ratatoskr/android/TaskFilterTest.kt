package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TaskFilterTest {
    private fun task(file: String = "", mime: String = "", kind: String = "media", audio: Boolean = false, url: String = "https://example.com/x") =
        MobileTask("id", url, "", TaskState.COMPLETED, audioOnly = audio, kind = kind, fileName = file, mime = mime)

    @Test fun categoriesFollowTheFileNotTheLink() {
        assertEquals("Video", TaskFilter.categoryOf(task("a.mp4", "video/mp4")))
        assertEquals("Music", TaskFilter.categoryOf(task("a.m4a", "audio/mp4")))
        assertEquals("Archives", TaskFilter.categoryOf(task("a.zip")))
        assertEquals("Programs", TaskFilter.categoryOf(task("a.apk")))
        assertEquals("Documents", TaskFilter.categoryOf(task("a.pdf")))
        assertEquals("Images", TaskFilter.categoryOf(task("a.jpg", "image/jpeg")))
        assertEquals("Other", TaskFilter.categoryOf(task("a.xyz")))
    }

    @Test fun aTaskWithoutAFileYetIsGuessedFromHowItWasAdded() {
        assertEquals("Video", TaskFilter.categoryOf(task()))
        assertEquals("Music", TaskFilter.categoryOf(task(audio = true)))
        assertEquals("Music", TaskFilter.categoryOf(task(url = "https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT")))
        assertEquals("Other", TaskFilter.categoryOf(task(kind = "file")))
    }

    @Test fun nullShowsEverythingAndPresentKeepsDisplayOrder() {
        val tasks = listOf(task("a.pdf"), task("a.mp4"), task("b.pdf"))
        assertTrue(tasks.all { TaskFilter.matches(it, null) })
        assertTrue(TaskFilter.matches(tasks[1], "Video"))
        assertFalse(TaskFilter.matches(tasks[0], "Video"))
        assertEquals(listOf("Video", "Documents"), TaskFilter.present(tasks))
    }
}
