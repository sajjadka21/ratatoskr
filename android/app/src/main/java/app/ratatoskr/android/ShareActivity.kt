package app.ratatoskr.android

import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.widget.*
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import java.util.UUID

sealed class ShareProbe {
    data object Loading : ShareProbe()
    data class Ready(val info: LinkInfo) : ShareProbe()
    data class Failed(val code: String) : ShareProbe()
}
class ShareModel : ViewModel() {
    val state = MutableStateFlow<ShareProbe>(ShareProbe.Loading)
    val selected = mutableSetOf<Int>()
    private var started = false
    private val process = "probe-${UUID.randomUUID()}"
    fun probe(context: Context, url: String) {
        if (started) return
        started = true
        viewModelScope.launch {
            state.value = try {
                val info = withContext(Dispatchers.IO) { Engine.probe(context.applicationContext, url, process) }
                selected.addAll(info.items.map { it.index }); ShareProbe.Ready(info)
            } catch (error: CancellationException) { throw error }
            catch (error: Exception) { ShareProbe.Failed(DownloadService.errorCode(error)) }
        }
    }
    override fun onCleared() { Engine.cancel(process); super.onCleared() }
}

/** Instagram → Share → Ratatoskr: optionally enqueue immediately, otherwise choose
 * quality and album items. No accessibility overlay or account credentials. */
class ShareActivity : MobileActivity() {
    private lateinit var box: LinearLayout
    private lateinit var model: ShareModel
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        box = column().apply { setPadding(dp(20), dp(20), dp(20), dp(16)); setBackgroundColor(paper) }
        setContentView(ScrollView(this).apply { isVerticalScrollBarEnabled = false; isHorizontalScrollBarEnabled = false; addView(box) })
        model = ViewModelProvider(this)[ShareModel::class.java]
        val shared = if (intent?.action == Intent.ACTION_VIEW) {
            // ratatoskr://add?url=… from our own pages, or a plain file link opened with "Open with Ratatoskr"
            val data = intent?.dataString
            (if (data?.startsWith("http", true) == true) data.takeIf { LinkUtils.isPublicHttpUrl(it) } else LinkUtils.handoffUrl(data))
                ?: run { finishWith(R.string.bad_link); return }
        } else intent?.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        val text = shared ?: if (intent?.getBooleanExtra(EXTRA_FROM_CLIPBOARD, false) == true) clipboardText() else null
        val urls = LinkPlan.parse(text)
        if (urls.isEmpty() || urls.any { !LinkUtils.isPublicHttpUrl(it) }) { finishWith(R.string.bad_link); return }
        if (!intent.getBooleanExtra("choose-media-items", false)) {
            startActivity(Intent(this, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
                .putExtra(Intent.EXTRA_TEXT, urls.joinToString("\n")))
            finish(); return
        }
        val prefs = MobilePreferences(this)
        // A Spotify track has one sensible outcome: its audio. No quality question.
        if (urls.all { Spotify.isTrackUrl(it) }) { enqueue(urls, null, true); return }
        val allowed = TaskPolicy.mayRun(prefs.networkPolicy, MobileNetwork.snapshot(this), prefs.allowRoaming)
        if (prefs.quickDownload) { enqueue(urls, prefs.defaultHeight, prefs.defaultAudio); return }
        // A plain file link needs no questions: no quality to choose, so skip the media probe.
        if (urls.size == 1 && LinkPlan.classify(urls.first()) == LinkKind.FILE) { enqueue(urls, null, false); return }
        if (urls.size > 1 || !allowed) {
            val counts = LinkPlan.summarize(urls)
            box.addView(label(if (!allowed) getString(R.string.waiting_network) else getString(R.string.links_summary, urls.size, counts.files, counts.media)))
            box.addView(button(getString(R.string.download_selected)) { enqueue(urls, prefs.defaultHeight, prefs.defaultAudio) })
            box.addView(button(getString(R.string.cancel)) { finish() }); return
        }
        model.probe(applicationContext, urls.first())
        lifecycleScope.launch { model.state.collect { value -> when (value) {
            ShareProbe.Loading -> { box.removeAllViews(); box.addView(label(getString(R.string.checking))); box.addView(label(getString(R.string.checking_help), 14f)); box.addView(ProgressBar(this@ShareActivity)); box.addView(button(getString(R.string.cancel)) { finish() }) }
            is ShareProbe.Ready -> choices(value.info)
            is ShareProbe.Failed -> {
                if (LinkPlan.mayTryFile(urls.first(), value.code)) {
                    // Extensionless file endpoints still work with the single
                    // Download action. The HTTP engine rejects HTML responses.
                    enqueue(urls, null, false, "file")
                    return@collect
                }
                box.removeAllViews(); box.addView(label(MobileText.error(this@ShareActivity, value.code)))
                box.addView(button(getString(R.string.cancel)) { finish() })
            }
        } } }
    }
    private fun clipboardText(): String? = (getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).primaryClip
        ?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(this)?.toString()
    private fun choices(info: LinkInfo) {
        box.removeAllViews()
        box.addView(label(info.title.ifEmpty { "Ratatoskr" }, 20f))
        if (info.truncated) box.addView(label(getString(R.string.album_limit), 14f))
        if (info.items.size > 1) {
            box.addView(label(getString(R.string.select_items), 14f))
            for (item in info.items) box.addView(CheckBox(this).apply {
                minimumHeight = dp(48)
                setTextColor(ink)
                val kind = getString(when (item.kind) { "photo" -> R.string.photo; "audio" -> R.string.audio; else -> R.string.video })
                text = "${item.index}. $kind — ${item.title}"; isChecked = item.index in model.selected
                setOnCheckedChangeListener { _, checked -> if (checked) model.selected.add(item.index) else model.selected.remove(item.index) }
            })
        }
        fun choice(label: String, height: Int?, audio: Boolean) {
            box.addView(button(label) {
                val selected = model.selected.filter { index -> !audio || info.items.first { it.index == index }.kind != "photo" }.sorted()
                if (selected.isEmpty()) { Toast.makeText(this, R.string.selection_empty, Toast.LENGTH_SHORT).show(); return@button }
                DownloadService.start(this, info.url, height, audio, info.title, items = selected.joinToString(",")); finish()
            })
        }
        if (info.hasVideo) {
            box.addView(label(getString(R.string.choose_quality), 14f))
            if (info.heights.isEmpty()) choice(getString(R.string.best_quality), null, false)
            else info.heights.forEach { choice(getString(R.string.video_height, it), it, false) }
        } else choice(getString(R.string.download_selected), null, false)
        if (info.items.any { it.kind != "photo" }) choice(getString(R.string.audio_only), null, true)
        box.addView(button(getString(R.string.cancel)) { finish() })
    }
    private fun enqueue(urls: List<String>, height: Int?, audio: Boolean, kind: String? = null) {
        if (kind == null) DownloadService.startMany(this, urls, height, audio)
        else urls.forEach { DownloadService.start(this, it, height, audio, "", kind) }
        finish()
    }
    private fun finishWith(message: Int) { Toast.makeText(this, message, Toast.LENGTH_LONG).show(); finish() }
    companion object { const val EXTRA_FROM_CLIPBOARD = "from_clipboard" }
}
