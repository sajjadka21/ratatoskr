package app.ratatoskr.android

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.widget.*
import androidx.appcompat.app.AlertDialog
import androidx.core.widget.doAfterTextChanged
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.*

class MainActivity : MobileActivity() {
    private lateinit var tasks: LinearLayout
    private var history = false
    private var query = ""
    private var signature = ""
    private val meter = SpeedMeter()
    private lateinit var banner: LinearLayout
    private var dismissedLink = ""
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        history = savedInstanceState?.getBoolean("history") ?: false
        query = savedInstanceState?.getString("query").orEmpty()
        MobileRuntime.initialize(TaskStore.get(this))
        NetworkJobs.schedule(this)
        val box = column().apply { setBackgroundColor(paper) }
        insets(box)
        box.addView(label("Ratatoskr", 28f))
        box.addView(label(getString(R.string.main_hint), 14f))
        banner = column().apply { visibility = android.view.View.GONE }
        box.addView(banner)
        val actions = LinearLayout(this)
        actions.addView(button(getString(R.string.add_links)) { addLinks() }, LinearLayout.LayoutParams(0, -2, 1f))
        actions.addView(button(getString(R.string.paste)) { startActivity(Intent(this, ShareActivity::class.java).putExtra(ShareActivity.EXTRA_FROM_CLIPBOARD, true)) }, LinearLayout.LayoutParams(0, -2, 1f))
        box.addView(actions)
        val tabs = LinearLayout(this)
        tabs.addView(button(getString(R.string.active_jobs)) { history = false; render(true) }, LinearLayout.LayoutParams(0, -2, 1f))
        tabs.addView(button(getString(R.string.history)) { history = true; render(true) }, LinearLayout.LayoutParams(0, -2, 1f))
        box.addView(tabs)
        val bulk = LinearLayout(this)
        bulk.addView(button(getString(R.string.pause_all)) { forEach(TaskPolicy.inFlight + TaskState.QUEUED + TaskState.WAITING_NETWORK, DownloadService.ACTION_PAUSE) }, LinearLayout.LayoutParams(0, -2, 1f))
        bulk.addView(button(getString(R.string.resume_all)) { forEach(setOf(TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK), DownloadService.ACTION_RESUME) }, LinearLayout.LayoutParams(0, -2, 1f))
        bulk.addView(button(getString(R.string.clear_finished)) { TaskStore.get(this).clearFinished(); render(true) }, LinearLayout.LayoutParams(0, -2, 1f))
        box.addView(bulk)
        box.addView(EditText(this).apply {
            hint = getString(R.string.search_history); setTextColor(ink); setText(query); maxLines = 1
            doAfterTextChanged { query = it.toString(); render(true) }
        })
        tasks = column()
        box.addView(ScrollView(this).apply { addView(tasks) }, LinearLayout.LayoutParams(-1, 0, 1f))
        val footer = LinearLayout(this)
        footer.addView(button(getString(R.string.settings)) { settings() }, LinearLayout.LayoutParams(0, -2, 1f))
        footer.addView(button(getString(R.string.about)) { about() }, LinearLayout.LayoutParams(0, -2, 1f))
        box.addView(footer)
        setContentView(box)
        lifecycleScope.launch { repeatOnLifecycle(Lifecycle.State.STARTED) { while (isActive) { render(); delay(750) } } }
        if (android.os.Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED)
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
    }
    override fun onSaveInstanceState(outState: Bundle) {
        outState.putBoolean("history", history); outState.putString("query", query); super.onSaveInstanceState(outState)
    }
    private fun render(force: Boolean = false) {
        if (!::tasks.isInitialized) return
        val list = TaskStore.get(this).list().asReversed().filter {
            val past = it.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED, TaskState.FAILED)
            past == history && (query.isBlank() || it.title.contains(query, true) || it.fileName.contains(query, true))
        }
        list.forEach { if (it.state !in TaskPolicy.inFlight) meter.forget(it.id) }
        val next = list.toString()
        if (!force && next == signature) return
        signature = next; tasks.removeAllViews()
        if (list.isEmpty()) tasks.addView(label(getString(R.string.empty_jobs)))
        for (task in list) {
            val card = column().apply {
                setPadding(dp(14), dp(12), dp(14), dp(12))
                background = android.graphics.drawable.GradientDrawable().apply { setColor(surface); cornerRadius = dp(16).toFloat(); setStroke(dp(1), accent and 0x55FFFFFF) }
            }
            card.addView(label(task.title.ifEmpty { task.fileName.ifEmpty { getString(R.string.app_name) } }))
            card.addView(label(MobileText.state(this, task), 14f))
            val stats = stats(task)
            if (stats.isNotEmpty()) card.addView(label(stats, 13f))
            if (task.error.isNotEmpty()) card.addView(label(MobileText.error(this, task.error), 14f))
            if (task.state in TaskPolicy.inFlight) card.addView(ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply { max = 100; progress = task.progress })
            val row = LinearLayout(this)
            fun action(text: Int, block: () -> Unit) { row.addView(button(getString(text)) { runCatching(block).onFailure { Toast.makeText(this, R.string.error_retry, Toast.LENGTH_LONG).show() } }, LinearLayout.LayoutParams(0, -2, 1f)) }
            when (task.state) {
                TaskState.COMPLETED -> {
                    val media = TaskStore.get(this).outputs(task.id).ifEmpty { if (task.uri.isNotEmpty()) listOf(SavedMedia(task.uri, task.fileName, task.mime)) else emptyList() }
                    if (media.isNotEmpty()) {
                        action(R.string.open_file) {
                            if (media.size == 1) MediaActions.open(this, media.first())
                            else AlertDialog.Builder(this).setItems(media.map { it.name }.toTypedArray()) { _, index -> MediaActions.open(this, media[index]) }.show()
                        }
                        action(R.string.share_file) { MediaActions.share(this, media) }
                    }
                }
                TaskState.CANCELLED -> Unit
                TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK -> {
                    action(R.string.resume) { DownloadService.command(this, task.id, DownloadService.ACTION_RESUME) }
                    action(R.string.cancel) { DownloadService.command(this, task.id, DownloadService.ACTION_CANCEL) }
                }
                else -> {
                    action(R.string.pause) { DownloadService.command(this, task.id, DownloadService.ACTION_PAUSE) }
                    action(R.string.cancel) { DownloadService.command(this, task.id, DownloadService.ACTION_CANCEL) }
                }
            }
            if (history) action(R.string.remove) { TaskStore.get(this).remove(task.id); render(true) }
            card.addView(row)
            tasks.addView(card, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(12) })
        }
    }
    /** "12.4 MB / 80 MB · 2.1 MB/s · 0:32 left", from what the service has stored. */
    private fun stats(task: MobileTask): String {
        if (task.totalBytes <= 0 && task.bytesDone <= 0) return ""
        val parts = mutableListOf(if (task.totalBytes > 0) "${Format.bytes(task.bytesDone)} / ${Format.bytes(task.totalBytes)}" else Format.bytes(task.bytesDone))
        if (task.state == TaskState.DOWNLOADING) {
            val speed = meter.sample(task.id, task.bytesDone, android.os.SystemClock.elapsedRealtime())
            if (speed > 0) parts.add(Format.speed(speed))
            val eta = if (task.totalBytes > 0) Format.eta(task.totalBytes - task.bytesDone, speed) else ""
            if (eta.isNotEmpty()) parts.add(getString(R.string.eta_left, eta))
        }
        return parts.joinToString(" · ")
    }
    private fun forEach(states: Set<TaskState>, action: String) {
        TaskStore.get(this).list().filter { it.state in states }.forEach { runCatching { DownloadService.command(this, it.id, action) } }
    }
    /** Copied links are offered once, as a one-tap download; reading the clipboard needs window focus. */
    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus || !::banner.isInitialized) return
        banner.removeAllViews(); banner.visibility = android.view.View.GONE
        if (!MobilePreferences(this).watchClipboard) return
        val clip = (getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager).primaryClip
        val text = clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(this)?.toString().orEmpty()
        val urls = LinkPlan.parse(text).filter { LinkUtils.isPublicHttpUrl(it) }
        if (urls.isEmpty() || urls.joinToString("\n") == dismissedLink) return
        val key = urls.joinToString("\n")
        banner.visibility = android.view.View.VISIBLE
        banner.addView(label(if (urls.size == 1) getString(R.string.clipboard_found, LinkPlan.host(urls.first())) else getString(R.string.clipboard_many, urls.size), 14f))
        if (urls.any { Spotify.isTrackUrl(it) }) banner.addView(label(getString(R.string.spotify_note), 12f))
        val row = LinearLayout(this)
        row.addView(button(getString(R.string.download_now)) {
            dismissedLink = key; banner.visibility = android.view.View.GONE
            if (urls.size == 1) startActivity(Intent(this, ShareActivity::class.java).putExtra(Intent.EXTRA_TEXT, urls.first()))
            else addLinks(key)
        }, LinearLayout.LayoutParams(0, -2, 1f))
        row.addView(button(getString(R.string.dismiss)) { dismissedLink = key; banner.visibility = android.view.View.GONE }, LinearLayout.LayoutParams(0, -2, 1f))
        banner.addView(row)
    }
    /** Paste any number of links (or a `[1-20]` pattern); each one is routed to the right engine. */
    private fun addLinks(prefill: String = "") {
        val prefs = MobilePreferences(this)
        val form = column().apply { setPadding(dp(20), dp(8), dp(20), 0) }
        val input = EditText(this).apply { hint = getString(R.string.links_hint); minLines = 4; maxLines = 8; setText(prefill)
            inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE }
        val summary = label("", 13f)
        val audio = CheckBox(this).apply { text = getString(R.string.audio_only_all); isChecked = prefs.defaultAudio; setTextColor(ink) }
        form.addView(input); form.addView(summary); form.addView(audio); form.addView(label(getString(R.string.pattern_hint), 12f))
        form.addView(button(getString(R.string.paste_clipboard)) {
            val clip = (getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager).primaryClip
            val text = clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(this)?.toString().orEmpty()
            if (text.isNotBlank()) input.setText(if (input.text.isBlank()) text else input.text.toString() + "\n" + text)
        })
        val dialog = AlertDialog.Builder(this).setTitle(R.string.add_links).setView(ScrollView(this).apply { addView(form) })
            .setPositiveButton(R.string.download_now, null).setNeutralButton(R.string.file_download, null).setNegativeButton(R.string.cancel, null).create()
        fun links() = LinkPlan.parse(input.text.toString())
        input.doAfterTextChanged {
            val urls = links(); val counts = LinkPlan.summarize(urls)
            summary.text = if (urls.isEmpty()) "" else getString(R.string.links_summary, urls.size, counts.files, counts.media)
        }
        dialog.setOnShowListener {
            input.setText(input.text.toString())   // refresh the summary for a prefilled list
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                val urls = links()
                if (urls.isEmpty() || urls.any { !LinkUtils.isPublicHttpUrl(it) }) { input.error = getString(R.string.bad_link); return@setOnClickListener }
                if (urls.size == 1 && LinkPlan.classify(urls.first()) == LinkKind.MEDIA && !Spotify.isTrackUrl(urls.first()))
                    startActivity(Intent(this, ShareActivity::class.java).putExtra(Intent.EXTRA_TEXT, urls.first()))   // one video: choose quality
                else DownloadService.startMany(this, urls, prefs.defaultHeight, audio.isChecked)
                dialog.dismiss()
            }
            // "Direct file": treat every link as a plain file, whatever the address looks like.
            dialog.getButton(AlertDialog.BUTTON_NEUTRAL).setOnClickListener {
                val urls = links()
                if (urls.isEmpty() || urls.any { !LinkUtils.isPublicHttpUrl(it) }) { input.error = getString(R.string.bad_link); return@setOnClickListener }
                urls.forEach { DownloadService.start(this, it, null, false, "", "file") }
                dialog.dismiss()
            }
        }
        dialog.show()
    }
    private fun settings() {
        val prefs = MobilePreferences(this)
        val form = column().apply { setPadding(dp(20), 0, dp(20), dp(12)) }
        fun choices(title: String, entries: List<String>, selected: Int): Spinner {
            form.addView(label(title, 14f))
            return Spinner(this).also { it.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, entries); it.setSelection(selected.coerceAtLeast(0)); form.addView(it) }
        }
        fun toggle(key: Int, selected: Boolean) = CheckBox(this).apply { text = getString(key); isChecked = selected; setTextColor(ink); form.addView(this) }
        val network = choices(getString(R.string.network_setting), listOf(getString(R.string.network_any), getString(R.string.network_wifi), getString(R.string.network_unmetered)), prefs.networkPolicy.ordinal)
        val roaming = toggle(R.string.roaming_setting, prefs.allowRoaming)
        val quick = toggle(R.string.quick_setting, prefs.quickDownload)
        val watch = toggle(R.string.clipboard_setting, prefs.watchClipboard)
        val categories = toggle(R.string.category_setting, prefs.categoryFolders)
        val audio = toggle(R.string.default_audio, prefs.defaultAudio)
        val heights = listOf<Int?>(null, 1080, 720, 480)
        val quality = choices(getString(R.string.default_quality), heights.map { it?.let { height -> "${height}p" } ?: getString(R.string.best_quality) }, heights.indexOf(prefs.defaultHeight))
        form.addView(label(getString(R.string.parallel_setting), 14f))
        val parallel = EditText(this).apply { inputType = android.text.InputType.TYPE_CLASS_NUMBER; setText(prefs.concurrency.toString()); form.addView(this) }
        form.addView(label(getString(R.string.connections_setting), 14f))
        val connections = EditText(this).apply { inputType = android.text.InputType.TYPE_CLASS_NUMBER; setText(prefs.connections.toString()); form.addView(this) }
        form.addView(label(getString(R.string.speed_setting), 14f))
        val speed = EditText(this).apply { inputType = android.text.InputType.TYPE_CLASS_NUMBER; setText((prefs.speedLimit / 1024).toString()); form.addView(this) }
        val modes = listOf("system", "light", "dark")
        val mode = choices(getString(R.string.appearance), listOf(getString(R.string.theme_system), getString(R.string.theme_light), getString(R.string.theme_dark)), modes.indexOf(prefs.mode))
        val brands = listOf("midnight-arcane", "ember-forge", "forest-rune", "frost-byte")
        val brand = choices("Ratatoskr", listOf("Midnight Arcane", "Ember Forge", "Forest Rune", "Frost Byte"), brands.indexOf(prefs.brand))
        val languages = listOf("", "fa", "en")
        val language = choices("Language / زبان", listOf(getString(R.string.theme_system), "فارسی", "English"), languages.indexOf(prefs.language))
        val dialog = AlertDialog.Builder(this).setTitle(R.string.settings).setView(ScrollView(this).apply { addView(form) }).setPositiveButton(R.string.save_settings, null).setNegativeButton(R.string.cancel, null).create()
        dialog.setOnShowListener { dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
            val count = parallel.text.toString().toIntOrNull()
            val rate = speed.text.toString().toLongOrNull()
            val links = connections.text.toString().toIntOrNull()
            if (links == null || links !in 1..8) { connections.error = getString(R.string.settings_invalid); return@setOnClickListener }
            if (count == null || count !in 1..3 || rate == null || rate < 0 || rate > Long.MAX_VALUE / 1024) { speed.error = getString(R.string.settings_invalid); return@setOnClickListener }
            prefs.networkPolicy = NetworkPolicy.entries[network.selectedItemPosition]; prefs.allowRoaming = roaming.isChecked
            prefs.quickDownload = quick.isChecked; prefs.watchClipboard = watch.isChecked; prefs.categoryFolders = categories.isChecked; prefs.defaultAudio = audio.isChecked; prefs.defaultHeight = heights[quality.selectedItemPosition]
            prefs.concurrency = count; prefs.connections = links; prefs.speedLimit = rate * 1024; prefs.mode = modes[mode.selectedItemPosition]; prefs.brand = brands[brand.selectedItemPosition]
            prefs.language = languages[language.selectedItemPosition]
            androidx.appcompat.app.AppCompatDelegate.setApplicationLocales(androidx.core.os.LocaleListCompat.forLanguageTags(prefs.language))
            dialog.dismiss(); if (DownloadService.running) DownloadService.wake(this); recreate()
        } }
        dialog.show()
    }
    private fun about() {
        val box = column().apply { setPadding(dp(20), dp(12), dp(20), dp(12)) }
        val version = packageManager.getPackageInfo(packageName, 0).versionName.orEmpty()
        box.addView(label("Ratatoskr $version · GPL-3.0", 20f)); box.addView(label(getString(R.string.about_details), 14f))
        box.addView(button(getString(R.string.app_release)) { startActivity(Intent(Intent.ACTION_VIEW, android.net.Uri.parse("https://github.com/sajjadka21/ratatoskr/releases/latest"))) })
        lateinit var update: com.google.android.material.button.MaterialButton
        update = button(getString(R.string.update_engine)) {
            update.isEnabled = false; update.text = getString(R.string.updating)
            lifecycleScope.launch {
                val ok = runCatching { withContext(Dispatchers.IO) { Engine.update(this@MainActivity) } }.getOrDefault(false)
                update.isEnabled = true; update.text = getString(R.string.update_engine)
                Toast.makeText(this@MainActivity, if (ok) R.string.updated else R.string.update_failed, Toast.LENGTH_LONG).show()
            }
        }
        box.addView(update)
        AlertDialog.Builder(this).setView(ScrollView(this).apply { addView(box) }).setPositiveButton(android.R.string.ok, null).show()
    }
}
