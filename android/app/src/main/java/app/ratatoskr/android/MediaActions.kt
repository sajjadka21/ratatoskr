package app.ratatoskr.android

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri

object MediaActions {
    fun openIntent(media: SavedMedia): Intent = Intent(Intent.ACTION_VIEW).setDataAndType(Uri.parse(media.uri), media.mime)
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
    fun shareIntent(media: List<SavedMedia>): Intent {
        require(media.isNotEmpty())
        val uris = ArrayList(media.map { Uri.parse(it.uri) })
        val mime = if (media.map { it.mime }.distinct().size == 1) media.first().mime else "*/*"
        return Intent(if (uris.size == 1) Intent.ACTION_SEND else Intent.ACTION_SEND_MULTIPLE).apply {
            type = mime
            if (uris.size == 1) putExtra(Intent.EXTRA_STREAM, uris.first()) else putParcelableArrayListExtra(Intent.EXTRA_STREAM, uris)
            clipData = ClipData.newRawUri(media.first().name, uris.first()).also { clip -> uris.drop(1).forEach { clip.addItem(ClipData.Item(it)) } }
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        }
    }
    fun open(context: Context, media: SavedMedia) { context.startActivity(openIntent(media)) }
    fun share(context: Context, media: List<SavedMedia>) { context.startActivity(Intent.createChooser(shareIntent(media), context.getString(R.string.share_file))) }
}

object MobileText {
    fun state(context: Context, task: MobileTask): String = context.getString(when (task.state) {
        TaskState.QUEUED -> R.string.queued
        TaskState.SAVED -> R.string.saved_only
        TaskState.PROBING -> R.string.preparing_download
        TaskState.DOWNLOADING -> R.string.downloading
        TaskState.MERGING -> R.string.merging
        TaskState.SAVING -> R.string.saving
        TaskState.PAUSED -> R.string.paused
        TaskState.WAITING_NETWORK -> R.string.waiting_network
        TaskState.NEEDS_SELECTION -> R.string.choose_quality
        TaskState.FAILED -> R.string.failed
        TaskState.COMPLETED -> R.string.completed
        TaskState.CANCELLED -> R.string.cancelled
    }) + if (task.state in TaskPolicy.inFlight && task.state != TaskState.PROBING && task.progress > 0) " ${task.progress}%" else ""
    fun error(context: Context, code: String): String = context.getString(when (code) {
        "no_space" -> R.string.error_space
        "rate_limited" -> R.string.error_rate
        "auth_required" -> R.string.error_private
        "not_found" -> R.string.error_removed
        "not_a_file" -> R.string.error_not_file
        "invalid_range", "incomplete" -> R.string.error_integrity
        "cannot_write" -> R.string.error_write
        "unsupported_media" -> R.string.error_unsupported
        "invalid_output" -> R.string.error_invalid_output
        "extractor_failed" -> R.string.error_extractor
        "system_timeout" -> R.string.error_timeout
        "storage_permission" -> R.string.error_storage
        "bad_link" -> R.string.bad_link
        "interrupted" -> R.string.error_interrupted
        "waiting_network", "network" -> R.string.error_network
        else -> R.string.error_retry
    })
}
