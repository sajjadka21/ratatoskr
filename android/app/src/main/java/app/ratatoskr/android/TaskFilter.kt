package app.ratatoskr.android

/** The category tabs of the list (Video, Music, …). Pure, so it runs as a plain unit test. */
object TaskFilter {
    /** Order shown in the filter row; "Images" are the files [FileCategory] leaves out of the folders. */
    val categories = listOf("Video", "Music", "Archives", "Programs", "Documents", "Images", "Other")

    fun categoryOf(task: MobileTask): String {
        if (task.fileName.isEmpty()) {
            if (task.audioOnly || Spotify.isTrackUrl(task.url)) return "Music"
            if (task.kind != "file") return "Video"
            val inferred = FileCategory.folder(TaskQuery.name(task), task.mime)
            return if (TaskQuery.extension(task) in setOf("jpg", "jpeg", "png", "gif", "webp")) "Images" else inferred.ifEmpty { "Images" }
        }
        return FileCategory.folder(task.fileName, task.mime).ifEmpty { "Images" }
    }

    /** True when [category] is null (show everything) or is the task's own. */
    fun matches(task: MobileTask, category: String?) = category == null || categoryOf(task) == category

    /** The categories that have at least one task, in display order. */
    fun present(tasks: List<MobileTask>): List<String> = tasks.map(::categoryOf).toSet().let { found -> categories.filter { it in found } }
}
