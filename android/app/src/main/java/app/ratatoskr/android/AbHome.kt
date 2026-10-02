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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.ratatoskr.android.abicons.*

data class AbHomeState(
    val rows: List<TaskRow> = emptyList(), val selecting: Boolean = false,
    val history: Boolean = false, val query: String = "", val summary: String = "",
    val clipboard: String = "", val category: String? = null,
)

@Composable
fun RatatoskrTheme(activity: MobileActivity, content: @Composable () -> Unit) {
    val colors = if (activity.dark) darkColorScheme() else lightColorScheme()
    MaterialTheme(colorScheme = colors.copy(
        primary = Color(activity.accent), onPrimary = Color(activity.paper),
        background = Color(activity.paper), onBackground = Color(activity.ink),
        surface = Color(activity.surface), onSurface = Color(activity.ink),
        surfaceVariant = Color(activity.surface), onSurfaceVariant = Color(activity.muted),
        error = Color(activity.danger), outline = Color(activity.ink).copy(alpha = 0.15f),
    ), content = content)
}

private val abShape = RoundedCornerShape(8.dp)
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
    var sort by remember { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    PageUi(
        header = {
            Column(Modifier.fillMaxWidth().background(colors.background.copy(alpha = 0.96f))) {
                Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                    if (state.selecting) {
                        AbIconButton(ABDMIcons.Clear, stringResource(R.string.cancel)) { onSelection(R.string.cancel) }
                        Text(stringResource(R.string.selected_count, state.rows.count { it.selected }), Modifier.weight(1f), fontSize = 18.sp)
                        AbResourceButton(R.drawable.ic_pause, R.string.pause) { onSelection(R.string.pause) }
                        AbResourceButton(R.drawable.ic_retry, R.string.resume) { onSelection(R.string.resume) }
                        AbResourceButton(R.drawable.ic_delete, R.string.remove) { onSelection(R.string.remove) }
                    } else {
                        Image(painterResource(R.mipmap.ic_launcher), null, Modifier.size(28.dp))
                        Spacer(Modifier.width(12.dp))
                        Text("Ratatoskr", Modifier.weight(1f), fontSize = 20.sp, fontWeight = FontWeight.Bold)
                        Text(state.summary, color = colors.onSurfaceVariant, fontSize = 11.sp, modifier = Modifier.widthIn(max = 150.dp), maxLines = 2)
                    }
                }
                if (state.clipboard.isNotEmpty()) {
                    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(stringResource(R.string.clipboard_many, LinkPlan.parse(state.clipboard).size), Modifier.weight(1f), fontSize = 12.sp)
                        TextButton(onClick = { onClipboard(true) }) { Text(stringResource(R.string.download_action)) }
                        AbIconButton(ABDMIcons.Clear, stringResource(R.string.dismiss)) { onClipboard(false) }
                    }
                }
            }
        },
        footer = {
            Column(Modifier.background(Brush.verticalGradient(listOf(Color.Transparent, colors.background))).imePadding()) {
                BottomNavigation(showingSearch, state.query, onQuery, { showingSearch = false }, { showingSearch = true }, onAdd,
                    menu, { menu = !menu }, filter, { filter = !filter }, sort, { sort = !sort }, state.history, state.category,
                    mainMenu = {
                        DropdownMenu(menu, { menu = false }) {
                            listOf(R.string.browser, R.string.paste, R.string.pause_all, R.string.resume_all, R.string.clear_finished, R.string.select_all, R.string.plugins, R.string.settings).forEach { title ->
                                DropdownMenuItem(text = { Text(stringResource(title)) }, onClick = { menu = false; onMenu(title) })
                            }
                        }
                    },
                    filterMenu = {
                        DropdownMenu(filter, { filter = false }) {
                            DropdownMenuItem(text = { Text(stringResource(R.string.nav_downloads)) }, onClick = { filter = false; onHistory(false) })
                            DropdownMenuItem(text = { Text(stringResource(R.string.history)) }, onClick = { filter = false; onHistory(true) })
                            HorizontalDivider()
                            DropdownMenuItem(text = { Text(stringResource(R.string.filter_all)) }, onClick = { filter = false; onCategory(null) })
                            categories.forEach { (key, label) -> DropdownMenuItem(text = { Text(label) }, onClick = { filter = false; onCategory(key) }) }
                        }
                    },
                    sortMenu = {
                        DropdownMenu(sort, { sort = false }) {
                            listOf(R.string.sort_newest, R.string.sort_oldest, R.string.sort_name, R.string.sort_size).forEachIndexed { index, title ->
                                DropdownMenuItem(text = { Text(stringResource(title)) }, onClick = { sort = false; onSort(index) })
                            }
                        }
                    })
            }
        },
    ) { padding -> DownloadList(state, actions, padding) }
}

/** Adapted directly from AB's BottomNavigation and navigation item composables. */
@Composable
private fun BottomNavigation(
    showingSearch: Boolean, query: String, onQuery: (String) -> Unit, onDismissSearch: () -> Unit, onSearch: () -> Unit,
    onAdd: () -> Unit, menuSelected: Boolean, onMenu: () -> Unit,
    filterSelected: Boolean, onFilter: () -> Unit, sortSelected: Boolean, onSort: () -> Unit,
    history: Boolean, category: String?,
    mainMenu: @Composable () -> Unit, filterMenu: @Composable () -> Unit, sortMenu: @Composable () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.weight(1f).height(IntrinsicSize.Max).shadow(4.dp, abShape).clip(abShape)
            .border(1.dp, colors.onSurface.copy(alpha = 0.1f), abShape).background(colors.surface)) {
            AnimatedContent(showingSearch, label = "AB toolbar search") { search ->
                Row(Modifier.fillMaxWidth().height(IntrinsicSize.Max), verticalAlignment = Alignment.CenterVertically) {
                    if (search) SearchBox(query, onQuery, onDismissSearch)
                    else {
                        Box { mainMenu(); BottomNavigationItem(ABDMIcons.Menu, stringResource(R.string.menu), menuSelected, onMenu) }
                        BottomNavigationItem(ABDMIcons.Search, stringResource(R.string.search_history), false, onSearch)
                        Spacer(Modifier.fillMaxHeight().width(1.dp).background(colors.onSurface.copy(alpha = 0.1f)))
                        Box(Modifier.weight(1f)) {
                            filterMenu()
                            Row(Modifier.fillMaxWidth().heightIn(min = 52.dp).clickable(onClick = onFilter).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
                                Icon(if (history) ABDMIcons.FolderFinished else ABDMIcons.FolderUnfinished, null, Modifier.size(16.dp))
                                Spacer(Modifier.width(6.dp))
                                Text(category?.let { stringResource(R.string.filter_category) } ?: stringResource(if (history) R.string.history else R.string.nav_downloads), fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                            BottomNavigationSelectedIndicator(filterSelected)
                        }
                        Spacer(Modifier.fillMaxHeight().width(1.dp).background(colors.onSurface.copy(alpha = 0.1f)))
                        Box { sortMenu(); BottomNavigationItem(ABDMIcons.Clock, stringResource(R.string.sort_downloads), sortSelected, onSort) }
                    }
                }
            }
        }
        AnimatedVisibility(!showingSearch) {
            Row {
                Spacer(Modifier.width(8.dp))
                Icon(ABDMIcons.Plus, stringResource(R.string.add_links), Modifier.shadow(4.dp, abShape)
                    .border(1.dp, primaryGradient, abShape).clip(abShape).background(colors.surface)
                    .background(Brush.linearGradient(listOf(colors.primary.copy(alpha = 0.25f), colors.primary.copy(alpha = 0.15f))))
                    .clickable(onClick = onAdd).padding(16.dp).size(20.dp), tint = colors.primary)
            }
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

@Composable
private fun BottomNavigationItem(icon: ImageVector, description: String, selected: Boolean, onClick: () -> Unit) {
    Box {
        Icon(icon, description, Modifier.clickable(onClick = onClick).padding(16.dp).size(20.dp))
        BottomNavigationSelectedIndicator(selected)
    }
}

@Composable
private fun BoxScope.BottomNavigationSelectedIndicator(selected: Boolean) {
    if (selected) {
        val color = MaterialTheme.colorScheme.primary
        Box(Modifier.matchParentSize().background(Brush.horizontalGradient(listOf(color.copy(alpha = 0.15f), Color.Transparent))))
        Box(Modifier.matchParentSize().wrapContentHeight(Alignment.Bottom).height(1.dp).background(primaryGradient))
    }
}

/** AB's flat, stable-keyed animated list, rather than an unrelated card dashboard. */
@Composable
private fun DownloadList(state: AbHomeState, actions: TaskActions, padding: PaddingValues) {
    val divider = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.25f)
    Box(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize().semantics { contentDescription = "downloads-list" }, state = rememberLazyListState(), contentPadding = padding) {
            itemsIndexed(state.rows, key = { _, row -> row.task.id }) { index, row ->
                Column(Modifier.animateItem()) {
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
    Column(modifier.fillMaxWidth().let { if (row.selected) it.background(Brush.horizontalGradient(listOf(colors.primary.copy(alpha = 0.15f), colors.primary.copy(alpha = 0.03f)))) else it }
        .combinedClickable(onClick = { if (selecting) actions.toggle(task) else actions.details(task) }, onLongClick = { actions.toggle(task) })
        .padding(16.dp).semantics { stateDescription = task.state.name }) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            AnimatedVisibility(selecting) { Checkbox(row.selected, onCheckedChange = { actions.toggle(task) }) }
            Icon(ABDMIcons.File, null, Modifier.size(24.dp), tint = colors.primary)
            Spacer(Modifier.width(8.dp))
            Column(Modifier.weight(1f)) {
                Text(name, maxLines = 1, overflow = TextOverflow.Ellipsis, fontSize = 14.sp)
                Spacer(Modifier.height(8.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    val track = Modifier.weight(1f).height(6.dp).clip(CircleShape)
                    if (task.state == TaskState.PROBING) LinearProgressIndicator(track, color = colors.primary)
                    else LinearProgressIndicator(progress = { task.progress.coerceIn(0,100)/100f }, modifier = track, color = statusColor, trackColor = colors.onSurface.copy(alpha = 0.12f))
                    Spacer(Modifier.width(4.dp))
                    Box(Modifier.size(6.dp).background(statusColor, CircleShape))
                }
            }
        }
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(row.stats.ifEmpty { MobileText.state(androidx.compose.ui.platform.LocalContext.current, task) }, fontSize = 11.sp, color = colors.onSurfaceVariant)
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
    var invalid by rememberSaveable { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val urls = remember(text) { LinkPlan.parse(text) }
    val counts = remember(urls) { LinkPlan.summarize(urls) }
    ModalBottomSheet(onDismissRequest = onClose, containerColor = MaterialTheme.colorScheme.surface, shape = RoundedCornerShape(topStart = 8.dp, topEnd = 8.dp), dragHandle = null) {
        Column(Modifier.fillMaxWidth().imePadding().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp).padding(bottom = 16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.add_links), Modifier.weight(1f), fontSize = 20.sp, fontWeight = FontWeight.Bold)
                AbIconButton(ABDMIcons.Clear, stringResource(R.string.cancel), onClose)
            }
            OutlinedTextField(text, { text = it; invalid = false }, Modifier.focusRequester(focus).fillMaxWidth(),
                label = { Text(stringResource(R.string.links_hint)) }, minLines = 2, maxLines = 5, isError = invalid,
                trailingIcon = { AbResourceButton(R.drawable.ic_paste, R.string.paste_clipboard) { val pasted = onPaste(); if (pasted.isNotBlank()) text = if (text.isBlank()) pasted else "$text\n$pasted" } },
                supportingText = { if (invalid) Text(stringResource(R.string.bad_link)) else Text(stringResource(R.string.add_link_help)) }, shape = abShape)
            if (urls.isNotEmpty()) Text(stringResource(R.string.links_summary, urls.size, counts.files, counts.media), fontSize = 12.sp, color = MaterialTheme.colorScheme.primary)
            Row(Modifier.fillMaxWidth().clickable { audio = !audio }, verticalAlignment = Alignment.CenterVertically) {
                Checkbox(audio, { audio = it }); Text(stringResource(R.string.audio_only_all), fontSize = 13.sp)
            }
            Text(stringResource(R.string.pattern_hint), fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(16.dp))
            Row {
                OutlinedButton(onClose, Modifier.weight(1f), shape = abShape) { Text(stringResource(R.string.cancel)) }
                Spacer(Modifier.width(8.dp))
                Button(onClick = { invalid = !onDownload(text, audio) }, enabled = text.isNotBlank(), modifier = Modifier.weight(1f), shape = abShape) { Text(stringResource(R.string.download_action)) }
            }
        }
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
}
