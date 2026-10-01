package app.ratatoskr.android

import android.app.TimePickerDialog
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.*
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatDelegate
import androidx.core.os.LocaleListCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.google.android.material.button.MaterialButton
import com.google.android.material.switchmaterial.SwitchMaterial

/** All settings on one scrolling page. Every control saves the moment it changes; there is no Save button. */
class SettingsActivity : MobileActivity() {
    private lateinit var prefs: MobilePreferences
    private val pickPlugin = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri -> if (uri != null) importPlugin(uri) }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefs = MobilePreferences(this)
        val root = ScrollView(this).apply { setBackgroundColor(paper); clipToPadding = false }
        val page = column()
        root.addView(page)
        ViewCompat.setOnApplyWindowInsetsListener(root) { _, inset ->
            val bars = inset.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.ime())
            page.setPadding(dp(16) + bars.left, dp(8) + bars.top, dp(16) + bars.right, dp(24) + bars.bottom)
            inset
        }
        val top = LinearLayout(this).apply { gravity = Gravity.CENTER_VERTICAL }
        top.addView(icon(R.drawable.ic_close, getString(R.string.cancel)) { finish() })
        top.addView(TextView(this).apply { text = getString(R.string.settings); textSize = 22f; setTextColor(ink); typeface = android.graphics.Typeface.DEFAULT_BOLD; setPadding(dp(8), 0, 0, 0) })
        page.addView(top)

        // --- network and time
        page.addView(section(R.string.section_network))
        page.addView(card {
            choice(R.string.network_setting, listOf(R.string.network_any, R.string.network_wifi, R.string.network_unmetered).map { getString(it) }, prefs.networkPolicy.ordinal) { prefs.networkPolicy = NetworkPolicy.entries[it]; NetworkJobs.schedule(this@SettingsActivity); wake() }
            toggle(R.string.roaming_setting, prefs.allowRoaming) { prefs.allowRoaming = it; wake() }
            windowRows()
        })

        // --- downloads
        page.addView(section(R.string.section_downloads))
        page.addView(card {
            slider(R.string.connections_value, 1, 8, prefs.connections) { prefs.connections = it }
            slider(R.string.parallel_value, 1, 3, prefs.concurrency) { prefs.concurrency = it; wake() }
            val limits = listOf(0L, 256L, 512L, 1024L, 2048L, 5120L)
            choice(R.string.speed_setting, limits.map { if (it == 0L) getString(R.string.speed_unlimited) else Format.speed(it * 1024) }, limits.indexOf(prefs.speedLimit / 1024).let { if (it < 0) 0 else it }) { prefs.speedLimit = limits[it] * 1024 }
            toggle(R.string.category_setting, prefs.categoryFolders) { prefs.categoryFolders = it }
        })

        // --- adding links
        page.addView(section(R.string.section_intake))
        page.addView(card {
            toggle(R.string.clipboard_setting, prefs.watchClipboard) { prefs.watchClipboard = it }
            toggle(R.string.quick_setting, prefs.quickDownload) { prefs.quickDownload = it }
            toggle(R.string.default_audio, prefs.defaultAudio) { prefs.defaultAudio = it }
            val heights = listOf<Int?>(null, 1080, 720, 480)
            choice(R.string.default_quality, heights.map { it?.let { height -> "${height}p" } ?: getString(R.string.best_quality) }, heights.indexOf(prefs.defaultHeight).coerceAtLeast(0)) { prefs.defaultHeight = heights[it] }
        })

        // --- appearance
        page.addView(section(R.string.section_appearance))
        page.addView(card {
            val modes = listOf("system", "light", "dark")
            choice(R.string.appearance, listOf(R.string.theme_system, R.string.theme_light, R.string.theme_dark).map { getString(it) }, modes.indexOf(prefs.mode).coerceAtLeast(0)) { prefs.mode = modes[it]; recreate() }
            val brands = listOf("midnight-arcane", "ember-forge", "forest-rune", "frost-byte")
            choice(R.string.app_name, listOf("Midnight Arcane", "Ember Forge", "Forest Rune", "Frost Byte"), brands.indexOf(prefs.brand).coerceAtLeast(0)) { prefs.brand = brands[it]; recreate() }
            val languages = listOf("", "fa", "en")
            choice(R.string.language, listOf(getString(R.string.theme_system), "فارسی", "English"), languages.indexOf(prefs.language).coerceAtLeast(0)) {
                prefs.language = languages[it]; AppCompatDelegate.setApplicationLocales(LocaleListCompat.forLanguageTags(prefs.language))
            }
        })

        // --- updates and plugins
        page.addView(section(R.string.section_updates))
        page.addView(card {
            toggle(R.string.auto_update_setting, prefs.autoUpdateCheck) { prefs.autoUpdateCheck = it }
            val version = packageManager.getPackageInfo(packageName, 0).versionName.orEmpty()
            addView(label(getString(R.string.version_line, version) + " · GPL-3.0", 13f))
            addView(button(getString(R.string.check_app_update)) { UpdateFlow.check(this@SettingsActivity, manual = true) })
            lateinit var engine: MaterialButton
            engine = button(getString(R.string.update_engine)) { updateEngine(engine) }
            addView(engine)
            addView(button(getString(R.string.open_plugins)) { plugins() })
            addView(button(getString(R.string.app_release)) { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(AppUpdate.RELEASES_PAGE))) })
        })
        page.addView(label(getString(R.string.about_details), 12f).apply { setTextColor(muted) })
        setContentView(root)
        if (intent.getBooleanExtra(EXTRA_PLUGINS, false)) plugins()
    }

    private fun wake() { if (DownloadService.running) DownloadService.wake(this) }

    // ---- building blocks ----------------------------------------------------------------------------
    private fun section(title: Int) = TextView(this).apply {
        text = getString(title); textSize = 13f; setTextColor(accent); typeface = android.graphics.Typeface.DEFAULT_BOLD
        setPadding(dp(4), dp(20), 0, dp(6))
    }
    private fun card(build: LinearLayout.() -> Unit) = column().apply {
        setPadding(dp(14), dp(6), dp(14), dp(10)); background = rounded(surface, 18, accent and 0x33FFFFFF); build()
    }
    private fun LinearLayout.toggle(text: Int, checked: Boolean, onChange: (Boolean) -> Unit) = addView(SwitchMaterial(this@SettingsActivity).apply {
        this.text = getString(text); isChecked = checked; setTextColor(ink); setPadding(0, dp(10), 0, dp(10))
        thumbTintList = android.content.res.ColorStateList.valueOf(accent); trackTintList = android.content.res.ColorStateList.valueOf(accent and 0x66FFFFFF)
        setOnCheckedChangeListener { _, value -> onChange(value) }
    })
    private fun LinearLayout.choice(title: Int, entries: List<String>, selected: Int, onPick: (Int) -> Unit) {
        addView(label(getString(title), 14f))
        addView(Spinner(this@SettingsActivity).apply {
            adapter = ArrayAdapter(this@SettingsActivity, android.R.layout.simple_spinner_dropdown_item, entries); setSelection(selected)
            onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
                private var first = true   // the first callback is the initial selection, not a change
                override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) { if (first) first = false else onPick(position) }
                override fun onNothingSelected(parent: AdapterView<*>?) = Unit
            }
        })
    }
    private fun LinearLayout.slider(text: Int, min: Int, max: Int, value: Int, onChange: (Int) -> Unit) {
        val caption = label(getString(text, value), 14f)
        addView(caption)
        addView(SeekBar(this@SettingsActivity).apply {
            this.max = max - min; progress = value - min
            progressTintList = android.content.res.ColorStateList.valueOf(accent); thumbTintList = android.content.res.ColorStateList.valueOf(accent)
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(bar: SeekBar?, progress: Int, fromUser: Boolean) { caption.text = getString(text, progress + min); if (fromUser) onChange(progress + min) }
                override fun onStartTrackingTouch(bar: SeekBar?) = Unit
                override fun onStopTrackingTouch(bar: SeekBar?) = Unit
            })
        })
    }

    /** "Only download between 02:00 and 07:00", like the schedulers of desktop download managers. */
    private fun LinearLayout.windowRows() {
        val times = LinearLayout(this@SettingsActivity)
        lateinit var from: MaterialButton
        lateinit var until: MaterialButton
        fun refresh() {
            val window = prefs.window
            from.text = getString(R.string.window_from) + " " + Schedule.clock(window.startMinute)
            until.text = getString(R.string.window_to) + " " + Schedule.clock(window.endMinute)
            times.visibility = if (window.enabled) View.VISIBLE else View.GONE
        }
        fun pick(start: Boolean) {
            val window = prefs.window
            val minute = if (start) window.startMinute else window.endMinute
            TimePickerDialog(this@SettingsActivity, { _, hour, min ->
                prefs.window = if (start) window.copy(startMinute = hour * 60 + min) else window.copy(endMinute = hour * 60 + min)
                NetworkJobs.schedule(this@SettingsActivity); wake(); refresh()
            }, minute / 60, minute % 60, android.text.format.DateFormat.is24HourFormat(this@SettingsActivity)).show()
        }
        toggle(R.string.window_setting, prefs.window.enabled) { prefs.window = prefs.window.copy(enabled = it); NetworkJobs.schedule(this@SettingsActivity); wake(); refresh() }
        from = button("") { pick(true) }; until = button("") { pick(false) }
        times.addView(from, LinearLayout.LayoutParams(0, -2, 1f)); times.addView(until, LinearLayout.LayoutParams(0, -2, 1f))
        addView(times)
        addView(label(getString(R.string.window_hint), 12f).apply { setTextColor(muted) })
        refresh()
    }

    // ---- actions -------------------------------------------------------------------------------------
    private fun updateEngine(button: MaterialButton) {
        button.isEnabled = false; button.text = getString(R.string.updating)
        Thread {
            val ok = runCatching { Engine.update(this) }.getOrDefault(false)
            runOnUiThread {
                button.isEnabled = true; button.text = getString(R.string.update_engine)
                Toast.makeText(this, if (ok) R.string.updated else R.string.update_failed, Toast.LENGTH_LONG).show()
            }
        }.start()
    }

    private fun importPlugin(uri: Uri) {
        val ok = runCatching {
            val text = contentResolver.openInputStream(uri)?.use { LinkUtils.readText(it, Plugins.MAX_BYTES + 1) } ?: error("unreadable")
            PluginStore(this).import(text)
        }.isSuccess
        Toast.makeText(this, if (ok) R.string.plugin_added else R.string.plugin_invalid, Toast.LENGTH_LONG).show()
        if (ok) plugins()
    }

    private fun plugins() {
        val store = PluginStore(this)
        val box = column().apply { setPadding(dp(20), dp(12), dp(20), dp(12)) }
        box.addView(label(getString(R.string.plugins_hint), 13f))
        val installed = store.all()
        if (installed.isEmpty()) box.addView(label(getString(R.string.plugins_none)))
        lateinit var dialog: AlertDialog
        for (plugin in installed) {
            val row = LinearLayout(this).apply { gravity = Gravity.CENTER_VERTICAL }
            row.addView(CheckBox(this).apply {
                text = "${plugin.name} · ${plugin.version.ifEmpty { "-" }}"; isChecked = store.enabled(plugin.id); setTextColor(ink)
                setOnCheckedChangeListener { _, on -> store.setEnabled(plugin.id, on) }
            }, LinearLayout.LayoutParams(0, -2, 1f))
            row.addView(icon(R.drawable.ic_delete, getString(R.string.plugin_remove), danger) { store.remove(plugin.id); dialog.dismiss(); plugins() })
            box.addView(row)
        }
        box.addView(button(getString(R.string.plugins_add)) { dialog.dismiss(); pickPlugin.launch(arrayOf("application/json", "text/plain", "application/octet-stream")) })
        dialog = AlertDialog.Builder(this).setTitle(R.string.plugins).setView(ScrollView(this).apply { addView(box) }).setPositiveButton(android.R.string.ok, null).create()
        dialog.show()
    }

    companion object { const val EXTRA_PLUGINS = "open_plugins" }
}
