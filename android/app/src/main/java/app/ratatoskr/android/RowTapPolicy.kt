package app.ratatoskr.android

enum class RowTapAction { SELECT, RESUME, OPEN, DETAILS }
object RowTapPolicy {
    fun action(state: TaskState, selecting: Boolean): RowTapAction = when {
        selecting -> RowTapAction.SELECT
        state in setOf(TaskState.FAILED, TaskState.WAITING_NETWORK, TaskState.PAUSED, TaskState.SAVED) -> RowTapAction.RESUME
        state == TaskState.COMPLETED -> RowTapAction.OPEN
        else -> RowTapAction.DETAILS
    }
}
