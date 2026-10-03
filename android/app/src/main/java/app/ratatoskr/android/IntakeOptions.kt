package app.ratatoskr.android

enum class IntakeMode { NOW, SAVE, QUEUE, SCHEDULE }

/** Persist the chosen action together with the job, before any executor can see it. */
data class IntakeOptions(val mode: IntakeMode = IntakeMode.NOW, val startAt: Long = 0, val groupName: String = "", val allowDuplicate: Boolean = false) {
    val initialState get() = if (mode in setOf(IntakeMode.SAVE, IntakeMode.QUEUE)) TaskState.SAVED else TaskState.QUEUED
    fun validate(now: Long = System.currentTimeMillis()) {
        require(groupName.trim().length <= 80) { "group_too_long" }
        require(mode != IntakeMode.QUEUE || groupName.isNotBlank()) { "group_required" }
        require((mode != IntakeMode.SCHEDULE && startAt == 0L) || startAt > now) { "schedule_past" }
        require(mode in setOf(IntakeMode.SCHEDULE, IntakeMode.QUEUE) || startAt == 0L) { "invalid_schedule" }
    }
}

/** Filtering only presents the journal; it never starts or changes a task. */
object TaskQuery {
    fun nameFromUrl(url: String): String = runCatching {
        java.net.URI(url).path.substringAfterLast('/').ifEmpty { LinkPlan.host(url) }
    }.getOrDefault(LinkPlan.host(url)).take(300)
    fun name(task: MobileTask) = task.fileName.ifEmpty { task.title.ifEmpty { nameFromUrl(task.url) } }
    fun extension(task: MobileTask): String = name(task).substringAfterLast('.', "").lowercase(java.util.Locale.ROOT)
        .takeIf { it.matches(Regex("[a-z0-9]{1,10}")) }.orEmpty()
    fun apply(tasks: List<MobileTask>, query: String, sort: Int, category: String?, format: String?, from: Long, until: Long): List<MobileTask> {
        val shown = tasks.filter { (query.isBlank() || name(it).contains(query.trim(), true)) && TaskFilter.matches(it, category) &&
            (format.isNullOrEmpty() || extension(it) == format) && (from == 0L || it.createdAt >= from) && (until == 0L || it.createdAt < until) }
        return when (sort) {
            5 -> shown.sortedWith(compareBy<MobileTask> { it.queuePosition }.thenBy { it.id })
            1 -> shown.sortedWith(compareBy<MobileTask> { it.createdAt }.thenBy { it.id })
            2 -> shown.sortedWith(compareBy<MobileTask> { name(it).lowercase(java.util.Locale.ROOT) }.thenBy { it.id })
            3 -> shown.sortedWith(compareByDescending<MobileTask> { it.totalBytes }.thenBy { it.id })
            4 -> shown.sortedWith(compareBy<MobileTask> { extension(it) }.thenBy { name(it).lowercase(java.util.Locale.ROOT) })
            else -> shown.sortedWith(compareByDescending<MobileTask> { it.createdAt }.thenBy { it.id })
        }
    }
}
