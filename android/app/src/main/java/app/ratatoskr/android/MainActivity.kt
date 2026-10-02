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
    private var query = ""
    private var dismissedLink = ""
    private val meter = SpeedMeter()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        history = savedInstanceState?.getBoolean("history") ?: false
        query = savedInstanceState?.getString("query").orEmpty()
        addPrefill = savedInstanceState?.getString("add-prefill")
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
                    androidx.compose.material3.Surface(modifier = androidx.compose.ui.Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.systemBars), color = androidx.compose.material3.MaterialTheme.colorScheme.background) {
                        AbHome(homeState, this@MainActivity,
                            onAdd = { addLinks() }, onQuery = { query = it; render() },
                            onHistory = { history = it; render() }, onMenu = ::menuAction,
                            onCategory = { category = it; render() },
                            categories = TaskFilter.present(TaskStore.get(this@MainActivity).list()).map { it to categoryName(it) },
                            onSort = { sortOrder = it; render() },
                            onClipboard = { download -> val text = homeState.clipboard; dismissedLink = text; homeState = homeState.copy(clipboard = ""); if (download) addLinks(text) },
                            onSelection = { action -> when (action) {
                                R.string.cancel -> { selection.clear(); render() }
                                R.string.pause -> applyToSelection(DownloadService.ACTION_PAUSE)
                                R.string.resume -> applyToSelection(DownloadService.ACTION_RESUME)
                                R.string.remove -> removeSelection()
                            } })
                        addPrefill?.let { prefill -> AbEnterUrl(prefill, MobilePreferences(this@MainActivity).defaultAudio,
                            onClose = { addPrefill = null }, onPaste = ::clipboardText, onDownload = ::downloadLinks) }
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
        outState.putInt("sortOrder", sortOrder); outState.putString("category", category)
        outState.putString("add-prefill", addPrefill); outState.putString("dismissed-link", dismissedLink)
        outState.putStringArrayList("selection", ArrayList(selection)); super.onSaveInstanceState(outState)
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
            R.string.resume_all -> forEach(setOf(TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK), DownloadService.ACTION_RESUME)
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
        val shown = all.filter {
            (!history || it.state in finished) && (query.isBlank() || it.title.contains(query, true) || it.fileName.contains(query, true)) && TaskFilter.matches(it, category)
        }.let { items -> when (sortOrder) {
            1 -> items.sortedBy { it.createdAt }
            2 -> items.sortedBy { it.title.lowercase(java.util.Locale.ROOT) }
            3 -> items.sortedByDescending { it.totalBytes }
            else -> items.sortedByDescending { it.createdAt }
        } }
        homeState = homeState.copy(rows = shown.map { task ->
            if (task.state !in TaskPolicy.inFlight) meter.forget(task.id)
            val label = when {
                task.startAt > now -> getString(R.string.scheduled_for, DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(task.startAt)))
                !open && task.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) -> getString(R.string.outside_window)
                else -> ""
            }
            TaskRow(task, stats(task), label, task.id in selection)
        }, selecting = selection.isNotEmpty(), selectionCount = selection.size, history = history, query = query, category = category,
            summary = getString(R.string.summary_line, activeCount, all.size - activeCount))
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
        val now = Calendar.getInstance()
        DatePickerDialog(this, { _, year, month, day ->
            TimePickerDialog(this, { _, hour, minute ->
                val chosen = Calendar.getInstance().apply { set(year, month, day, hour, minute, 0); set(Calendar.MILLISECOND, 0) }
                TaskStore.get(this).schedule(task.id, chosen.timeInMillis)
                NetworkJobs.schedule(this)
                Toast.makeText(this, R.string.schedule_set, Toast.LENGTH_SHORT).show(); render()
            }, now.get(Calendar.HOUR_OF_DAY), now.get(Calendar.MINUTE), android.text.format.DateFormat.is24HourFormat(this)).show()
        }, now.get(Calendar.YEAR), now.get(Calendar.MONTH), now.get(Calendar.DAY_OF_MONTH)).show()
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
        row(R.string.detail_added, DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(task.createdAt)))
        if (task.startAt > System.currentTimeMillis()) row(R.string.detail_starts, DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(task.startAt)))
        if (task.error.isNotEmpty()) row(R.string.detail_problem, MobileText.error(this, task.error))
        val actions = column().apply { setPadding(0, dp(8), 0, 0) }
        fun act(text: Int, block: () -> Unit) = actions.addView(button(getString(text)) { sheet.dismiss(); block() }, LinearLayout.LayoutParams(-1, -2))
        when (task.state) {
            TaskState.COMPLETED -> { act(R.string.open_file) { open(task) }; act(R.string.share_file) { share(task) } }
            TaskState.CANCELLED -> Unit
            TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK -> act(R.string.resume) { command(task, DownloadService.ACTION_RESUME) }
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

    private fun downloadLinks(text: String, audio: Boolean): Boolean {
        val urls = LinkPlan.parse(text)
        if (urls.isEmpty() || urls.any { !LinkUtils.isPublicHttpUrl(it) }) return false
        val prefs = MobilePreferences(this)
        if (urls.size == 1 && LinkPlan.classify(urls.first()) == LinkKind.MEDIA && !Spotify.isTrackUrl(urls.first()) && !audio && !prefs.quickDownload)
            startActivity(Intent(this, ShareActivity::class.java).putExtra(Intent.EXTRA_TEXT, urls.first()))
        else DownloadService.startMany(this, urls, prefs.defaultHeight, audio)
        addPrefill = null
        render()
        return true
    }
}
