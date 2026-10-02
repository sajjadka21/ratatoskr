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
import com.google.android.material.floatingactionbutton.ExtendedFloatingActionButton
import kotlinx.coroutines.*
import java.text.DateFormat
import java.util.Calendar
import java.util.Date

/** The home screen: a header, Active/History tabs, the list of downloads and a button to add links. */
class MainActivity : MobileActivity(), TaskActions {
    private lateinit var adapter: TaskAdapter
    private lateinit var banner: LinearLayout
    private lateinit var header: LinearLayout
    private lateinit var selectionBar: LinearLayout
    private lateinit var selectionTitle: TextView
    private val selection = linkedSetOf<String>()
    private val leaveSelection = object : OnBackPressedCallback(false) { override fun handleOnBackPressed() { selection.clear(); render() } }
    private lateinit var empty: LinearLayout
    private lateinit var summary: TextView
    private lateinit var activeTab: MaterialButton
    private lateinit var historyTab: MaterialButton
    private var history = false
    private var category: String? = null
    private var filterKey = ""
    private lateinit var filters: LinearLayout
    private lateinit var filterScroll: HorizontalScrollView
    private var query = ""
    private var dismissedLink = ""
    private val meter = SpeedMeter()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        history = savedInstanceState?.getBoolean("history") ?: false
        query = savedInstanceState?.getString("query").orEmpty()
        MobileRuntime.initialize(TaskStore.get(this))
        NetworkJobs.schedule(this)

        val root = FrameLayout(this).apply { setBackgroundColor(paper) }
        val content = column()
        // header: icon, title, summary, menu
        header = LinearLayout(this).apply { gravity = Gravity.CENTER_VERTICAL; setPadding(0, dp(4), 0, dp(8)) }
        header.addView(ImageView(this).apply { setImageResource(R.mipmap.ic_launcher); layoutParams = LinearLayout.LayoutParams(dp(40), dp(40)).apply { marginEnd = dp(12) } })
        val titles = column().apply { layoutParams = LinearLayout.LayoutParams(0, -2, 1f) }
        titles.addView(TextView(this).apply { text = "Ratatoskr"; textSize = 22f; setTextColor(ink); typeface = android.graphics.Typeface.DEFAULT_BOLD })
        summary = TextView(this).apply { textSize = 12f; setTextColor(muted) }
        titles.addView(summary)
        header.addView(titles)
        header.addView(icon(R.drawable.ic_paste, getString(R.string.paste)) { startActivity(Intent(this, ShareActivity::class.java).putExtra(ShareActivity.EXTRA_FROM_CLIPBOARD, true)) })
        lateinit var more: ImageButton
        more = icon(R.drawable.ic_more, getString(R.string.menu)) { menu(more) }
        header.addView(more)
        content.addView(header)
        // shown instead of the header while downloads are selected
        selectionBar = LinearLayout(this).apply { gravity = Gravity.CENTER_VERTICAL; setPadding(0, dp(4), 0, dp(8)); visibility = View.GONE }
        selectionBar.addView(icon(R.drawable.ic_close, getString(R.string.cancel)) { selection.clear(); render() })
        selectionTitle = TextView(this).apply { textSize = 17f; setTextColor(ink); layoutParams = LinearLayout.LayoutParams(0, -2, 1f); setPadding(dp(8), 0, 0, 0) }
        selectionBar.addView(selectionTitle)
        selectionBar.addView(button(getString(R.string.select_all)) { selection.addAll(adapter.currentList.map { it.task.id }); render() })
        selectionBar.addView(icon(R.drawable.ic_pause, getString(R.string.pause)) { applyToSelection(DownloadService.ACTION_PAUSE) })
        selectionBar.addView(icon(R.drawable.ic_retry, getString(R.string.resume)) { applyToSelection(DownloadService.ACTION_RESUME) })
        selectionBar.addView(icon(R.drawable.ic_delete, getString(R.string.remove), danger) { removeSelection() })
        content.addView(selectionBar)
        onBackPressedDispatcher.addCallback(this, leaveSelection)
        banner = column().apply { visibility = View.GONE }
        content.addView(banner)

        val group = MaterialButtonToggleGroup(this).apply { isSingleSelection = true; isSelectionRequired = true }
        fun tab() = MaterialButton(this, null, com.google.android.material.R.attr.materialButtonOutlinedStyle).apply {
            id = View.generateViewId(); setTextColor(accent); layoutParams = LinearLayout.LayoutParams(0, dp(44), 1f)
        }
        activeTab = tab(); historyTab = tab()
        group.addView(activeTab); group.addView(historyTab)
        group.check(if (history) historyTab.id else activeTab.id)
        group.addOnButtonCheckedListener { _, id, checked -> if (checked) { history = id == historyTab.id; render() } }
        content.addView(group, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(8) })
        content.addView(EditText(this).apply {
            hint = getString(R.string.search_history); setTextColor(ink); setHintTextColor(muted); setText(query); maxLines = 1; inputType = android.text.InputType.TYPE_CLASS_TEXT
            background = rounded(surface, 14); setPadding(dp(14), dp(10), dp(14), dp(10))
            doAfterTextChanged { query = it.toString(); render() }
        }, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(10) })

        filters = LinearLayout(this)
        filterScroll = HorizontalScrollView(this).apply { isHorizontalScrollBarEnabled = false; visibility = View.GONE; addView(filters) }
        content.addView(filterScroll, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(10) })

        adapter = TaskAdapter(this, this)
        val list = RecyclerView(this).apply {
            layoutManager = LinearLayoutManager(this@MainActivity); adapter = this@MainActivity.adapter
            itemAnimator = null; clipToPadding = false; setPadding(0, 0, 0, dp(96)); overScrollMode = View.OVER_SCROLL_NEVER
        }
        empty = column().apply {
            gravity = Gravity.CENTER; visibility = View.GONE
            addView(ImageView(this@MainActivity).apply { setImageResource(R.mipmap.ic_launcher); alpha = 0.85f; layoutParams = LinearLayout.LayoutParams(dp(96), dp(96)) })
            addView(TextView(this@MainActivity).apply { text = getString(R.string.empty_jobs); textSize = 14f; gravity = Gravity.CENTER; setTextColor(muted); setPadding(dp(32), dp(12), dp(32), 0) })
        }
        val body = FrameLayout(this)
        body.addView(list, FrameLayout.LayoutParams(-1, -1)); body.addView(empty, FrameLayout.LayoutParams(-1, -1))
        content.addView(body, LinearLayout.LayoutParams(-1, 0, 1f))
        root.addView(content, FrameLayout.LayoutParams(-1, -1))

        val fab = ExtendedFloatingActionButton(this).apply {
            text = getString(R.string.add_links); setIconResource(R.drawable.ic_add)
            backgroundTintList = android.content.res.ColorStateList.valueOf(accent); setTextColor(paper); iconTint = android.content.res.ColorStateList.valueOf(paper)
            setOnClickListener { addLinks() }
        }
        val fabParams = FrameLayout.LayoutParams(-2, -2, Gravity.BOTTOM or Gravity.END)
        root.addView(fab, fabParams)
        ViewCompat.setOnApplyWindowInsetsListener(root) { _, inset ->
            val bars = inset.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.ime())
            content.setPadding(dp(16) + bars.left, dp(8) + bars.top, dp(16) + bars.right, 0)
            fabParams.setMargins(dp(16), dp(16), dp(16), dp(16) + bars.bottom); fab.layoutParams = fabParams
            inset
        }
        setContentView(root)
        lifecycleScope.launch { repeatOnLifecycle(Lifecycle.State.STARTED) { while (isActive) { render(); delay(750) } } }
        UpdateFlow.check(this, manual = false)
        welcome()
        if (android.os.Build.VERSION.SDK_INT < 29 && checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE) != PackageManager.PERMISSION_GRANTED)
            requestPermissions(arrayOf(Manifest.permission.WRITE_EXTERNAL_STORAGE), 2)
        if (android.os.Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED)
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putBoolean("history", history); outState.putString("query", query); super.onSaveInstanceState(outState)
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

    private fun menu(anchor: View) {
        val popup = PopupMenu(this, anchor)
        val items = listOf(R.string.browser, R.string.pause_all, R.string.resume_all, R.string.clear_finished, R.string.plugins, R.string.settings)
        items.forEachIndexed { index, title -> popup.menu.add(0, index, index, title) }
        popup.setOnMenuItemClickListener { item ->
            when (items[item.itemId]) {
                R.string.browser -> startActivity(Intent(this, BrowserActivity::class.java))
                R.string.pause_all -> forEach(TaskPolicy.inFlight + TaskState.QUEUED + TaskState.WAITING_NETWORK, DownloadService.ACTION_PAUSE)
                R.string.resume_all -> forEach(setOf(TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK), DownloadService.ACTION_RESUME)
                R.string.clear_finished -> { TaskStore.get(this).clearFinished(); render() }
                R.string.plugins -> startActivity(Intent(this, SettingsActivity::class.java).putExtra(SettingsActivity.EXTRA_PLUGINS, true))
                else -> startActivity(Intent(this, SettingsActivity::class.java))
            }
            true
        }
        popup.show()
    }

    private fun render() {
        if (!::adapter.isInitialized) return
        val store = TaskStore.get(this)
        val all = store.list()
        val now = System.currentTimeMillis()
        val finished = setOf(TaskState.COMPLETED, TaskState.CANCELLED, TaskState.FAILED)
        val activeCount = all.count { it.state !in finished }
        activeTab.text = getString(R.string.tab_count, getString(R.string.active_jobs), activeCount)
        historyTab.text = getString(R.string.tab_count, getString(R.string.history), all.size - activeCount)
        summary.text = getString(R.string.summary_line, activeCount, all.size - activeCount)
        val open = Schedule.now(MobilePreferences(this).window)
        val inTab = all.asReversed().filter {
            (it.state in finished) == history && (query.isBlank() || it.title.contains(query, true) || it.fileName.contains(query, true))
        }
        val present = TaskFilter.present(inTab)
        if (category != null && category !in present) category = null
        renderFilters(present)
        val shown = inTab.filter { TaskFilter.matches(it, category) }
        shown.forEach { if (it.state !in TaskPolicy.inFlight) meter.forget(it.id) }
        adapter.submitList(shown.map { task ->
            val label = when {
                task.startAt > now -> getString(R.string.scheduled_for, DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(task.startAt)))
                !open && task.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) -> getString(R.string.outside_window)
                else -> ""
            }
            TaskRow(task, stats(task), label, task.id in selection)
        })
        empty.visibility = if (shown.isEmpty()) View.VISIBLE else View.GONE
        selection.retainAll(all.map { it.id }.toSet())
        header.visibility = if (selection.isEmpty()) View.VISIBLE else View.GONE
        selectionBar.visibility = if (selection.isEmpty()) View.GONE else View.VISIBLE
        selectionTitle.text = getString(R.string.selected_count, selection.size)
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

    /** A row of category chips (All, Video, Music…), shown only when the list holds more than one kind. */
    private fun renderFilters(present: List<String>) {
        val key = present.joinToString(",") + "|" + category
        if (key == filterKey) return
        filterKey = key
        filters.removeAllViews()
        fun add(label: String, value: String?) {
            val on = category == value
            filters.addView(TextView(this).apply {
                text = label; textSize = 13f; setPadding(dp(14), dp(7), dp(14), dp(7))
                setTextColor(if (on) paper else accent)
                background = if (on) rounded(accent, 20) else rounded(android.graphics.Color.TRANSPARENT, 20, accent and 0x66FFFFFF)
                setOnClickListener { category = value; render() }
            }, LinearLayout.LayoutParams(-2, -2).apply { marginEnd = dp(8) })
        }
        if (present.size > 1) { add(getString(R.string.filter_all), null); present.forEach { add(categoryName(it), it) } }
        filterScroll.visibility = if (filters.childCount == 0) View.GONE else View.VISIBLE
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

    // ---- list actions ------------------------------------------------------------------------------
    override fun command(task: MobileTask, action: String) = DownloadService.command(this, task.id, action)
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
        val actions = LinearLayout(this).apply { setPadding(0, dp(8), 0, 0) }
        fun act(text: Int, block: () -> Unit) = actions.addView(button(getString(text)) { sheet.dismiss(); block() }, LinearLayout.LayoutParams(0, -2, 1f))
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
        sheet.setContentView(box)
        sheet.show()
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
}
