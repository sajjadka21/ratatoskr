/* Derived from AB Download Manager HomePage.kt, BottomNavigation.kt, DownloadList.kt,
 * RenderDownloadItem.kt and EnterURLPage.kt (Apache-2.0).
 * Ratatoskr adaptations are described in third-party/ab-ui/NOTICE and provenance.json. */
package app.ratatoskr.android

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import app.ratatoskr.android.abicons.*

data class AbHomeState(
    val rows: List<TaskRow> = emptyList(), val selecting: Boolean = false, val selectionCount: Int = 0,
    val history: Boolean = false, val query: String = "", val summary: String = "",
    val clipboard: String = "", val category: String? = null,
)

@Composable
fun RatatoskrTheme(activity: MobileActivity, content: @Composable () -> Unit) {
    val colors = if (activity.dark) darkColorScheme() else lightColorScheme()
    val typography = Typography(
        bodyLarge = TextStyle(fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.sp),
        bodyMedium = TextStyle(fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.sp),
        bodySmall = TextStyle(fontSize = 12.sp, lineHeight = 18.sp, letterSpacing = 0.sp),
        labelLarge = TextStyle(fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.sp, fontWeight = FontWeight.Medium),
    )
    MaterialTheme(typography = typography, colorScheme = colors.copy(
        primary = Color(activity.accent), onPrimary = Color(activity.paper),
        background = Color(activity.paper), onBackground = Color(activity.ink),
        surface = Color(activity.surface), onSurface = Color(activity.ink),
        surfaceVariant = Color(activity.surface), onSurfaceVariant = Color(activity.muted),
        error = Color(activity.danger), outline = Color(activity.ink).copy(alpha = 0.15f),
    ), content = content)
}

private val abShape = RoundedCornerShape(16.dp)
private val primaryGradient: Brush
    @Composable get() = Brush.linearGradient(listOf(MaterialTheme.colorScheme.primary, MaterialTheme.colorScheme.primary.copy(alpha = 0.75f)))

/** AB's overlaid header/footer: the list uses their measured height, including large fonts. */
@Composable
private fun PageUi(header: @Composable () -> Unit, footer: @Composable () -> Unit, content: @Composable (PaddingValues) -> Unit) {
    var headerHeight by remember { mutableIntStateOf(0) }
    var footerHeight by remember { mutableIntStateOf(0) }
    val density = LocalDensity.current
    Box(Modifier.fillMaxSize()) {
        content(PaddingValues(top = with(density) { headerHeight.toDp() }, bottom = with(density) { footerHeight.toDp() }))
        Box(Modifier.onSizeChanged { headerHeight = it.height }.align(Alignment.TopCenter)) { header() }
        Box(Modifier.onSizeChanged { footerHeight = it.height }.align(Alignment.BottomCenter)) { footer() }
    }
}

/** AB SettingsPage/PageUi shell; existing native preference controls are adapters. */
@Composable
fun AbSettingsPage(scroll: android.widget.ScrollView, title: String, onBack: () -> Unit) {
    Surface(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
        PageUi(header = {
            Row(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.background).padding(horizontal = 8.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                AbIconButton(ABDMIcons.Back, stringResource(R.string.cancel), onBack)
                Text(title, Modifier.padding(start = 16.dp), fontWeight = FontWeight.Bold, fontSize = 20.sp)
            }
        }, footer = {}) { padding ->
            AndroidView(factory = { scroll.apply { (parent as? android.view.ViewGroup)?.removeView(this); isVerticalScrollBarEnabled = false } },
                modifier = Modifier.fillMaxSize().padding(top = padding.calculateTopPadding()))
        }
    }
}

@Composable
fun AbHome(
    state: AbHomeState, actions: TaskActions,
    onAdd: () -> Unit, onQuery: (String) -> Unit, onHistory: (Boolean) -> Unit,
    onMenu: (Int) -> Unit, onCategory: (String?) -> Unit, categories: List<Pair<String, String>>,
    onSort: (Int) -> Unit, onClipboard: (Boolean) -> Unit,
    onSelection: (Int) -> Unit,
) {
    var showingSearch by rememberSaveable { mutableStateOf(state.query.isNotEmpty()) }
    var menu by remember { mutableStateOf(false) }
    var filter by remember { mutableStateOf(false) }
    var activeOnly by rememberSaveable { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    val shown = if (activeOnly && !state.history) state.copy(rows = state.rows.filter { it.task.state !in setOf(TaskState.COMPLETED, TaskState.CANCELLED) }) else state
    PageUi(header = {
        Column(Modifier.fillMaxWidth().background(colors.background)) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                AndroidView(factory = { android.widget.ImageView(it).apply { setImageResource(R.mipmap.ic_launcher); importantForAccessibility = android.view.View.IMPORTANT_FOR_ACCESSIBILITY_NO } }, modifier = Modifier.size(32.dp))
                Text("Ratatoskr", Modifier.weight(1f).padding(start = 10.dp), fontSize = 22.sp, fontWeight = FontWeight.SemiBold)
                AbResourceButton(R.drawable.ic_nav_browser, R.string.browser) { onMenu(R.string.browser) }
                AbIconButton(ABDMIcons.Search, stringResource(R.string.search_history)) { showingSearch = !showingSearch }
                Box {
                    AbIconButton(ABDMIcons.Menu, stringResource(R.string.menu)) { menu = true }
                    DropdownMenu(menu, { menu = false }) {
                        listOf(R.string.paste, R.string.pause_all, R.string.resume_all, R.string.clear_finished, R.string.select_all, R.string.plugins).forEach { title ->
                            DropdownMenuItem(text = { Text(stringResource(title)) }, onClick = { menu = false; onMenu(title) })
                        }
                        listOf(R.string.sort_newest, R.string.sort_oldest, R.string.sort_name, R.string.sort_size).forEachIndexed { index, title ->
                            DropdownMenuItem(text = { Text(stringResource(title)) }, onClick = { menu = false; onSort(index) })
                        }
                    }
                }
            }
            if (state.selecting) Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                AbIconButton(ABDMIcons.Clear, stringResource(R.string.cancel)) { onSelection(R.string.cancel) }
                Text(stringResource(R.string.selected_count, state.selectionCount), Modifier.weight(1f))
                AbResourceButton(R.drawable.ic_pause, R.string.pause) { onSelection(R.string.pause) }
                AbResourceButton(R.drawable.ic_retry, R.string.resume) { onSelection(R.string.resume) }
                AbResourceButton(R.drawable.ic_delete, R.string.remove) { onSelection(R.string.remove) }
            } else Text(stringResource(R.string.nav_downloads), Modifier.padding(horizontal = 24.dp, vertical = 12.dp), fontSize = 30.sp, fontWeight = FontWeight.Bold)
            if (showingSearch) SearchBox(state.query, onQuery) { showingSearch = false; onQuery("") }
            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                listOf(R.string.filter_all, R.string.active_jobs, R.string.completed).forEachIndexed { index, title ->
                    val selected = if (state.history) index == 2 else index == if (activeOnly) 1 else 0
                    TextButton(onClick = { activeOnly = index == 1; onHistory(index == 2) }, modifier = Modifier.weight(1f).drawBehind {
                        if (selected) drawLine(colors.primary, Offset(0f, size.height), Offset(size.width, size.height), 2.dp.toPx())
                    }) { Text(stringResource(title), color = if (selected) colors.primary else colors.onSurfaceVariant) }
                }
                Box {
                    AbResourceButton(R.drawable.ic_filter, R.string.filter_category) { filter = true }
                    DropdownMenu(filter, { filter = false }) {
                        DropdownMenuItem(text = { Text(stringResource(R.string.filter_all)) }, onClick = { filter = false; onCategory(null) })
                        categories.forEach { (key, label) -> DropdownMenuItem(text = { Text(label) }, onClick = { filter = false; onCategory(key) }) }
                    }
                }
            }
            if (state.clipboard.isNotEmpty()) Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.clipboard_many, LinkPlan.parse(state.clipboard).size), Modifier.weight(1f))
                TextButton(onClick = { onClipboard(true) }) { Text(stringResource(R.string.download_action)) }
                AbIconButton(ABDMIcons.Clear, stringResource(R.string.dismiss)) { onClipboard(false) }
            }
        }
    }, footer = { RatatoskrDock(onAdd, { activeOnly = false; onHistory(false) }, { onMenu(R.string.settings) }) }) { padding -> DownloadList(shown, actions, padding) }
}

/** A real concave cradle, drawn independently of RTL destination placement. */
@Composable
private fun RatatoskrDock(onAdd: () -> Unit, onDownloads: () -> Unit, onSettings: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val gold = if (androidx.compose.ui.platform.LocalContext.current.resources.configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK == android.content.res.Configuration.UI_MODE_NIGHT_YES) Color(0xffD6B778) else Color(0xffB48B42)
    Box(Modifier.fillMaxWidth().height(96.dp).imePadding()) {
        Box(Modifier.fillMaxSize().drawBehind {
            val mid = size.width / 2; val top = 24.dp.toPx(); val radius = 36.dp.toPx()
            val path = Path().apply {
                moveTo(0f, top); lineTo(mid - radius - 12.dp.toPx(), top)
                cubicTo(mid - radius, top, mid - radius, top + radius, mid, top + radius)
                cubicTo(mid + radius, top + radius, mid + radius, top, mid + radius + 12.dp.toPx(), top)
                lineTo(size.width, top); lineTo(size.width, size.height); lineTo(0f, size.height); close()
            }
            drawPath(path, colors.surface)
        })
        Row(Modifier.fillMaxWidth().align(Alignment.BottomCenter).height(68.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f).clickable(onClick = onDownloads).heightIn(min = 56.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                Icon(painterResource(R.drawable.ic_nav_downloads), null, tint = colors.primary, modifier = Modifier.size(24.dp))
                Text(stringResource(R.string.nav_downloads), fontSize = 12.sp, color = colors.primary)
            }
            Spacer(Modifier.width(88.dp))
            Column(Modifier.weight(1f).clickable(onClick = onSettings).heightIn(min = 56.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                Icon(painterResource(R.drawable.ic_nav_settings), null, modifier = Modifier.size(24.dp))
                Text(stringResource(R.string.settings), fontSize = 12.sp)
            }
        }
        FilledIconButton(onClick = onAdd, modifier = Modifier.align(Alignment.TopCenter).size(56.dp), shape = CircleShape,
            colors = IconButtonDefaults.filledIconButtonColors(containerColor = gold, contentColor = Color(0xff12171B))) {
            Icon(ABDMIcons.Plus, stringResource(R.string.add_links), Modifier.size(28.dp))
        }
    }
}

@Composable
private fun SearchBox(text: String, onText: (String) -> Unit, onDismiss: () -> Unit) {
    BackHandler(onBack = onDismiss)
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    TextField(text, onText, Modifier.fillMaxWidth().heightIn(min = 52.dp).focusRequester(focus), singleLine = true,
        placeholder = { Text(stringResource(R.string.search_history), fontSize = 14.sp) },
        leadingIcon = { Icon(ABDMIcons.Search, null, Modifier.size(20.dp)) },
        trailingIcon = { AbIconButton(ABDMIcons.Clear, stringResource(R.string.cancel)) { if (text.isEmpty()) onDismiss() else onText("") } },
        colors = TextFieldDefaults.colors(focusedContainerColor = MaterialTheme.colorScheme.surface, unfocusedContainerColor = MaterialTheme.colorScheme.surface,
            focusedIndicatorColor = Color.Transparent, unfocusedIndicatorColor = Color.Transparent))
}

/** AB's flat, stable-keyed animated list, rather than an unrelated card dashboard. */
@Composable
private fun DownloadList(state: AbHomeState, actions: TaskActions, padding: PaddingValues) {
    val divider = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.10f)
    Box(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize().semantics { contentDescription = "downloads-list" }, state = rememberLazyListState(), contentPadding = padding) {
            itemsIndexed(state.rows.sortedBy { if (it.task.state in TaskPolicy.inFlight) 0 else if (it.task.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED)) 2 else 1 }, key = { _, row -> row.task.id }) { index, row ->
                Column(Modifier.animateItem()) {
                    val ordered = state.rows.sortedBy { if (it.task.state in TaskPolicy.inFlight) 0 else if (it.task.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED)) 2 else 1 }
                    val group = if (row.task.state in TaskPolicy.inFlight) 0 else if (row.task.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED)) 2 else 1
                    val previous = ordered.getOrNull(index - 1)?.task?.state?.let { if (it in TaskPolicy.inFlight) 0 else if (it in setOf(TaskState.COMPLETED, TaskState.CANCELLED)) 2 else 1 }
                    if (previous != group) Text(stringResource(when (group) { 0 -> R.string.downloading; 2 -> R.string.completed; else -> R.string.pending_downloads }), Modifier.padding(horizontal = 24.dp, vertical = 16.dp), fontWeight = FontWeight.SemiBold, fontSize = 16.sp)
                    RenderDownloadItem(row, state.selecting, actions,
                        Modifier.let { if (index == 0) it else it.drawBehind {
                            drawLine(Brush.horizontalGradient(listOf(Color.Transparent, divider, Color.Transparent)), Offset.Zero, Offset(size.width, 0f))
                        } })
                }
            }
        }
        if (state.rows.isEmpty()) Box(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.Center) {
            Text(stringResource(R.string.empty_jobs), color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.75f), fontSize = 14.sp, modifier = Modifier.padding(24.dp))
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RenderDownloadItem(row: TaskRow, selecting: Boolean, actions: TaskActions, modifier: Modifier) {
    val task = row.task
    val colors = MaterialTheme.colorScheme
    val statusColor = when (task.state) { TaskState.COMPLETED -> Color(0xff65bb86); TaskState.FAILED -> colors.error; in TaskPolicy.inFlight -> colors.primary; else -> colors.onSurfaceVariant }
    val name = task.title.ifEmpty { task.fileName.ifEmpty { LinkPlan.host(task.url) } }
    Column(modifier.fillMaxWidth().let { if (task.state in TaskPolicy.inFlight) it.padding(horizontal = 16.dp, vertical = 8.dp).clip(RoundedCornerShape(20.dp)).background(colors.surface) else it }.let { if (row.selected) it.background(Brush.horizontalGradient(listOf(colors.primary.copy(alpha = 0.15f), colors.primary.copy(alpha = 0.03f)))) else it }
        .combinedClickable(onClick = { if (selecting) actions.toggle(task) else actions.details(task) }, onLongClick = { actions.toggle(task) })
        .padding(16.dp).semantics { stateDescription = task.state.name }) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            AnimatedVisibility(selecting) { Checkbox(row.selected, onCheckedChange = { actions.toggle(task) }) }
            Icon(painterResource(when {
                task.state == TaskState.COMPLETED -> R.drawable.ic_kind_check
                task.fileName.endsWith(".zip", true) -> R.drawable.ic_kind_archive
                task.mime.startsWith("video/") -> R.drawable.ic_kind_video
                task.mime.startsWith("audio/") -> R.drawable.ic_kind_music
                task.mime.startsWith("image/") -> R.drawable.ic_kind_image
                else -> R.drawable.ic_kind_file
            }), null, Modifier.size(36.dp), tint = statusColor)
            Spacer(Modifier.width(8.dp))
            Column(Modifier.weight(1f)) {
                Text(name, maxLines = 2, overflow = TextOverflow.Ellipsis, fontSize = 16.sp, fontWeight = FontWeight.Medium)
                if (task.state in TaskPolicy.inFlight) {
                Spacer(Modifier.height(12.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    val track = Modifier.weight(1f).height(6.dp).clip(CircleShape)
                    if (task.state == TaskState.PROBING) LinearProgressIndicator(track, color = colors.primary)
                    else LinearProgressIndicator(progress = { task.progress.coerceIn(0,100)/100f }, modifier = track, color = statusColor, trackColor = colors.onSurface.copy(alpha = 0.12f))
                    Spacer(Modifier.width(4.dp))
                    Text("${task.progress}%", color = statusColor, fontSize = 12.sp)
                }
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(listOf(MobileText.state(androidx.compose.ui.platform.LocalContext.current, task), row.stats).filter { it.isNotEmpty() }.joinToString(" · "), fontSize = 13.sp, color = colors.onSurfaceVariant)
                if (row.schedule.isNotEmpty()) Text(row.schedule, fontSize = 11.sp, color = colors.primary)
                if (task.error.isNotEmpty()) Text(MobileText.error(androidx.compose.ui.platform.LocalContext.current, task.error), fontSize = 12.sp, color = colors.error)
            }
            if (!selecting) {
                when (task.state) {
                    TaskState.COMPLETED -> AbResourceButton(R.drawable.ic_open, R.string.open_file) { actions.open(task) }
                    TaskState.CANCELLED -> Unit
                    TaskState.PAUSED, TaskState.FAILED, TaskState.WAITING_NETWORK -> AbResourceButton(R.drawable.ic_retry, R.string.resume) { actions.command(task, DownloadService.ACTION_RESUME) }
                    else -> AbResourceButton(R.drawable.ic_pause, R.string.pause) { actions.command(task, DownloadService.ACTION_PAUSE) }
                }
            }
        }
    }
}

@Composable
private fun AbIconButton(icon: ImageVector, description: String, onClick: () -> Unit) {
    IconButton(onClick = onClick, modifier = Modifier.size(48.dp)) { Icon(icon, description, Modifier.size(20.dp)) }
}

@Composable
private fun AbResourceButton(icon: Int, description: Int, onClick: () -> Unit) {
    IconButton(onClick = onClick, modifier = Modifier.size(48.dp)) { Icon(painterResource(icon), stringResource(description), Modifier.size(20.dp)) }
}

/** AB URL sheet adapted to Ratatoskr's automatic routing and multiple-link parser. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AbEnterUrl(prefill: String, defaultAudio: Boolean, onClose: () -> Unit, onPaste: () -> String, onDownload: (String, Boolean) -> Boolean) {
    var text by rememberSaveable { mutableStateOf(prefill) }
    var audio by rememberSaveable { mutableStateOf(defaultAudio) }
    var advanced by rememberSaveable { mutableStateOf(defaultAudio) }
    var invalid by rememberSaveable { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val urls = remember(text) { LinkPlan.parse(text) }
    val counts = remember(urls) { LinkPlan.summarize(urls) }
    val navigationBottom = with(LocalDensity.current) { WindowInsets.navigationBars.getBottom(this).toDp() }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = MaterialTheme.colorScheme.surface, shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp), dragHandle = { BottomSheetDefaults.DragHandle() },
        contentWindowInsets = { WindowInsets(0, 0, 0, 0) }) {
        // The sheet subcomposes its children; request focus in that composition,
        // after the field has attached, rather than in the parent composition.
        LaunchedEffect(Unit) { focus.requestFocus() }
        Column(Modifier.fillMaxWidth().imePadding().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp).padding(bottom = 16.dp + navigationBottom)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.add_links), Modifier.weight(1f), fontSize = 20.sp, fontWeight = FontWeight.Bold)
                AbIconButton(ABDMIcons.Clear, stringResource(R.string.cancel), onClose)
            }
            OutlinedTextField(text, { text = it; invalid = false }, Modifier.focusRequester(focus).fillMaxWidth(),
                label = { Text(stringResource(R.string.links_hint)) }, minLines = 2, maxLines = 5, isError = invalid,
                trailingIcon = { AbResourceButton(R.drawable.ic_paste, R.string.paste_clipboard) { val pasted = onPaste(); if (pasted.isNotBlank()) text = if (text.isBlank()) pasted else "$text\n$pasted" } },
                supportingText = { if (invalid) Text(stringResource(R.string.bad_link)) else Text(stringResource(R.string.add_link_help)) }, shape = abShape)
            if (urls.isNotEmpty()) Text(stringResource(R.string.links_summary, urls.size, counts.files, counts.media), fontSize = 12.sp, color = MaterialTheme.colorScheme.primary)
            TextButton(onClick = { advanced = !advanced }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.more_options)) }
            if (advanced) {
            Row(Modifier.fillMaxWidth().clickable { audio = !audio }, verticalAlignment = Alignment.CenterVertically) {
                Checkbox(audio, { audio = it }); Text(stringResource(R.string.audio_only_all), fontSize = 13.sp)
            }
            Text(stringResource(R.string.pattern_hint), fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Spacer(Modifier.height(16.dp))
            Button(onClick = { invalid = !onDownload(text, audio) }, enabled = text.isNotBlank(), modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp), shape = abShape) { Text(stringResource(R.string.download_action)) }
        }
    }
}
