package app.ratatoskr.android

import android.content.Context
import android.graphics.BitmapFactory
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private val Round = RoundedCornerShape(14.dp)
private val Ink = Color(0xff12171B)
private val Gold = Color(0xffD6B778)

@Composable private fun UiIcon(icon: Int, label: Int, action: () -> Unit) {
    IconButton(onClick = action, modifier = Modifier.size(48.dp)) { Icon(painterResource(icon), stringResource(label), Modifier.size(24.dp)) }
}

@Composable
fun AbHome(state: AbHomeState, actions: TaskActions, onAdd: () -> Unit, onQuery: (String) -> Unit,
    onHistory: (Boolean) -> Unit, onMenu: (Int) -> Unit, onCategory: (String?) -> Unit,
    categories: List<Pair<String, String>>, onSort: (Int) -> Unit, onClipboard: (Boolean) -> Unit, onSelection: (Int) -> Unit, onFilter: (() -> Unit)? = null) {
    var search by rememberSaveable { mutableStateOf(state.query.isNotEmpty()) }
    var active by rememberSaveable { mutableStateOf(false) }
    var menu by remember { mutableStateOf(false) }
    var filter by remember { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    val rows = if (active && !state.history) state.rows.filter { it.task.state !in setOf(TaskState.COMPLETED, TaskState.CANCELLED) } else state.rows
    Column(Modifier.fillMaxSize().background(colors.background)) {
        if (!search) CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
            BoxWithConstraints(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 12.dp)) {
                val brandWidth = (maxWidth - 200.dp).coerceAtLeast(90.dp)
                Row(Modifier.align(Alignment.CenterEnd).widthIn(max = brandWidth), verticalAlignment = Alignment.CenterVertically) {
                    Image(painterResource(R.drawable.brand_squirrel), null, Modifier.size(34.dp), colorFilter=androidx.compose.ui.graphics.ColorFilter.tint(colors.primary))
                    Text("Ratatoskr", Modifier.padding(start = 8.dp), color = colors.primary, fontSize = 22.sp, fontWeight = FontWeight.SemiBold, maxLines=1, overflow=TextOverflow.Ellipsis)
                }
                Row(Modifier.align(Alignment.CenterStart), verticalAlignment = Alignment.CenterVertically) {
                    UiIcon(R.drawable.ui_power, R.string.exit_app) { onMenu(R.string.exit_app) }
                    Box {
                        UiIcon(R.drawable.ui_ellipsis_vertical, R.string.menu) { menu = true }
                        DropdownMenu(menu, { menu = false }) {
                            listOf(R.string.paste, R.string.pause_all, R.string.resume_all, R.string.clear_finished, R.string.select_all, R.string.plugins).forEach { title ->
                                DropdownMenuItem(text = { Text(stringResource(title)) }, onClick = { menu = false; onMenu(title) })
                            }
                            listOf(R.string.sort_newest, R.string.sort_oldest, R.string.sort_name, R.string.sort_size, R.string.sort_format, R.string.sort_manual).forEachIndexed { index, title ->
                                DropdownMenuItem(text = { Text(stringResource(title)) }, onClick = { menu = false; onSort(index) })
                            }
                        }
                    }
                    UiIcon(R.drawable.ui_search, R.string.search_history) { search = !search }
                    UiIcon(R.drawable.ui_globe, R.string.browser) { onMenu(R.string.browser) }
                }
            }
        }
        if (state.selecting) Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            UiIcon(R.drawable.ui_x, R.string.cancel) { onSelection(R.string.cancel) }
            Text(stringResource(R.string.selected_count, state.selectionCount), Modifier.weight(1f))
        } else if (!search) Text(stringResource(R.string.nav_downloads), Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(top = 16.dp, bottom = 12.dp), fontSize = 30.sp, lineHeight = 36.sp, fontWeight = FontWeight.Bold, textAlign = TextAlign.Start)
        if (search) {
            BackHandler { search = false; onQuery("") }
            val focus = remember { FocusRequester() }
            LaunchedEffect(Unit) { focus.requestFocus() }
            OutlinedTextField(state.query, onQuery, Modifier.fillMaxWidth().padding(horizontal = 16.dp).focusRequester(focus), singleLine = true,
                placeholder = { Text(stringResource(R.string.search_history)) }, trailingIcon = { UiIcon(R.drawable.ui_x, R.string.cancel) { search = false; onQuery("") } }, shape = Round)
        } else Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp).height(48.dp), verticalAlignment = Alignment.CenterVertically) {
            listOf(R.string.filter_all, R.string.active_jobs, R.string.group_completed).forEachIndexed { index, label ->
                val selected = if (state.history) index == 2 else index == if (active) 1 else 0
                Box(Modifier.weight(1f).fillMaxHeight().clickable { active = index == 1; onHistory(index == 2) }
                    .semantics { this.selected = selected }.drawBehind {
                        if (selected) drawLine(colors.primary, Offset(8.dp.toPx(), size.height - 1.dp.toPx()), Offset(size.width - 8.dp.toPx(), size.height - 1.dp.toPx()), 2.dp.toPx())
                    }, contentAlignment = Alignment.Center) {
                    Text(stringResource(label), color = if (selected) colors.primary else colors.onSurfaceVariant, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            Box {
                UiIcon(R.drawable.ui_filter, R.string.filter_category) { if (onFilter != null) onFilter() else filter = true }
                DropdownMenu(filter, { filter = false }) {
                    DropdownMenuItem(text = { Text(stringResource(R.string.filter_all)) }, onClick = { filter = false; onCategory(null) })
                    categories.forEach { (key, label) -> DropdownMenuItem(text = { Text(label) }, onClick = { filter = false; onCategory(key) }) }
                }
            }
        }
        if (state.filtersActive) Text(stringResource(R.string.filters_active), Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp), color = colors.primary, fontSize = 12.sp)
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = { onMenu(R.string.create_queue) }) { Icon(painterResource(R.drawable.ui_plus), null, Modifier.size(18.dp)); Text(stringResource(R.string.create_queue)) }
            state.namedQueues.filter { name -> rows.none { it.task.groupName == name } }.forEach { name -> Text(name, Modifier.padding(horizontal = 12.dp), color = colors.primary) }
        }
        HorizontalDivider(color = colors.outline.copy(alpha = colors.outline.alpha * .8f))
        if (state.clipboard.isNotEmpty()) Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(R.string.clipboard_many, LinkPlan.parse(state.clipboard).size), Modifier.weight(1f), fontSize = 13.sp)
            TextButton(onClick = { onClipboard(true) }) { Text(stringResource(R.string.paste_short)) }
            UiIcon(R.drawable.ui_x, R.string.dismiss) { onClipboard(false) }
        }
        Box(Modifier.weight(1f)) {
            if (rows.isEmpty()) Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text(stringResource(R.string.empty_jobs), Modifier.padding(24.dp), color = colors.onSurfaceVariant) }
            else LazyColumn(Modifier.fillMaxSize().semantics { contentDescription = "downloads-list" }, contentPadding = PaddingValues(bottom = 12.dp)) {
                val ungrouped = rows.filter { it.task.groupName.isEmpty() }
                rows.filter { it.task.groupName.isNotEmpty() }.groupBy { it.task.groupName }.forEach { (name, members) ->
                    item(key = "named-$name") {
                        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                            Text(name + " (${members.size})", Modifier.weight(1f), fontWeight = FontWeight.SemiBold, color = colors.primary)
                            val running = members.any { it.task.state in TaskPolicy.inFlight || it.task.state in setOf(TaskState.QUEUED, TaskState.WAITING_NETWORK) }
                            UiIcon(if (running) R.drawable.ui_pause else R.drawable.ui_play, if (running) R.string.pause else R.string.resume) { actions.group(name, !running) }
                        }
                    }
                    items(members, key = { it.task.id }) { row -> if (row.task.state in TaskPolicy.inFlight) ActiveTransfer(row, state.selecting, actions) else FileRow(row, state.selecting, actions) }
                }
                val groups = listOf(ungrouped.filter { it.task.state in TaskPolicy.inFlight }, ungrouped.filter { it.task.state !in TaskPolicy.inFlight && it.task.state !in setOf(TaskState.COMPLETED, TaskState.CANCELLED) }, ungrouped.filter { it.task.state in setOf(TaskState.COMPLETED, TaskState.CANCELLED) })
                groups.forEachIndexed { index, group ->
                    if (group.isNotEmpty()) {
                        item(key = "section-$index") { Text(stringResource(when(index) { 0 -> R.string.group_transferring; 2 -> R.string.group_completed; else -> R.string.pending_downloads }) + " (${java.text.NumberFormat.getIntegerInstance(LocalContext.current.resources.configuration.locales[0]).format(group.size)})", Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(top = 12.dp, bottom = 4.dp), fontSize = 16.sp, lineHeight=22.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Start) }
                        items(group, key = { it.task.id }) { row -> if (index == 0) ActiveTransfer(row, state.selecting, actions) else FileRow(row, state.selecting, actions) }
                        if (index == 0) item(key = "network") { NetworkStatus() }
                    }
                }
            }
        }
        if (state.selecting) SelectionTools(state, onSelection)
        else if (!search) TransferDock(onAdd, { active = false; onHistory(false) }, { onMenu(R.string.settings) })
    }
}

@Composable private fun SelectionTools(state: AbHomeState, onAction: (Int) -> Unit) {
    val selected = state.rows.filter { it.selected }.map { it.task }
    val single = selected.singleOrNull()
    val editable = single != null && single.state !in TaskPolicy.inFlight && single.state !in setOf(TaskState.COMPLETED, TaskState.CANCELLED)
    val buttons = listOf(
        Triple(R.drawable.ui_play, R.string.resume, selected.any { RowTapPolicy.action(it.state, false) == RowTapAction.RESUME }),
        Triple(R.drawable.ui_pause, R.string.pause, selected.any { it.state in TaskPolicy.inFlight }),
        Triple(R.drawable.ui_info, R.string.task_details, single != null),
        Triple(R.drawable.ui_edit, R.string.change_link, editable),
        Triple(R.drawable.ui_up, R.string.selection_up, editable),
        Triple(R.drawable.ui_down, R.string.selection_down, editable),
        Triple(R.drawable.ic_retry, R.string.selection_repeat, selected.isNotEmpty()),
        Triple(R.drawable.ic_delete, R.string.remove, selected.isNotEmpty())
    )
    Surface(tonalElevation = 3.dp, shadowElevation = 6.dp, modifier = Modifier.fillMaxWidth()) {
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp, vertical = 4.dp), horizontalArrangement = Arrangement.SpaceEvenly) {
                    buttons.forEach { (icon, label, enabled) ->
                        IconButton(onClick = { onAction(label) }, enabled = enabled, modifier = Modifier.size(48.dp)) {
                            Icon(painterResource(icon), stringResource(label), Modifier.size(24.dp), tint = if (label == R.string.remove) MaterialTheme.colorScheme.error else LocalContentColor.current)
                        }
                    }
        }
    }
}

@Composable private fun TransferDock(onAdd: () -> Unit, onDownloads: () -> Unit, onSettings: () -> Unit) {
    val c = MaterialTheme.colorScheme
    Box(Modifier.fillMaxWidth().height(88.dp)) {
        Box(Modifier.fillMaxSize().drawBehind {
            val x=size.width/2;val y=16.dp.toPx();val r=34.dp.toPx();val shoulder=12.dp.toPx();val depth=46.dp.toPx()
            val p=Path().apply { moveTo(0f,y);lineTo(x-r-shoulder,y);cubicTo(x-r,y,x-r,y+depth,x,y+depth);cubicTo(x+r,y+depth,x+r,y,x+r+shoulder,y);lineTo(size.width,y);lineTo(size.width,size.height);lineTo(0f,size.height);close() }
            drawPath(p,c.surface)
            val edge=Path().apply { moveTo(0f,y);lineTo(x-r-shoulder,y);cubicTo(x-r,y,x-r,y+depth,x,y+depth);cubicTo(x+r,y+depth,x+r,y,x+r+shoulder,y);lineTo(size.width,y) }
            drawPath(edge,c.outline.copy(alpha=c.outline.alpha*.8f),style=androidx.compose.ui.graphics.drawscope.Stroke(1.dp.toPx()))
        })
        CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
            Row(Modifier.align(Alignment.BottomCenter).fillMaxWidth().height(66.dp), verticalAlignment = Alignment.CenterVertically) {
                DockItem(R.drawable.ui_settings, R.string.settings, c.onSurfaceVariant, Modifier.weight(1f), onSettings)
                Spacer(Modifier.width(96.dp))
                DockItem(R.drawable.ui_download, R.string.nav_downloads, c.primary, Modifier.weight(1f), onDownloads)
            }
        }
        FilledIconButton(onClick=onAdd,modifier=Modifier.align(Alignment.TopCenter).size(56.dp).background(androidx.compose.ui.graphics.Brush.linearGradient(listOf(Color(0xffE2C78C),Color(0xffC5A361))),CircleShape).border(1.dp,Gold,CircleShape),shape=CircleShape,
            colors=IconButtonDefaults.filledIconButtonColors(containerColor=Color.Transparent,contentColor=Ink)) { Icon(painterResource(R.drawable.ui_plus), stringResource(R.string.add_links), Modifier.size(30.dp),tint=Ink) }
    }
}
@Composable private fun DockItem(icon:Int,label:Int,tint:Color,modifier:Modifier,action:()->Unit) {
    Column(modifier.fillMaxHeight().clickable(onClick=action),horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.Center) {
        Icon(painterResource(icon),null,Modifier.size(26.dp),tint=tint)
        Text(stringResource(label),color=tint,fontSize=13.sp)
    }
}
@Composable private fun TransferAction(task:MobileTask,actions:TaskActions) {
    val c=MaterialTheme.colorScheme
    val resume=task.state in setOf(TaskState.SAVED,TaskState.PAUSED,TaskState.FAILED,TaskState.WAITING_NETWORK)
    val waiting=task.state in setOf(TaskState.QUEUED,TaskState.NEEDS_SELECTION,TaskState.CANCELLED)
    val complete=task.state==TaskState.COMPLETED
    val tint=when { complete -> Color(0xff65BB86); task.state==TaskState.FAILED -> c.error; waiting -> c.onSurfaceVariant; else -> c.primary }
    val label=when { complete -> R.string.open_file; resume -> R.string.resume; waiting -> R.string.task_details; else -> R.string.pause }
    IconButton(onClick={ when { complete -> actions.open(task); waiting -> actions.details(task); resume -> actions.command(task,DownloadService.ACTION_RESUME); else -> actions.command(task,DownloadService.ACTION_PAUSE) } },
        modifier=Modifier.size(48.dp).border(1.5.dp,tint.copy(alpha=.8f),CircleShape)) {
        Icon(painterResource(when { complete -> R.drawable.ui_check; resume -> R.drawable.ui_play; task.state==TaskState.CANCELLED -> R.drawable.ui_x; waiting -> R.drawable.ui_clock; else -> R.drawable.ui_pause }),stringResource(label),Modifier.size(24.dp),tint=tint)
    }
}
private fun kindIcon(task:MobileTask):Int {
    val name=task.fileName.ifEmpty { task.title }.lowercase(java.util.Locale.ROOT)
    return when { name.endsWith(".zip") || name.endsWith(".rar") || name.endsWith(".7z") -> R.drawable.ui_file_archive
        task.mime.startsWith("video/") || name.endsWith(".mp4") -> R.drawable.ui_file_video
        task.mime.startsWith("audio/") || name.endsWith(".mp3") -> R.drawable.ui_file_audio
        task.mime.startsWith("image/") -> R.drawable.ui_image; else -> R.drawable.ui_file_text }
}
@Composable private fun FileBadge(task:MobileTask,modifier:Modifier=Modifier) {
    val c=MaterialTheme.colorScheme
    val archive=kindIcon(task)==R.drawable.ui_file_archive
    val completedPdf=task.state==TaskState.COMPLETED && task.fileName.lowercase(java.util.Locale.ROOT).endsWith(".pdf")
    Box(modifier.size(32.dp).clip(RoundedCornerShape(6.dp)).background(if(completedPdf)Color(0xffB83E36) else if(archive)c.primary.copy(alpha=.4f) else c.onSurfaceVariant.copy(alpha=.2f)),contentAlignment=Alignment.Center) {
        Icon(painterResource(kindIcon(task)),null,Modifier.size(24.dp),tint=if(completedPdf)Color.White else if(archive)c.primary else c.onSurface)
    }
}
@OptIn(ExperimentalFoundationApi::class)
@Composable private fun FileRow(row:TaskRow,selecting:Boolean,actions:TaskActions) {
    val t=row.task;val c=MaterialTheme.colorScheme;var menu by remember { mutableStateOf(false) }
    Column(Modifier.padding(horizontal=12.dp).fillMaxWidth().combinedClickable(onClick={actions.tap(t)},onLongClick={actions.toggle(t)}).background(if(row.selected)c.primary.copy(alpha=.1f) else Color.Transparent)) {
        HorizontalDivider(color=c.outline.copy(alpha=c.outline.alpha*.8f))
        CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
            Row(Modifier.fillMaxWidth().padding(vertical=10.dp),verticalAlignment=Alignment.CenterVertically) {
                if(selecting)Checkbox(row.selected,{actions.toggle(t)}) else TransferAction(t,actions)
                Spacer(Modifier.width(12.dp));FileBadge(t);Spacer(Modifier.width(16.dp))
                Column(Modifier.weight(1f)) {
                    Text(t.title.ifEmpty { t.fileName.ifEmpty { LinkPlan.host(t.url) } },fontSize=16.sp,lineHeight=22.sp,maxLines=2,overflow=TextOverflow.Ellipsis,style=MaterialTheme.typography.bodyLarge.copy(textDirection=TextDirection.Ltr),fontWeight=FontWeight.Medium)
                    val status=MobileText.state(LocalContext.current,t)
                    val size=if(t.state==TaskState.COMPLETED)Format.bytes(t.totalBytes.takeIf { it>0 }?:t.bytesDone) else row.stats
                    Text(listOf("\u2067$status\u2069",size.takeIf { it.isNotBlank() }?.let { "\u2066$it\u2069" }.orEmpty()).filter { it.isNotBlank() }.joinToString(" · "),fontSize=12.sp,lineHeight=18.sp,color=if(t.state==TaskState.FAILED)c.error else c.onSurfaceVariant,maxLines=2,overflow=TextOverflow.Ellipsis,style=MaterialTheme.typography.bodySmall.copy(textDirection=TextDirection.Ltr))
                    if(t.error.isNotEmpty())Text(MobileText.error(LocalContext.current,t.error),color=c.error,fontSize=12.sp,maxLines=2,overflow=TextOverflow.Ellipsis)
                    if(row.schedule.isNotEmpty())Text(row.schedule,color=c.primary,fontSize=12.sp)
                }
                Box {
                    UiIcon(R.drawable.ui_ellipsis_vertical,R.string.task_details){menu=true}
                    DropdownMenu(menu,{menu=false}) {
                        DropdownMenuItem(text={Text(stringResource(R.string.task_details))},onClick={menu=false;actions.details(t)})
                        if(t.uri.isNotBlank()) DropdownMenuItem(text={Text(stringResource(R.string.share_file))},onClick={menu=false;actions.share(t)})
                        DropdownMenuItem(text={Text(stringResource(R.string.remove))},onClick={menu=false;if(t.state==TaskState.QUEUED)actions.command(t,DownloadService.ACTION_CANCEL) else actions.remove(t)})
                    }
                }
            }
        }
    }
}
@OptIn(ExperimentalFoundationApi::class)
@Composable private fun ActiveTransfer(row:TaskRow,selecting:Boolean,actions:TaskActions) {
    val t=row.task;val c=MaterialTheme.colorScheme
    Column(Modifier.fillMaxWidth().padding(horizontal=8.dp,vertical=0.dp).clip(RoundedCornerShape(20.dp)).background(c.surface).border(1.dp,c.outline.copy(alpha=c.outline.alpha*.6f),RoundedCornerShape(20.dp))
        .combinedClickable(onClick={actions.tap(t)},onLongClick={actions.toggle(t)}).padding(12.dp)) {
        CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
            Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {
                if(selecting)Checkbox(row.selected,{actions.toggle(t)}) else TransferAction(t,actions)
                Column(Modifier.weight(1f).padding(horizontal=12.dp),horizontalAlignment=Alignment.Start) {
                    Text(t.title.ifEmpty { t.fileName.ifEmpty { LinkPlan.host(t.url) } },fontSize=16.sp,maxLines=2,overflow=TextOverflow.Ellipsis,textAlign=TextAlign.Start,style=MaterialTheme.typography.bodyLarge.copy(textDirection=TextDirection.Ltr))
                    Text(LinkPlan.host(t.url),fontSize=13.sp,lineHeight=18.sp,color=c.onSurfaceVariant,maxLines=1,overflow=TextOverflow.Ellipsis)
                }
                PreviewImage(row.thumbnail ?: t.uri.takeIf { it.isNotBlank() },t,Modifier.width(88.dp).height(64.dp))
            }
            Row(Modifier.fillMaxWidth().padding(top=8.dp),verticalAlignment=Alignment.CenterVertically) {
                val fraction = TransferProgress.fraction(t)
                Text(fraction?.let { "${(it*100).toInt()}%" } ?: "—",fontSize=20.sp,fontWeight=FontWeight.Medium,modifier=Modifier.width(52.dp))
                if(fraction == null)LinearProgressIndicator(Modifier.weight(1f).height(8.dp).clip(CircleShape),color=c.primary,trackColor=c.onSurfaceVariant.copy(alpha=.2f))
                else LinearProgressIndicator(progress={fraction},modifier=Modifier.weight(1f).height(8.dp).clip(CircleShape),color=c.primary,trackColor=c.onSurfaceVariant.copy(alpha=.2f))
            }
            val parts=row.stats.split(" · ");val speed=parts.firstOrNull { it.endsWith("/s") }.orEmpty();val eta=parts.drop(1).firstOrNull { !it.endsWith("/s") }.orEmpty()
            Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) { Text(speed,fontSize=13.sp,lineHeight=18.sp,color=c.onSurfaceVariant);Text(eta.ifEmpty { if(t.state!=TaskState.DOWNLOADING)MobileText.state(LocalContext.current,t) else "" },fontSize=13.sp,lineHeight=18.sp,color=c.onSurfaceVariant) }
            Text(parts.firstOrNull().orEmpty(),Modifier.fillMaxWidth(),fontSize=13.sp,lineHeight=18.sp,color=c.onSurfaceVariant,textAlign=TextAlign.Start,style=MaterialTheme.typography.bodyMedium.copy(textDirection=TextDirection.Ltr))
        }
    }
}
@Composable private fun PreviewImage(source:String?,task:MobileTask,modifier:Modifier) {
    val context=LocalContext.current
    val bitmap by produceState<ImageBitmap?>(null,source) {
        value=withContext(Dispatchers.IO) { runCatching {
            val uri=source?.let(Uri::parse) ?: return@runCatching null
            val bytes=when(uri.scheme) {
                "file","content" -> context.contentResolver.openInputStream(uri)?.use { it.readBytesBounded(8*1024*1024) }
                "https","http" -> SafeHttp.open(uri.toString()).let { connection -> try { SafeHttp.requireSuccess(connection.responseCode);connection.inputStream.use { it.readBytesBounded() } } finally { connection.disconnect() } }
                else -> null
            } ?: return@runCatching null
            val options=BitmapFactory.Options().apply { inJustDecodeBounds=true };BitmapFactory.decodeByteArray(bytes,0,bytes.size,options)
            if(options.outWidth<=0 || options.outHeight<=0 || options.outWidth>8192 || options.outHeight>8192)return@runCatching null
            options.inJustDecodeBounds=false;options.inSampleSize=(maxOf(options.outWidth,options.outHeight)/256).coerceAtLeast(1)
            BitmapFactory.decodeByteArray(bytes,0,bytes.size,options)?.asImageBitmap()
        }.getOrNull() }
    }
    Box(modifier.clip(RoundedCornerShape(8.dp)).background(MaterialTheme.colorScheme.background),contentAlignment=Alignment.Center) {
        if(bitmap!=null)Image(bitmap!!,null,Modifier.fillMaxSize().semantics { testTag="download-preview" },contentScale=ContentScale.Crop) else Icon(painterResource(kindIcon(task)),null,Modifier.size(30.dp),tint=MaterialTheme.colorScheme.primary)
    }
}
private fun java.io.InputStream.readBytesBounded(limit:Int=2*1024*1024):ByteArray {
    val output=java.io.ByteArrayOutputStream();val buffer=ByteArray(8192);var count=0
    while(true){val n=read(buffer);if(n<0)break;count+=n;require(count<=limit);output.write(buffer,0,n)}
    return output.toByteArray()
}
@Composable private fun NetworkStatus() {
    val context=LocalContext.current;val c=MaterialTheme.colorScheme
    val cm=context.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
    val caps=cm.getNetworkCapabilities(cm.activeNetwork)
    val wifi=caps?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)==true
    CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
    Row(Modifier.fillMaxWidth().padding(horizontal=16.dp,vertical=8.dp),verticalAlignment=Alignment.CenterVertically) {
        Icon(painterResource(if(caps==null)R.drawable.ui_wifi_off else R.drawable.ui_wifi),null,Modifier.size(22.dp),tint=c.onSurfaceVariant)
        Text(stringResource(if(caps==null)R.string.network_unavailable else if(wifi)R.string.connected_wifi else R.string.connected_mobile),Modifier.padding(start=8.dp),color=c.onSurfaceVariant,fontSize=13.sp,lineHeight=18.sp)
    }
    }
}

private sealed interface QualityProbeState {
    data object Idle : QualityProbeState
    data object Loading : QualityProbeState
    data class Ready(val info: LinkInfo) : QualityProbeState
    data class Failed(val code: String) : QualityProbeState
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun AbEnterUrl(prefill:String,defaultAudio:Boolean,onClose:()->Unit,onPaste:()->String,onDownload:(String,Boolean,Int?)->Boolean,folder:String="Downloads/Ratatoskr",onFolder:()->Unit={},onSubmit:((String,Boolean,Int?,IntakeOptions)->Boolean)?=null,groups:List<String> = emptyList(),previousTasks:List<MobileTask> = emptyList()) {
    var text by rememberSaveable(prefill) { mutableStateOf(prefill) };var audio by rememberSaveable { mutableStateOf(defaultAudio) }
    var more by rememberSaveable { mutableStateOf(defaultAudio) };var invalid by rememberSaveable { mutableStateOf(false) }
    val urls=remember(text){LinkPlan.parse(text)};val counts=remember(urls){LinkPlan.summarize(urls)};val c=MaterialTheme.colorScheme
    val activity = LocalContext.current as MobileActivity
    val defaultHeight = MobilePreferences(activity).defaultHeight
    var qualityHeight by rememberSaveable(prefill) { mutableStateOf<Int?>(defaultHeight) }
    val probeUrl = urls.singleOrNull()?.takeIf { LinkPlan.classify(it) == LinkKind.MEDIA }
    val hasMediaLinks = urls.any { LinkPlan.classify(it) == LinkKind.MEDIA }
    val qualityProbe by produceState<QualityProbeState>(QualityProbeState.Idle, probeUrl) {
        if (probeUrl == null) { value = QualityProbeState.Idle; return@produceState }
        kotlinx.coroutines.delay(350)
        if (!LinkUtils.isPublicHttpUrl(probeUrl)) { value = QualityProbeState.Idle; return@produceState }
        value = QualityProbeState.Loading
        val processId = "quality-${java.util.UUID.randomUUID()}"
        value = try {
            QualityProbeState.Ready(withContext(Dispatchers.IO) { Engine.probe(activity.applicationContext, probeUrl, processId) })
        } catch (error: kotlinx.coroutines.CancellationException) {
            Engine.cancel(processId); throw error
        } catch (error: Exception) {
            QualityProbeState.Failed(DownloadService.errorCode(error))
        }
    }
    LaunchedEffect(probeUrl) {
        qualityHeight = defaultHeight
        audio = defaultAudio
    }
    var queueDialog by rememberSaveable { mutableStateOf(false) }
    var group by rememberSaveable { mutableStateOf("") }; var startAt by rememberSaveable { mutableLongStateOf(0) }
    var groupMenu by remember { mutableStateOf(false) }
    val destinationTree = MobilePreferences(activity).saveTree
    val folderFiles by produceState<FolderFileInspection?>(null, urls, destinationTree) {
        value = null
        if (urls.isNotEmpty()) {
            kotlinx.coroutines.delay(350)
            value = withContext(Dispatchers.IO) { ExistingFiles.inspect(activity, urls) }
        }
    }
    fun submit(mode: IntakeMode): Boolean {
        val options = IntakeOptions(mode, if (mode in setOf(IntakeMode.SCHEDULE, IntakeMode.QUEUE)) startAt else 0, if (mode == IntakeMode.QUEUE) group.trim() else "")
        invalid = runCatching { options.validate(); if (onSubmit != null) !onSubmit(text, audio, qualityHeight, options) else !onDownload(text, audio, qualityHeight) }.getOrDefault(true)
        return !invalid
    }
    val focus=remember { FocusRequester() }
    ModalBottomSheet(sheetState=rememberModalBottomSheetState(skipPartiallyExpanded=true),onDismissRequest=onClose,containerColor=c.surface,shape=RoundedCornerShape(topStart=28.dp,topEnd=28.dp),dragHandle={BottomSheetDefaults.DragHandle()},contentWindowInsets={WindowInsets(0,0,0,0)}) {
        LaunchedEffect(Unit){if(prefill.isEmpty())focus.requestFocus()}
        Column(Modifier.fillMaxWidth().imePadding().verticalScroll(rememberScrollState()).padding(horizontal=16.dp).navigationBarsPadding().padding(bottom=20.dp)) {
            Row(Modifier.fillMaxWidth().padding(bottom=16.dp),verticalAlignment=Alignment.CenterVertically) {
                Text(stringResource(R.string.new_download),Modifier.weight(1f),fontSize=24.sp,fontWeight=FontWeight.Bold)
                UiIcon(R.drawable.ui_x,R.string.cancel,onClose)
            }
            Text(stringResource(R.string.link_label),Modifier.fillMaxWidth().padding(bottom=6.dp),fontSize=14.sp,color=c.onSurfaceVariant,textAlign=TextAlign.Start)
            CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
                OutlinedTextField(text,{text=it;invalid=false},Modifier.fillMaxWidth().focusRequester(focus),maxLines=3,singleLine=urls.size<=1,isError=invalid,shape=Round,
                    placeholder={Text("https://",fontSize=14.sp)},textStyle=MaterialTheme.typography.bodyLarge.copy(textDirection=TextDirection.Ltr),
                    leadingIcon={TextButton(onClick={val pasted=onPaste();if(pasted.isNotBlank())text=if(text.isBlank())pasted else "$text\n$pasted"}) { Icon(painterResource(R.drawable.ui_clipboard),stringResource(R.string.paste_clipboard),Modifier.size(18.dp));Spacer(Modifier.width(6.dp));Text(stringResource(R.string.paste_short),fontSize=12.sp) } })
            }
            if(invalid && (urls.isEmpty() || urls.any { !LinkUtils.isPublicHttpUrl(it) }))Text(stringResource(R.string.bad_link),color=c.error,fontSize=13.sp)
            // Editing and pasting can temporarily produce incomplete or invalid URLs.
            // They must never throw during composition and leave the modal unresponsive.
            val previous = remember(urls, previousTasks) { val ids = urls.mapNotNull { runCatching { LinkUtils.contentIdentity(it) }.getOrNull() }.toSet(); previousTasks.filter { runCatching { LinkUtils.contentIdentity(it.url) in ids }.getOrDefault(false) }.sortedByDescending { it.createdAt } }
            if (previous.isNotEmpty()) Surface(Modifier.fillMaxWidth().padding(top=12.dp), shape=Round, color=c.primary.copy(alpha=.08f), border=BorderStroke(1.dp,c.primary.copy(alpha=.35f))) {
                Column(Modifier.padding(14.dp)) {
                    Text(stringResource(if(previous.any { it.state==TaskState.COMPLETED }) R.string.duplicate_done else R.string.duplicate_pending), fontWeight=FontWeight.SemiBold, color=c.primary)
                    previous.take(2).forEach { Text(TaskQuery.name(it) + " · " + MobileText.state(activity,it), fontSize=13.sp, maxLines=2, overflow=TextOverflow.Ellipsis) }
                    Text(stringResource(R.string.duplicate_history_hint),fontSize=12.sp,color=c.onSurfaceVariant)
                }
            }
            folderFiles?.let { result ->
                if (result.matches.isNotEmpty()) Surface(Modifier.fillMaxWidth().padding(top = 12.dp), shape = Round, color = c.surfaceVariant) {
                    Column(Modifier.padding(14.dp)) {
                        Text(stringResource(R.string.folder_existing), fontWeight = FontWeight.SemiBold)
                        result.matches.take(4).forEach { Text(it, fontSize = 13.sp, maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall.copy(textDirection = TextDirection.Ltr)) }
                        Text(stringResource(R.string.folder_name_hint), fontSize = 12.sp, color = c.onSurfaceVariant)
                    }
                }
                if (result.limited) Text(stringResource(R.string.folder_access_limited), Modifier.padding(top = 8.dp), fontSize = 12.sp, color = c.onSurfaceVariant)
            }
            if(urls.isNotEmpty()) {
                val name=Uri.parse(urls.first()).lastPathSegment?.takeIf { it.contains('.') }.orEmpty()
                Surface(Modifier.fillMaxWidth().padding(top=12.dp),shape=Round,color=c.surface,border=BorderStroke(1.dp,c.outline.copy(alpha=c.outline.alpha*.8f))) {
                    CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
                        Row(Modifier.padding(16.dp),verticalAlignment=Alignment.CenterVertically) {
                            val type=name.substringAfterLast('.',"").uppercase(java.util.Locale.ROOT)
                            Icon(painterResource(if(type in listOf("ZIP","RAR","7Z"))R.drawable.ui_file_archive else R.drawable.ui_file_text),null,Modifier.size(32.dp),tint=c.primary)
                            Column(Modifier.padding(start=16.dp)) {
                                if(name.isNotEmpty())Text(name,fontSize=16.sp)
                                Text(stringResource(R.string.links_summary,urls.size,counts.files,counts.media),fontSize=12.sp,color=c.onSurfaceVariant)
                            }
                        }
                    }
                }
            }
            if (hasMediaLinks) {
                val info = (qualityProbe as? QualityProbeState.Ready)?.info
                val currentHeight = qualityHeight
                val choices = (info?.takeIf { it.items.size == 1 }?.heights?.takeIf { it.isNotEmpty() } ?: listOf(1080, 720, 480, 360)).distinct().toMutableList()
                if (currentHeight != null && currentHeight !in choices) choices.add(0, currentHeight)
                fun labelWithSize(label: String, bytes: Long?): String = if (bytes != null) activity.getString(R.string.quality_with_size, label, Format.bytes(bytes)) else label
                Surface(Modifier.fillMaxWidth().padding(top = 12.dp), shape = Round, color = c.surfaceVariant.copy(alpha = .48f), border = BorderStroke(1.dp, c.outline.copy(alpha = .45f))) {
                    Column(Modifier.padding(14.dp)) {
                        Text(stringResource(R.string.choose_quality), fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                        if (info?.hasVideo == false) {
                            Text(stringResource(R.string.quality_not_needed), Modifier.padding(top = 8.dp), fontSize = 13.sp, color = c.onSurfaceVariant)
                        } else {
                            Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                FilterChip(selected = currentHeight == null && !audio, onClick = { qualityHeight = null; audio = false }, label = { Text(labelWithSize(activity.getString(R.string.best_quality), info?.qualities?.firstOrNull()?.bytes)) })
                                choices.forEach { height ->
                                    val bytes = info?.qualities?.firstOrNull { it.height == height }?.bytes
                                    val heightLabel = activity.getString(R.string.video_height, height)
                                    FilterChip(selected = currentHeight == height && !audio, onClick = { qualityHeight = height; audio = false }, label = { Text(labelWithSize(heightLabel, bytes)) })
                                }
                                FilterChip(selected = audio, onClick = { audio = true }, label = { Text(labelWithSize(activity.getString(R.string.audio_only), info?.audioBytes)) })
                            }
                        }
                        val probeMessage = when (val state = qualityProbe) {
                            QualityProbeState.Loading -> stringResource(R.string.quality_checking)
                            is QualityProbeState.Failed -> stringResource(if (state.code == "auth_required") R.string.error_private else R.string.quality_check_fallback)
                            QualityProbeState.Idle -> stringResource(R.string.quality_batch_hint)
                            is QualityProbeState.Ready -> when {
                                !state.info.hasVideo -> stringResource(R.string.quality_not_needed)
                                state.info.items.size > 1 -> stringResource(R.string.quality_batch_hint)
                                state.info.qualities.isNotEmpty() -> stringResource(R.string.quality_available)
                                else -> stringResource(R.string.quality_check_fallback)
                            }
                        }
                        Text(probeMessage, Modifier.padding(top = 6.dp), fontSize = 12.sp, color = c.onSurfaceVariant)
                    }
                }
            }
            Surface(Modifier.fillMaxWidth().padding(top=12.dp).clickable(onClick=onFolder),shape=Round,color=c.surface,border=BorderStroke(1.dp,c.outline.copy(alpha=c.outline.alpha*.8f))) {
                Row(Modifier.padding(14.dp),verticalAlignment=Alignment.CenterVertically) {
                    Icon(painterResource(R.drawable.ui_folder),null,Modifier.size(24.dp));Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) { Text(stringResource(R.string.save_location),fontSize=14.sp);Text(folder,fontSize=13.sp,lineHeight=18.sp,color=c.onSurfaceVariant,maxLines=1,overflow=TextOverflow.Ellipsis) }
                    Icon(painterResource(R.drawable.ui_chevron_right),null,Modifier.size(22.dp))
                }
            }
            Surface(Modifier.fillMaxWidth().padding(top=12.dp).clickable{more=!more}.semantics { stateDescription=if(more)"expanded" else "collapsed" },shape=Round,color=c.surface,border=BorderStroke(1.dp,c.outline.copy(alpha=c.outline.alpha*.8f))) {
                Row(Modifier.padding(horizontal=14.dp).heightIn(min=52.dp),verticalAlignment=Alignment.CenterVertically) {
                    Icon(painterResource(R.drawable.ui_settings),null,Modifier.size(20.dp),tint=c.primary); Spacer(Modifier.width(12.dp)); Column(Modifier.weight(1f)) { Text(stringResource(R.string.more_options),fontSize=15.sp); Text(stringResource(R.string.intake_settings_hint),fontSize=12.sp,color=c.onSurfaceVariant) };Icon(painterResource(R.drawable.ui_chevron_down),null,Modifier.size(20.dp))
                }
            }
            AnimatedVisibility(more) { Column {
                OutlinedButton(onClick = { MobileDates.choose(activity, if (startAt > System.currentTimeMillis()) startAt else System.currentTimeMillis() + 3_600_000) { startAt = it; invalid = false } }, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp), shape = Round) {
                    Icon(painterResource(R.drawable.ui_clock), null, Modifier.size(20.dp)); Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.choose_date_time))
                }
                Text(stringResource(R.string.pattern_hint),fontSize=12.sp,color=c.onSurfaceVariant)
            } }
            Text(stringResource(R.string.automatic_hint),Modifier.fillMaxWidth().padding(top=12.dp,bottom=20.dp),fontSize=12.sp,color=c.onSurfaceVariant,textAlign=TextAlign.Start)
            if (startAt > 0) Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.scheduled_for, MobileDates.format(activity, startAt)), Modifier.weight(1f), color = c.primary, fontSize = 13.sp)
                UiIcon(R.drawable.ui_x, R.string.schedule_clear) { startAt = 0; invalid = false }
            }
            if (invalid && startAt > 0 && startAt <= System.currentTimeMillis()) Text(stringResource(R.string.schedule_invalid), color = c.error, fontSize = 13.sp)
            CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                    OutlinedButton(onClick = { if (urls.isEmpty() || urls.any { !LinkUtils.isPublicHttpUrl(it) }) invalid = true else queueDialog = true }, enabled = text.isNotBlank(), modifier = Modifier.weight(1f).heightIn(min = 56.dp), shape = Round) {
                        Icon(painterResource(R.drawable.ui_list), null, Modifier.size(20.dp)); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.queue_action))
                    }
                    FilledIconButton(onClick = { submit(if (startAt > 0) IntakeMode.SCHEDULE else IntakeMode.NOW) }, enabled = text.isNotBlank(), modifier = Modifier.size(64.dp).then(if (previous.isNotEmpty()) Modifier.border(3.dp, c.error, CircleShape) else Modifier), shape = CircleShape,
                        colors = IconButtonDefaults.filledIconButtonColors(containerColor = if (previous.isNotEmpty()) c.error else Gold, contentColor = if (previous.isNotEmpty()) Color.White else Ink)) {
                        Icon(painterResource(R.drawable.ui_download), stringResource(R.string.download_action), Modifier.size(30.dp))
                    }
                    OutlinedButton(onClick = { submit(IntakeMode.SAVE) }, enabled = text.isNotBlank(), modifier = Modifier.weight(1f).heightIn(min = 56.dp), shape = Round) {
                        Icon(painterResource(R.drawable.ui_plus), null, Modifier.size(20.dp)); Spacer(Modifier.width(8.dp)); Text(stringResource(R.string.add_to_list))
                    }
                }
            }
        }
    }
    if (queueDialog) AlertDialog(onDismissRequest = { queueDialog = false },
        title = { Text(stringResource(R.string.select_queue)) },
        text = {
            Column {
                if (groups.isNotEmpty()) Box {
                    TextButton(onClick = { groupMenu = true }) { Text(stringResource(R.string.existing_queues)); Icon(painterResource(R.drawable.ui_chevron_down), null, Modifier.size(20.dp)) }
                    DropdownMenu(groupMenu, { groupMenu = false }) {
                        groups.forEach { name -> DropdownMenuItem(text = { Text(name) }, onClick = { group = name; groupMenu = false }) }
                    }
                }
                OutlinedTextField(group, { group = it.take(80); invalid = false }, Modifier.fillMaxWidth(), singleLine = true, shape = Round,
                    label = { Text(stringResource(R.string.group_name)) }, placeholder = { Text(stringResource(R.string.group_hint)) }, isError = invalid && group.isBlank())
                Text(stringResource(R.string.queue_intake_hint), Modifier.padding(top = 8.dp), fontSize = 13.sp, color = c.onSurfaceVariant)
                if (invalid && group.isBlank()) Text(stringResource(R.string.group_required), color = c.error, fontSize = 13.sp)
                if (startAt > 0) Text(stringResource(R.string.scheduled_for, MobileDates.format(activity, startAt)), fontSize = 13.sp, color = c.primary)
            }
        },
        confirmButton = { TextButton(onClick = { if (submit(IntakeMode.QUEUE)) queueDialog = false }) { Text(stringResource(R.string.add_queue)) } },
        dismissButton = { TextButton(onClick = { queueDialog = false }) { Text(stringResource(R.string.cancel)) } }, containerColor = c.surface)
}

