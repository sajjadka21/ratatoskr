package app.ratatoskr.android
import org.junit.Assert.*
import org.junit.Test
class RowTapPolicyTest {
    @Test fun failedAndWaitingTapResumeWithoutOpeningDetails() {
        listOf(TaskState.FAILED, TaskState.WAITING_NETWORK, TaskState.PAUSED, TaskState.SAVED).forEach { assertEquals(RowTapAction.RESUME, RowTapPolicy.action(it, false)) }
    }
    @Test fun selectionNeverStartsDownloads() {
        TaskState.entries.forEach { assertEquals(RowTapAction.SELECT, RowTapPolicy.action(it, true)) }
    }
    @Test fun queuedAndScheduledWorkDoesNotBypassSchedulerOnTap() {
        assertEquals(RowTapAction.DETAILS, RowTapPolicy.action(TaskState.QUEUED, false))
        assertEquals(RowTapAction.OPEN, RowTapPolicy.action(TaskState.COMPLETED, false))
    }
}
