package app.ratatoskr.android

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
class IntakeWorkflowTest {
    @Test fun heldAndScheduledJobsSurviveReopenWithoutStartingEarly() {
        val context = RuntimeEnvironment.getApplication()
        val database = "intake-${UUID.randomUUID()}.db"
        val store = TaskStore(context, database)
        val saved = store.enqueue("https://example.com/episode01.mp4", null, false, "Episode 1", "file", options = IntakeOptions(IntakeMode.QUEUE, groupName = "Series"))
        val time = System.currentTimeMillis() + 3_600_000
        val scheduled = store.enqueue("https://example.com/episode02.mp4", null, false, "Episode 2", "file", options = IntakeOptions(IntakeMode.SCHEDULE, time, "Series"))
        assertFalse(store.begin(saved.id)); assertFalse(store.begin(scheduled.id)); store.close()
        TaskStore(context, database).use { reopened ->
            reopened.recover()
            assertEquals(TaskState.SAVED, reopened.get(saved.id)!!.state)
            assertEquals("Series", reopened.get(saved.id)!!.groupName)
            assertEquals(time, reopened.get(scheduled.id)!!.startAt)
            assertFalse(Schedule.isDue(reopened.get(scheduled.id)!!.startAt, time - 1))
            reopened.schedule(saved.id, time)
            assertEquals(TaskState.QUEUED, reopened.get(saved.id)!!.state)
        }
        context.deleteDatabase(database)
    }
    @Test fun namedGroupsHaveOneOwnerAcrossBothExecutors() {
        val a = UUID.randomUUID().toString(); val b = UUID.randomUUID().toString(); val c = UUID.randomUUID().toString()
        val control = TransferControl { true }
        try {
            assertTrue(MobileRuntime.claim(a, control, 3, "Series"))
            assertFalse(MobileRuntime.claim(b, control, 3, "Series"))
            assertTrue(MobileRuntime.claim(c, control, 3, "Other series"))
            MobileRuntime.release(a)
            assertTrue(MobileRuntime.claim(b, control, 3, "Series"))
        } finally { listOf(a,b,c).forEach { MobileRuntime.release(it) } }
    }
    @Test fun pastSchedulesAndUnnamedQueuesAreRejectedBeforeJournalInsertion() {
        assertThrows(IllegalArgumentException::class.java) { IntakeOptions(IntakeMode.SCHEDULE, 99).validate(100) }
        assertThrows(IllegalArgumentException::class.java) { IntakeOptions(IntakeMode.QUEUE).validate() }
        assertEquals(TaskState.SAVED, IntakeOptions(IntakeMode.SAVE).initialState)
        IntakeOptions(IntakeMode.QUEUE, 200, "Series").validate(100)
        assertThrows(IllegalArgumentException::class.java) { IntakeOptions(IntakeMode.QUEUE, 99, "Series").validate(100) }
    }
    @Test fun filtersCombineNamesFormatsAndInclusiveLocalDates() {
        val tasks = listOf(
            MobileTask("1", "https://example.com/b.mp4", "b.mp4", TaskState.SAVED, createdAt=10),
            MobileTask("2", "https://example.com/a.mp4", "a.mp4", TaskState.SAVED, createdAt=20),
            MobileTask("3", "https://example.com/a.zip", "a.zip", TaskState.SAVED, createdAt=30))
        assertEquals(listOf("2"), TaskQuery.apply(tasks, "A", 0, null, "mp4", 20, 30).map { it.id })
        assertEquals(listOf("2","3","1"), TaskQuery.apply(tasks, "", 2, null, null, 0, 0).map { it.id })
        assertEquals(listOf("2","1","3"), TaskQuery.apply(tasks, "", 4, null, null, 0, 0).map { it.id })
    }
    @Test fun jalaliNowruzAndLeapDayMapToTheSameInstantsAsGregorian() {
        assertEquals(MobileDates.instant(false, 2024, 2, 20, 17, 45), MobileDates.instant(true, 1403, 0, 1, 17, 45))
        assertEquals(MobileDates.instant(false, 2021, 2, 20), MobileDates.instant(true, 1399, 11, 30))
        assertThrows(IllegalArgumentException::class.java) { MobileDates.instant(true, 1400, 11, 30) }
        val original = System.currentTimeMillis()
        val persian = MobileDates.calendar(true, original)
        assertEquals("persian", persian.type)
        assertEquals(original, persian.timeInMillis)
    }
    @Test fun directInstagramVideoWithNoFormatsIsNotRejectedOrSavedAsPoster() {
        val result = MediaMetadata.parse("https://www.instagram.com/reel/Dd_Rx17K9K3/", """{"title":"Reel","url":"https://cdn.example.com/video.mp4","ext":"mp4","thumbnail":"https://cdn.example.com/poster.jpg"}""")
        assertTrue(result.hasVideo); assertEquals("video", result.items.single().kind)
        assertNull(result.items.single().downloadUrl)
    }
    @Test fun knownVideoWithoutPlayableUrlDoesNotBecomeAThumbnailPhoto() {
        assertThrows(RuntimeException::class.java) {
            MediaMetadata.parse("https://www.instagram.com/reel/Test/", """{"ext":"mp4","formats":[],"thumbnails":[{"url":"https://cdn.example.com/poster.jpg"}]}""")
        }
    }
    @Test fun extensionlessFilesSkipExtractionAndFallbackCannotLoop() {
        assertEquals("Video", TaskFilter.categoryOf(MobileTask("pending", "https://example.com/episode.mp4", "", TaskState.SAVED, kind="file")))
        assertEquals(LinkKind.FILE, LinkPlan.classify("https://example.com/download/123"))
        assertEquals(LinkKind.MEDIA, LinkPlan.classify("https://example.com/stream.m3u8"))
        assertTrue(LinkPlan.mayTryMedia("https://example.com/watch/123", "not_a_file"))
        assertFalse(LinkPlan.mayTryMedia("https://www.instagram.com/reel/Test/", "not_a_file"))
        assertFalse(LinkPlan.mayTryMedia("https://example.com/file.zip", "not_a_file"))
        assertEquals("auth_required", DownloadService.errorCode(Exception("Instagram login required")))
        assertEquals("extractor_failed", DownloadService.errorCode(Exception("Unable to extract video data")))
    }
}
