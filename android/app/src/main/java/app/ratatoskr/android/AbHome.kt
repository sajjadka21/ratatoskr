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
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
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
    val clipboard: String = "", val category: String? = null, val filtersActive: Boolean = false,
    val namedQueues: List<String> = emptyList(),
)

@Composable
fun RatatoskrTheme(activity: MobileActivity, content: @Composable () -> Unit) {
    val colors = if (activity.dark) darkColorScheme() else lightColorScheme()
    val family = FontFamily(Font(R.font.vazirmatn_regular), Font(R.font.vazirmatn_semibold, FontWeight.SemiBold), Font(R.font.vazirmatn_bold, FontWeight.Bold))
    val typography = Typography(
        bodyLarge = TextStyle(fontFamily = family, fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.sp),
        bodyMedium = TextStyle(fontFamily = family, fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.sp),
        bodySmall = TextStyle(fontFamily = family, fontSize = 12.sp, lineHeight = 18.sp, letterSpacing = 0.sp),
        labelLarge = TextStyle(fontFamily = family, fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.sp, fontWeight = FontWeight.Medium),
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
                IconButton(onClick=onBack,modifier=Modifier.size(48.dp)) { Icon(ABDMIcons.Back,stringResource(R.string.cancel),Modifier.size(24.dp)) }
                Text(title, Modifier.padding(start = 16.dp), fontWeight = FontWeight.Bold, fontSize = 20.sp)
            }
        }, footer = {}) { padding ->
            AndroidView(factory = { scroll.apply { (parent as? android.view.ViewGroup)?.removeView(this); isVerticalScrollBarEnabled = false } },
                modifier = Modifier.fillMaxSize().padding(top = padding.calculateTopPadding()))
        }
    }
}
