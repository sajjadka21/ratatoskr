package app.ratatoskr.android

import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import androidx.recyclerview.widget.DiffUtil
import androidx.recyclerview.widget.ListAdapter
import androidx.recyclerview.widget.RecyclerView

/** One line of the list: the task plus the texts derived from it, so a change in either redraws the row. */
data class TaskRow(val task: MobileTask, val stats: String, val schedule: String, val selected: Boolean = false, val thumbnail: String? = null)

interface TaskActions {
    fun group(name: String, start: Boolean) {}
    /** True while some downloads are selected; a tap then selects instead of opening the details. */
    val selecting: Boolean
    fun toggle(task: MobileTask)
    fun tap(task: MobileTask) {
        when (RowTapPolicy.action(task.state, selecting)) {
            RowTapAction.SELECT -> toggle(task)
            RowTapAction.RESUME -> command(task, DownloadService.ACTION_RESUME)
            RowTapAction.OPEN -> open(task)
            RowTapAction.DETAILS -> details(task)
        }
    }
    fun details(task: MobileTask)
    fun command(task: MobileTask, action: String)
    fun open(task: MobileTask)
    fun share(task: MobileTask)
    fun schedule(task: MobileTask)
    fun remove(task: MobileTask)
}

class TaskAdapter(private val activity: MobileActivity, private val actions: TaskActions) : ListAdapter<TaskRow, TaskAdapter.Holder>(Diff) {
    object Diff : DiffUtil.ItemCallback<TaskRow>() {
        override fun areItemsTheSame(a: TaskRow, b: TaskRow) = a.task.id == b.task.id
        override fun areContentsTheSame(a: TaskRow, b: TaskRow) = a == b
    }

    class Holder(val card: LinearLayout, val badge: ImageView, val title: TextView, val subtitle: TextView, val status: LinearLayout,
                 val progress: ProgressBar, val stats: TextView, val error: TextView, val buttons: LinearLayout) : RecyclerView.ViewHolder(card)

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Holder {
        val a = activity
        val card = a.column().apply {
            setPadding(a.dp(14), a.dp(12), a.dp(14), a.dp(8))
            background = a.rounded(a.surface, 14)
            layoutParams = RecyclerView.LayoutParams(-1, -2).apply { bottomMargin = a.dp(10) }
        }
        val top = LinearLayout(a).apply { gravity = android.view.Gravity.CENTER_VERTICAL }
        val badge = ImageView(a).apply {
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            setPadding(a.dp(10), a.dp(10), a.dp(10), a.dp(10)); setColorFilter(a.accent); background = a.rounded(a.paper, 12)
            layoutParams = LinearLayout.LayoutParams(a.dp(44), a.dp(44)).apply { marginEnd = a.dp(12) }
        }
        val texts = a.column().apply { layoutParams = LinearLayout.LayoutParams(0, -2, 1f) }
        val title = TextView(a).apply { textSize = 16f; typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL); setTextColor(a.ink); maxLines = 2; ellipsize = android.text.TextUtils.TruncateAt.END }
        val subtitle = TextView(a).apply { textSize = 12f; setTextColor(a.muted); maxLines = 1; ellipsize = android.text.TextUtils.TruncateAt.END }
        texts.addView(title); texts.addView(subtitle)
        top.addView(badge); top.addView(texts)
        val status = LinearLayout(a).apply { setPadding(0, a.dp(8), 0, 0); gravity = android.view.Gravity.CENTER_VERTICAL }
        val progress = ProgressBar(a, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = 100; progressTintList = android.content.res.ColorStateList.valueOf(a.accent)
            progressBackgroundTintList = android.content.res.ColorStateList.valueOf(a.accent and 0x33FFFFFF)
            layoutParams = LinearLayout.LayoutParams(-1, a.dp(8)).apply { topMargin = a.dp(8) }
        }
        val stats = TextView(a).apply { textSize = 12f; setTextColor(a.muted); setPadding(0, a.dp(6), 0, 0) }
        val error = TextView(a).apply { textSize = 12f; setTextColor(a.danger); setPadding(0, a.dp(4), 0, 0) }
        val buttons = LinearLayout(a).apply { gravity = android.view.Gravity.END; setPadding(0, a.dp(4), 0, 0) }
        card.addView(top); card.addView(status); card.addView(progress); card.addView(stats); card.addView(error); card.addView(buttons)
        return Holder(card, badge, title, subtitle, status, progress, stats, error, buttons)
    }

    override fun onBindViewHolder(h: Holder, position: Int) {
        val a = activity
        val row = getItem(position); val task = row.task
        h.card.setOnClickListener { actions.tap(task) }
        h.card.setOnLongClickListener { actions.toggle(task); true }
        h.card.background = a.rounded(a.surface, 14, if (row.selected) a.accent else null).also { if (row.selected) (it as android.graphics.drawable.GradientDrawable).setStroke(a.dp(2), a.accent) }
        h.badge.setImageResource(if (row.selected) R.drawable.ic_kind_check else badgeFor(task))
        h.title.text = task.title.ifEmpty { task.fileName.ifEmpty { LinkPlan.host(task.url).ifEmpty { a.getString(R.string.app_name) } } }
        h.subtitle.text = LinkPlan.host(task.url)
        val color = when (task.state) {
            TaskState.COMPLETED -> a.success
            TaskState.FAILED -> a.danger
            in TaskPolicy.inFlight -> a.accent
            else -> a.muted
        }
        h.status.removeAllViews()
        h.status.addView(a.chip(MobileText.state(a, task), color))
        if (row.schedule.isNotEmpty()) h.status.addView(a.chip(row.schedule, a.accent), LinearLayout.LayoutParams(-2, -2).apply { marginStart = a.dp(8) })
        val showBar = task.state in TaskPolicy.inFlight || (task.state == TaskState.PAUSED && task.progress > 0)
        h.progress.visibility = if (showBar) View.VISIBLE else View.GONE
        h.progress.isIndeterminate = task.state == TaskState.PROBING || (task.state in TaskPolicy.inFlight && task.progress == 0)
        h.progress.progress = task.progress
        h.stats.visibility = if (row.stats.isEmpty()) View.GONE else View.VISIBLE; h.stats.text = row.stats
        h.error.visibility = if (task.error.isEmpty()) View.GONE else View.VISIBLE
        h.error.text = if (task.error.isEmpty()) "" else MobileText.error(a, task.error)
        h.buttons.removeAllViews()
        fun add(res: Int, text: Int, tint: Int = a.accent, block: () -> Unit) =
            h.buttons.addView(a.icon(res, a.getString(text), tint) { runCatching(block).onFailure { android.widget.Toast.makeText(a, R.string.error_retry, android.widget.Toast.LENGTH_LONG).show() } })
        when (task.state) {
            TaskState.COMPLETED -> { add(R.drawable.ic_open, R.string.open_file) { actions.open(task) }; add(R.drawable.ic_share, R.string.share_file) { actions.share(task) } }
            TaskState.CANCELLED -> Unit
            TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK -> {
                add(R.drawable.ic_retry, R.string.resume) { actions.command(task, DownloadService.ACTION_RESUME) }
                add(R.drawable.ic_schedule, R.string.schedule) { actions.schedule(task) }
                add(R.drawable.ic_close, R.string.cancel, a.danger) { actions.command(task, DownloadService.ACTION_CANCEL) }
            }
            else -> {
                add(R.drawable.ic_pause, R.string.pause) { actions.command(task, DownloadService.ACTION_PAUSE) }
                if (task.state == TaskState.QUEUED) add(R.drawable.ic_schedule, R.string.schedule) { actions.schedule(task) }
                add(R.drawable.ic_close, R.string.cancel, a.danger) { actions.command(task, DownloadService.ACTION_CANCEL) }
            }
        }
        if (task.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED, TaskState.FAILED)) add(R.drawable.ic_delete, R.string.remove, a.danger) { actions.remove(task) }
        h.buttons.visibility = if (h.buttons.childCount == 0) View.GONE else View.VISIBLE
    }

    private fun badgeFor(task: MobileTask) = when (TaskFilter.categoryOf(task)) {
        "Video" -> R.drawable.ic_kind_video
        "Music" -> R.drawable.ic_kind_music
        "Archives" -> R.drawable.ic_kind_archive
        "Programs" -> R.drawable.ic_kind_program
        "Images" -> R.drawable.ic_kind_image
        else -> R.drawable.ic_kind_file
    }
}
