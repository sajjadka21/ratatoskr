package app.ratatoskr.android

import android.Manifest
import android.app.DatePickerDialog
import android.app.TimePickerDialog
import android.content.ClipData
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.*
import androidx.activity.OnBackPressedCallback
import androidx.compose.ui.platform.ComposeView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.systemBars
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.appcompat.app.AlertDialog
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.widget.doAfterTextChanged
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.button.MaterialButton
import com.google.android.material.button.MaterialButtonToggleGroup
import com.google.android.material.floatingactionbutton.FloatingActionButton
import kotlinx.coroutines.*
import java.text.DateFormat
import java.util.Calendar
import java.util.Date

/** AB presentation adapted to Ratatoskr's persistent native download journal. */
class MainActivity : MobileActivity(), TaskActions {
    private val selection = linkedSetOf<String>()
    private val leaveSelection = object : OnBackPressedCallback(false) { override fun handleOnBackPressed() { selection.clear(); render() } }
    private var homeState by mutableStateOf(AbHomeState())
    private var addPrefill by mutableStateOf<String?>(null)
    private var history = false
    private var category: String? = null
    private var sortOrder = 0
    private var formatFilter: String? = null
    private var dateFrom = 0L
    private var dateUntil = 0L
    private var query = ""
    private var dismissedLink = ""
    private val meter = SpeedMeter()
    private var folderLabel by mutableStateOf("")
    private val chooseDownloadFolder = registerForActivityResult(androidx.activity.result.contract.ActivityResultContracts.OpenDocumentTree()) { uri ->
        if (uri != null) runCatching {
            contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
            MobilePreferences(this).saveTree = uri.toString()
            folderLabel = uri.lastPathSegment?.substringAfterLast(':').orEmpty()
        }.onFailure { Toast.makeText(this, R.string.error_write, Toast.LENGTH_LONG).show() }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        history = savedInstanceState?.getBoolean("history") ?: false
        query = savedInstanceState?.getString("query").orEmpty()
        addPrefill = if (savedInstanceState != null) savedInstanceState.getString("add-prefill") else intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        formatFilter = savedInstanceState?.getString("format-filter")
        dateFrom = savedInstanceState?.getLong("date-from") ?: 0L
        dateUntil = savedInstanceState?.getLong("date-until") ?: 0L
        dismissedLink = savedInstanceState?.getString("dismissed-link").orEmpty()
        MobileRuntime.initialize(TaskStore.get(this))
        NetworkJobs.schedule(this)

        sortOrder = savedInstanceState?.getInt("sortOrder") ?: 0
        category = savedInstanceState?.getString("category")
        selection.addAll(savedInstanceState?.getStringArrayList("selection").orEmpty())
        onBackPressedDispatcher.addCallback(this, leaveSelection)
        setContentView(ComposeView(this).apply {
            contentDescription = getString(R.string.app_name)
            setContent {
                RatatoskrTheme(this@MainActivity) {
                    androidx.compose.material3.Surface(modifier = androidx.compose.ui.Modifier.fillMaxSize().background(androidx.compose.material3.MaterialTheme.colorScheme.background).windowInsetsPadding(WindowInsets.systemBars), color = androidx.compose.material3.MaterialTheme.colorScheme.background) {
                        AbHome(homeState, this@MainActivity,
                            onAdd = { addLinks() }, onQuery = { query = it; render() },
                            onHistory = { history = it; render() }, onMenu = ::menuAction,
                            onCategory = { category = it; render() },
                            categories = TaskFilter.present(TaskStore.get(this@MainActivity).list()).map { it to categoryName(it) },
                            onSort = { sortOrder = it; render() }, onFilter = ::showFilters,
                            onClipboard = { download -> val text = homeState.clipboard; dismissedLink = text; homeState = homeState.copy(clipboard = ""); if (download) addLinks(text) },
                            onSelection = { action -> when (action) {
                                R.string.cancel -> { selection.clear(); render() }
                                R.string.pause -> applyToSelection(DownloadService.ACTION_PAUSE)
                                R.string.resume -> applyToSelection(DownloadService.ACTION_RESUME)
                                R.string.remove -> removeSelection()
                            } })
                        addPrefill?.let { prefill -> AbEnterUrl(prefill, MobilePreferences(this@MainActivity).defaultAudio,
                            onClose = { addPrefill = null }, onPaste = ::clipboardText, onDownload = ::downloadLinks,
                            folder = folderLabel.ifEmpty { MobilePreferences(this@MainActivity).saveTree.takeIf { it.isNotEmpty() }?.let { android.net.Uri.parse(it).lastPathSegment?.substringAfterLast(':') }.orEmpty().ifEmpty { "Downloads/Ratatoskr" } },
                            onFolder = { chooseDownloadFolder.launch(null) }, onSubmit = ::submitLinks,
                            groups = TaskStore.get(this@MainActivity).list().map { it.groupName }.filter { it.isNotEmpty() }.distinct(),
                            previousTasks = TaskStore.get(this@MainActivity).list()) }
                    }
                }
            }
        }, android.view.ViewGroup.LayoutParams(-1, -1))
        render()
        lifecycleScope.launch { repeatOnLifecycle(Lifecycle.State.STARTED) { while (isActive) { render(); delay(750) } } }
        UpdateFlow.check(this, manual = false)
        welcome()
        if (android.os.Build.VERSION.SDK_INT < 29 && checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE) != PackageManager.PERMISSION_GRANTED)
            requestPermissions(arrayOf(Manifest.permission.WRITE_EXTERNAL_STORAGE), 2)
        if (android.os.Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED)
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putBoolean("history", history); outState.putString("query", query)
        outState.putString("format-filter", formatFilter); outState.putLong("date-from", dateFrom); outState.putLong("date-until", dateUntil)
        outState.putInt("sortOrder", sortOrder); outState.putString("category", category)
        outState.putString("add-prefill", addPrefill); outState.putString("dismissed-link", dismissedLink)
        outState.putStringArrayList("selection", ArrayList(selection)); super.onSaveInstanceState(outState)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()?.let { text ->
            addPrefill = text
        }
    }

    /** One short first-run explanation, with the one setting that decides whether downloads survive in the background. */
    private fun welcome() {
        val prefs = MobilePreferences(this)
        if (prefs.onboarded) return
        prefs.onboarded = true
        AlertDialog.Builder(this).setTitle(R.string.welcome_title).setMessage(R.string.welcome_body)
            .setPositiveButton(R.string.welcome_done, null)
            .setNeutralButton(R.string.battery_open) { _, _ -> Power.openSettings(this) }.show()
    }

    private fun menuAction(action: Int) {
        when (action) {
            R.string.browser -> startActivity(Intent(this, BrowserActivity::class.java))
            R.string.paste -> addLinks(clipboardText())
            R.string.pause_all -> forEach(TaskPolicy.inFlight + TaskState.QUEUED + TaskState.WAITING_NETWORK, DownloadService.ACTION_PAUSE)
            R.string.resume_all -> forEach(setOf(TaskState.SAVED, TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK), DownloadService.ACTION_RESUME)
            R.string.clear_finished -> { TaskStore.get(this).clearFinished(); render() }
            R.string.select_all -> { selection.addAll(homeState.rows.map { it.task.id }); render() }
            R.string.plugins -> startActivity(Intent(this, SettingsActivity::class.java).putExtra(SettingsActivity.EXTRA_PLUGINS, true))
            else -> startActivity(Intent(this, SettingsActivity::class.java))
        }
    }

    private fun render() {
        val all = TaskStore.get(this).list()
        val now = System.currentTimeMillis()
        val finished = setOf(TaskState.COMPLETED, TaskState.CANCELLED)
        val activeCount = all.count { it.state !in finished }
        val open = Schedule.now(MobilePreferences(this).window)
        selection.retainAll(all.map { it.id }.toSet())
        val shown = TaskQuery.apply(all.filter { !history || it.state in finished }, query, sortOrder, category, formatFilter, dateFrom, dateUntil)
        homeState = homeState.copy(rows = shown.map { task ->
            if (task.state !in TaskPolicy.inFlight) meter.forget(task.id)
            val label = when {
                task.startAt > now -> getString(R.string.scheduled_for, MobileDates.format(this, task.startAt))
                !open && task.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) -> getString(R.string.outside_window)
                else -> ""
            }
            TaskRow(task, stats(task), label, task.id in selection, Engine.cachedThumbnail(task.url))
        }, selecting = selection.isNotEmpty(), selectionCount = selection.size, history = history, query = query, category = category,
            summary = getString(R.string.summary_line, activeCount, all.size - activeCount), filtersActive = formatFilter != null || dateFrom > 0 || dateUntil > 0 || category != null)
        leaveSelection.isEnabled = selection.isNotEmpty()
    }

    override val selecting get() = selection.isNotEmpty()
    override fun toggle(task: MobileTask) { if (!selection.remove(task.id)) selection.add(task.id); render() }

    private fun selectedTasks() = TaskStore.get(this).list().filter { it.id in selection }
    private fun applyToSelection(action: String) {
        selectedTasks().forEach { runCatching { DownloadService.command(this, it.id, action) } }
        selection.clear(); render()
    }
    /** Finished downloads leave the list (their files stay); unfinished ones are cancelled first. */
    private fun removeSelection() {
        val chosen = selectedTasks()
        if (chosen.isEmpty()) return
        AlertDialog.Builder(this).setMessage(getString(R.string.remove_selected, chosen.size))
            .setPositiveButton(R.string.remove) { _, _ ->
                val store = TaskStore.get(this)
                chosen.forEach { if (it.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED, TaskState.FAILED)) store.remove(it.id) else runCatching { DownloadService.command(this, it.id, DownloadService.ACTION_CANCEL) } }
                selection.clear(); render()
            }.setNegativeButton(R.string.cancel, null).show()
    }

    private fun categoryName(value: String) = getString(when (value) {
        "Video" -> R.string.cat_video; "Music" -> R.string.cat_music; "Archives" -> R.string.cat_archives; "Programs" -> R.string.cat_programs
        "Documents" -> R.string.cat_documents; "Images" -> R.string.cat_images; else -> R.string.cat_other
    })

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

    // ---- list actions ------------------------------------------------------------------------------
    override fun command(task: MobileTask, action: String) {
        runCatching { DownloadService.command(this, task.id, action) }
            .onFailure { Toast.makeText(this, R.string.error_retry, Toast.LENGTH_LONG).show() }
        render()
    }
    private fun media(task: MobileTask): List<SavedMedia> =
        TaskStore.get(this).outputs(task.id).ifEmpty { if (task.uri.isNotEmpty()) listOf(SavedMedia(task.uri, task.fileName, task.mime)) else emptyList() }
    override fun open(task: MobileTask) {
        val files = media(task)
        when {
            files.isEmpty() -> Unit
            files.size == 1 -> MediaActions.open(this, files.first())
            else -> AlertDialog.Builder(this).setItems(files.map { it.name }.toTypedArray()) { _, index -> MediaActions.open(this, files[index]) }.show()
        }
    }
    override fun share(task: MobileTask) { media(task).takeIf { it.isNotEmpty() }?.let { MediaActions.share(this, it) } }
    override fun remove(task: MobileTask) { TaskStore.get(this).remove(task.id); render() }
    override fun schedule(task: MobileTask) {
        MobileDates.choose(this, task.startAt.takeIf { it > System.currentTimeMillis() } ?: System.currentTimeMillis() + 3_600_000) { chosen ->
            TaskStore.get(this).schedule(task.id, chosen)
            NetworkJobs.schedule(this)
            Toast.makeText(this, R.string.schedule_set, Toast.LENGTH_SHORT).show(); render()
        }
    }
    override fun group(name: String, start: Boolean) {
        TaskStore.get(this).list().filter { it.groupName == name && it.state !in setOf(TaskState.COMPLETED, TaskState.CANCELLED) }.forEach {
            command(it, if (start) DownloadService.ACTION_RESUME else DownloadService.ACTION_PAUSE)
        }
    }

    private fun showFilters() {
        val sheet = BottomSheetDialog(this)
        val box = column().apply { setPadding(dp(20), dp(12), dp(20), dp(24)); setBackgroundColor(surface) }
        box.addView(label(getString(R.string.filters_title), 22f))
        val name = EditText(this).apply { hint = getString(R.string.search_history); setText(query); setTextColor(ink); minHeight = dp(48); maxLines = 1 }
        box.addView(name)
        fun choices(label: Int, values: List<String>, selected: Int): Spinner {
            box.addView(this.label(getString(label), 13f))
            return Spinner(this).apply { adapter = ArrayAdapter(this@MainActivity, android.R.layout.simple_spinner_dropdown_item, values); setSelection(selected); minimumHeight = dp(48); box.addView(this) }
        }
        val sort = choices(R.string.sort_label, listOf(R.string.sort_newest, R.string.sort_oldest, R.string.sort_name, R.string.sort_size, R.string.sort_format).map { getString(it) }, sortOrder)
        val formats = TaskStore.get(this).list().map { TaskQuery.extension(it) }.filter { it.isNotEmpty() }.distinct().sorted()
        val format = choices(R.string.format_filter, listOf(getString(R.string.filter_all)) + formats.map { it.uppercase() }, formats.indexOf(formatFilter) + 1)
        val categories = TaskFilter.present(TaskStore.get(this).list())
        val categoryInput = choices(R.string.filter_category, listOf(getString(R.string.filter_all)) + categories.map { categoryName(it) }, categories.indexOf(category) + 1)
        var from = dateFrom; var until = dateUntil
        lateinit var fromButton: android.view.View; lateinit var untilButton: android.view.View
        fun fromLabel() = getString(R.string.date_from) + if (from > 0) ": " + MobileDates.format(this, from) else ""
        fun untilLabel() = getString(R.string.date_until) + if (until > 0) ": " + MobileDates.format(this, until - 1) else ""
        fromButton = button(fromLabel()) { MobileDates.choose(this, if (from > 0) from else System.currentTimeMillis(), dateOnly = true) { from = MobileDates.dayStart(it); (fromButton as TextView).text = fromLabel() } }
        untilButton = button(untilLabel()) { MobileDates.choose(this, if (until > 0) until - 1 else System.currentTimeMillis(), dateOnly = true) { until = MobileDates.nextDay(it); (untilButton as TextView).text = untilLabel() } }
        box.addView(fromButton); box.addView(untilButton)
        box.addView(button(getString(R.string.filter_apply)) {
            if (from > 0 && until > 0 && from >= until) { Toast.makeText(this, R.string.date_range_invalid, Toast.LENGTH_LONG).show(); return@button }
            query = name.text.toString(); sortOrder = sort.selectedItemPosition
            formatFilter = formats.getOrNull(format.selectedItemPosition - 1); category = categories.getOrNull(categoryInput.selectedItemPosition - 1)
            dateFrom = from; dateUntil = until; sheet.dismiss(); render()
        })
        box.addView(button(getString(R.string.filter_reset)) { query = ""; sortOrder = 0; category = null; formatFilter = null; dateFrom = 0; dateUntil = 0; sheet.dismiss(); render() })
        sheet.setContentView(ScrollView(this).apply { isVerticalScrollBarEnabled = false; addView(box) }); sheet.show()
    }

    /** For a link that expired: paste the fresh one and the download carries on from what is already saved. */
    private fun changeLink(task: MobileTask) {
        val input = EditText(this).apply { hint = getString(R.string.links_hint); setText(task.url); maxLines = 3 }
        val box = column().apply { setPadding(dp(20), dp(8), dp(20), 0); addView(input) }
        AlertDialog.Builder(this).setTitle(R.string.change_link).setView(box).setNegativeButton(R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val url = LinkPlan.parse(input.text.toString()).firstOrNull()
                if (url != null && TaskStore.get(this).updateUrl(task.id, url)) { DownloadService.wake(this); Toast.makeText(this, R.string.link_changed, Toast.LENGTH_SHORT).show() }
                else Toast.makeText(this, R.string.bad_link, Toast.LENGTH_LONG).show()
                render()
            }.show()
    }

    /** Compares the saved file with a hash the user pasted from the download page. */
    private fun verifyChecksum(task: MobileTask) {
        val file = media(task).firstOrNull() ?: return
        val input = EditText(this).apply { hint = getString(R.string.checksum_hint); maxLines = 2 }
        val box = column().apply { setPadding(dp(20), dp(8), dp(20), 0); addView(input) }
        AlertDialog.Builder(this).setTitle(R.string.verify_checksum).setView(box).setNegativeButton(R.string.cancel, null)
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val expected = Checksums.normalize(input.text.toString())
                val algorithm = Checksums.algorithm(expected)
                if (algorithm == null) { Toast.makeText(this, R.string.checksum_bad, Toast.LENGTH_LONG).show(); return@setPositiveButton }
                Toast.makeText(this, R.string.checksum_checking, Toast.LENGTH_SHORT).show()
                lifecycleScope.launch {
                    val actual = runCatching { withContext(Dispatchers.IO) { Checksums.compute(contentResolver.openInputStream(android.net.Uri.parse(file.uri)) ?: error("unreadable"), algorithm) } }.getOrNull()
                    val text = when { actual == null -> getString(R.string.checksum_unreadable); actual == expected -> getString(R.string.checksum_match, algorithm); else -> getString(R.string.checksum_mismatch, algorithm) }
                    AlertDialog.Builder(this@MainActivity).setMessage(text).setPositiveButton(android.R.string.ok, null).show()
                }
            }.show()
    }

    /** Everything about one download, with the actions that do not fit on the card. */
    override fun details(task: MobileTask) {
        val sheet = BottomSheetDialog(this)
        val box = column().apply { setPadding(dp(20), dp(16), dp(20), dp(24)); setBackgroundColor(surface) }
        box.addView(label(task.title.ifEmpty { task.fileName.ifEmpty { LinkPlan.host(task.url) } }, 18f))
        fun row(name: Int, value: String) { if (value.isNotBlank()) box.addView(label("${getString(name)}: $value", 13f)) }
        row(R.string.detail_status, MobileText.state(this, task))
        row(R.string.detail_size, if (task.totalBytes > 0) Format.bytes(task.totalBytes) else if (task.bytesDone > 0) Format.bytes(task.bytesDone) else "")
        row(R.string.detail_source, LinkPlan.host(task.url))
        row(R.string.detail_file, task.fileName)
        row(R.string.detail_added, MobileDates.format(this, task.createdAt))
        if (task.startAt > System.currentTimeMillis()) row(R.string.detail_starts, MobileDates.format(this, task.startAt))
        if (task.error.isNotEmpty()) row(R.string.detail_problem, MobileText.error(this, task.error))
        val actions = column().apply { setPadding(0, dp(8), 0, 0) }
        fun act(text: Int, block: () -> Unit) = actions.addView(button(getString(text)) { sheet.dismiss(); block() }, LinearLayout.LayoutParams(-1, -2))
        when (task.state) {
            TaskState.COMPLETED -> { act(R.string.open_file) { open(task) }; act(R.string.share_file) { share(task) } }
            TaskState.CANCELLED -> Unit
            TaskState.SAVED, TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK -> act(R.string.resume) { command(task, DownloadService.ACTION_RESUME) }
            else -> act(R.string.pause) { command(task, DownloadService.ACTION_PAUSE) }
        }
        if (task.state !in setOf(TaskState.COMPLETED, TaskState.CANCELLED)) act(R.string.cancel) { command(task, DownloadService.ACTION_CANCEL) }
        if (task.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED, TaskState.FAILED)) act(R.string.remove) { remove(task) }
        act(R.string.copy_link) {
            (getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager).setPrimaryClip(ClipData.newPlainText("link", task.url))
            Toast.makeText(this, R.string.link_copied, Toast.LENGTH_SHORT).show()
        }
        if (task.state == TaskState.COMPLETED && media(task).isNotEmpty()) act(R.string.verify_checksum) { verifyChecksum(task) }
        if (task.state !in setOf(TaskState.COMPLETED, TaskState.CANCELLED) && task.state !in TaskPolicy.inFlight) {
            act(R.string.change_link) { changeLink(task) }
            act(R.string.schedule) { schedule(task) }
            if (task.startAt > 0) act(R.string.schedule_clear) { TaskStore.get(this).schedule(task.id, 0); DownloadService.wake(this); render() }
        }
        box.addView(actions)
        sheet.setContentView(ScrollView(this).apply { addView(box) })
        sheet.show()
    }

    private fun clipboardText(): String {
        val clip = (getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager).primaryClip
        return clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(this)?.toString().orEmpty()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus || !MobilePreferences(this).watchClipboard) return
        val urls = LinkPlan.parse(clipboardText()).filter { LinkUtils.isPublicHttpUrl(it) }
        val text = urls.joinToString("\n")
        if (text.isNotEmpty() && text != dismissedLink) homeState = homeState.copy(clipboard = text)
    }

    private fun addLinks(prefill: String = "") { addPrefill = prefill }

    private fun showAllDownloads() {
        history = false; query = ""; category = null; formatFilter = null; dateFrom = 0; dateUntil = 0; sortOrder = 0; selection.clear()
    }

    private fun downloadLinks(text: String, audio: Boolean): Boolean = submitLinks(text, audio, IntakeOptions())

    private fun submitLinks(text: String, audio: Boolean, options: IntakeOptions): Boolean {
        val urls = LinkPlan.parse(text)
        if (urls.isEmpty() || urls.any { !LinkUtils.isPublicHttpUrl(it) } || runCatching { options.validate() }.isFailure) return false
        val identities = urls.map { LinkUtils.contentIdentity(it) }.toSet()
        val previous = TaskStore.get(this).list().filter { LinkUtils.contentIdentity(it.url) in identities }.sortedByDescending { it.createdAt }
        if (previous.isNotEmpty() && !options.allowDuplicate) {
            val old = previous.first()
            val message = getString(if (previous.any { it.state == TaskState.COMPLETED }) R.string.duplicate_done else R.string.duplicate_pending) +
                "\n\n" + previous.take(3).joinToString("\n") { TaskQuery.name(it) + " · " + MobileText.state(this, it) + " · " + MobileDates.format(this, it.createdAt) } +
                "\n\n" + getString(R.string.duplicate_warning)
            addPrefill = null
            AlertDialog.Builder(this).setTitle(R.string.duplicate_title).setMessage(message)
                .setPositiveButton(R.string.duplicate_again) { _, _ -> submitLinks(text, audio, options.copy(allowDuplicate = true)) }
                .setNeutralButton(R.string.duplicate_view) { _, _ -> showAllDownloads(); render(); details(old) }
                .setNegativeButton(R.string.cancel) { _, _ -> addPrefill = text }.show()
            return true
        }
        val prefs = MobilePreferences(this)
        val result = runCatching { DownloadService.startMany(this, urls, prefs.defaultHeight, audio, options) }
        if (result.isFailure) { Toast.makeText(this, R.string.error_retry, Toast.LENGTH_LONG).show(); return false }
        addPrefill = null; showAllDownloads(); render()
        Toast.makeText(this, if (options.initialState == TaskState.SAVED) R.string.add_saved_feedback else R.string.add_started_feedback, Toast.LENGTH_LONG).show()
        return true
    }
}
