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
            catch (error: Exception) { ShareProbe.Failed(if (error is TransferFailure) error.code else "unsupported_media") }
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
        setContentView(ScrollView(this).apply { addView(box) })
        model = ViewModelProvider(this)[ShareModel::class.java]
        val shared = intent?.getStringExtra(Intent.EXTRA_TEXT)
        val text = shared ?: if (intent?.getBooleanExtra(EXTRA_FROM_CLIPBOARD, false) == true) clipboardText() else null
        val urls = LinkUtils.extractUrls(text)
        if (urls.isEmpty() || urls.any { !LinkUtils.isPublicHttpUrl(it) }) { finishWith(R.string.bad_link); return }
        val prefs = MobilePreferences(this)
        val allowed = TaskPolicy.mayRun(prefs.networkPolicy, MobileNetwork.snapshot(this), prefs.allowRoaming)
        if (prefs.quickDownload) { enqueue(urls, prefs.defaultHeight, prefs.defaultAudio); return }
        if (urls.size > 1 || !allowed) {
            box.addView(label(if (!allowed) getString(R.string.waiting_network) else "${urls.size} · ${getString(R.string.add_links)}"))
            box.addView(button(getString(R.string.download_selected)) { enqueue(urls, prefs.defaultHeight, prefs.defaultAudio) })
            box.addView(button(getString(R.string.cancel)) { finish() }); return
        }
        model.probe(applicationContext, urls.first())
        lifecycleScope.launch { model.state.collect { value -> when (value) {
            ShareProbe.Loading -> { box.removeAllViews(); box.addView(label(getString(R.string.checking))); box.addView(ProgressBar(this@ShareActivity)) }
            is ShareProbe.Ready -> choices(value.info)
            is ShareProbe.Failed -> {
                box.removeAllViews(); box.addView(label(MobileText.error(this@ShareActivity, value.code)))
                box.addView(button(getString(R.string.file_download)) { enqueue(urls, null, false, "file") })
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
    private fun enqueue(urls: List<String>, height: Int?, audio: Boolean, kind: String = "media") {
        urls.forEach { DownloadService.start(this, it, height, audio, "", kind) }; finish()
    }
    private fun finishWith(message: Int) { Toast.makeText(this, message, Toast.LENGTH_LONG).show(); finish() }
    companion object { const val EXTRA_FROM_CLIPBOARD = "from_clipboard" }
}
