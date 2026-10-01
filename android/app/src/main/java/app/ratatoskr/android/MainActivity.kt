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
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        history = savedInstanceState?.getBoolean("history") ?: false
        query = savedInstanceState?.getString("query").orEmpty()
        if (!DownloadService.running) TaskStore.get(this).recover()
        val box = column().apply { setBackgroundColor(paper) }
        insets(box)
        box.addView(label("Ratatoskr", 28f))
        box.addView(label(getString(R.string.main_hint), 14f))
        val actions = LinearLayout(this)
        actions.addView(button(getString(R.string.add_links)) { addLinks() }, LinearLayout.LayoutParams(0, -2, 1f))
        actions.addView(button(getString(R.string.paste)) { startActivity(Intent(this, ShareActivity::class.java).putExtra(ShareActivity.EXTRA_FROM_CLIPBOARD, true)) }, LinearLayout.LayoutParams(0, -2, 1f))
        box.addView(actions)
        val tabs = LinearLayout(this)
        tabs.addView(button(getString(R.string.active_jobs)) { history = false; render(true) }, LinearLayout.LayoutParams(0, -2, 1f))
        tabs.addView(button(getString(R.string.history)) { history = true; render(true) }, LinearLayout.LayoutParams(0, -2, 1f))
        box.addView(tabs)
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
        val next = list.toString()
        if (!force && next == signature) return
        signature = next; tasks.removeAllViews()
        if (list.isEmpty()) tasks.addView(label(getString(R.string.empty_jobs)))
        for (task in list) {
            val card = column().apply { setPadding(dp(12), dp(12), dp(12), dp(12)); setBackgroundColor(surface) }
            card.addView(label(task.title.ifEmpty { task.fileName.ifEmpty { getString(R.string.app_name) } }))
            card.addView(label(MobileText.state(this, task), 14f))
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
            card.addView(row)
            tasks.addView(card, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(12) })
        }
    }
    private fun addLinks() {
        val input = EditText(this).apply { hint = getString(R.string.links_hint); minLines = 3; maxLines = 6; inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE }
        val dialog = AlertDialog.Builder(this).setTitle(R.string.add_links).setView(input)
            .setPositiveButton(R.string.media_download, null).setNeutralButton(R.string.file_download, null).setNegativeButton(R.string.cancel, null).create()
        dialog.setOnShowListener {
            fun submit(file: Boolean) {
                val urls = LinkUtils.extractUrls(input.text.toString())
                if (urls.isEmpty() || urls.any { !LinkUtils.isPublicHttpUrl(it) }) { input.error = getString(R.string.bad_link); return }
                if (file) urls.forEach { DownloadService.start(this, it, null, false, "", "file") }
                else startActivity(Intent(this, ShareActivity::class.java).putExtra(Intent.EXTRA_TEXT, input.text.toString()))
                dialog.dismiss()
            }
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener { submit(false) }
            dialog.getButton(AlertDialog.BUTTON_NEUTRAL).setOnClickListener { submit(true) }
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
        val audio = toggle(R.string.default_audio, prefs.defaultAudio)
        val heights = listOf<Int?>(null, 1080, 720, 480)
        val quality = choices(getString(R.string.default_quality), heights.map { it?.let { height -> "${height}p" } ?: getString(R.string.best_quality) }, heights.indexOf(prefs.defaultHeight))
        form.addView(label(getString(R.string.parallel_setting), 14f))
        val parallel = EditText(this).apply { inputType = android.text.InputType.TYPE_CLASS_NUMBER; setText(prefs.concurrency.toString()); form.addView(this) }
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
            if (count == null || count !in 1..3 || rate == null || rate < 0 || rate > Long.MAX_VALUE / 1024) { speed.error = getString(R.string.settings_invalid); return@setOnClickListener }
            prefs.networkPolicy = NetworkPolicy.entries[network.selectedItemPosition]; prefs.allowRoaming = roaming.isChecked
            prefs.quickDownload = quick.isChecked; prefs.defaultAudio = audio.isChecked; prefs.defaultHeight = heights[quality.selectedItemPosition]
            prefs.concurrency = count; prefs.speedLimit = rate * 1024; prefs.mode = modes[mode.selectedItemPosition]; prefs.brand = brands[brand.selectedItemPosition]
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
