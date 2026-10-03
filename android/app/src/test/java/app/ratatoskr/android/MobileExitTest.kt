package app.ratatoskr.android

import android.app.job.JobScheduler
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.SQLiteMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
@SQLiteMode(SQLiteMode.Mode.NATIVE)
class MobileExitTest {
    @Test fun exitPausesPendingWorkAndLeavesSavedDownloadsIntact() {
        val context = RuntimeEnvironment.getApplication()
        val store = TaskStore(context, "exit-${java.util.UUID.randomUUID()}.db")
        try {
            val queued = store.enqueue("https://example.com/a", null, false, "a", "file")
            val saved = store.enqueue("https://example.com/b", null, false, "b", "file", options = IntakeOptions(IntakeMode.SAVE))
            NetworkJobs.schedule(context, store)
            MobileExit.stop(context, store)
            assertEquals(TaskState.PAUSED, store.get(queued.id)!!.state)
            assertEquals(TaskState.SAVED, store.get(saved.id)!!.state)
            val scheduler = context.getSystemService(JobScheduler::class.java)
            assertNull(scheduler.getPendingJob(NetworkJobs.JOB_ID))
            assertNull(scheduler.getPendingJob(NetworkJobs.TIMER_JOB_ID))
        } finally { val name = store.databaseName; store.close(); context.deleteDatabase(name) }
    }
    @Test fun emptyQueuesAndCalendarPreferenceSurviveNewPreferenceInstances() {
        val context = RuntimeEnvironment.getApplication()
        val prefs = MobilePreferences(context)
        prefs.namedQueues = setOf("سریال")
        prefs.calendarType = "persian"
        assertEquals(setOf("سریال"), MobilePreferences(context).namedQueues)
        assertEquals("persian", MobilePreferences(context).calendarType)
        prefs.namedQueues = emptySet()
        prefs.calendarType = "gregorian"
    }
}
