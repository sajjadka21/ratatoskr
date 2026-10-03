package app.ratatoskr.android

internal object TransferProgress {
    fun fraction(task: MobileTask): Float? {
        if (task.state == TaskState.PROBING) return null
        if (task.totalBytes > 0 && task.bytesDone > 0) return (task.bytesDone.toDouble() / task.totalBytes).toFloat().coerceIn(0f, 1f)
        if (task.progress > 0 || task.totalBytes > 0) return task.progress.coerceIn(0, 100) / 100f
        return null
    }
}
