package app.ratatoskr.android

import android.content.Context
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import java.io.File

/** Real production composables with declared sample data, for an apples-to-apples mockup review.
 * No fixture task or image is included in the installed production APK. */
class FaithfulDesignTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val context:Context get()=InstrumentationRegistry.getInstrumentation().targetContext
    private val actions=object:TaskActions {
        override val selecting=false
        override fun toggle(task:MobileTask){};override fun details(task:MobileTask){}
        override fun command(task:MobileTask,action:String){};override fun open(task:MobileTask){}
        override fun share(task:MobileTask){};override fun schedule(task:MobileTask){};override fun remove(task:MobileTask){}
    }
    private fun sample():List<TaskRow> {
        val image=File(context.cacheDir,"design-review-thumbnail.png")
        InstrumentationRegistry.getInstrumentation().context.assets.open("design-review-thumbnail.png").use { input -> image.outputStream().use { input.copyTo(it) } }
        fun task(id:String,name:String,state:TaskState,size:Long)=MobileTask(id,"https://example.com/${android.net.Uri.encode(name)}",name,state,kind="file",totalBytes=size,fileName=name)
        return listOf(
            TaskRow(task("review-video","Design course.mp4",TaskState.DOWNLOADING,1073741824).copy(progress=64,bytesDone=687194767,mime="video/mp4"),"640 MB / 1 GB · 8.4 MB/s · ۲ دقیقه باقی مانده","",thumbnail=android.net.Uri.fromFile(image).toString()),
            TaskRow(task("review-archive","Android Studio.zip",TaskState.PAUSED,1073741824).copy(bytesDone=268435456,progress=25),"256 MB / 1 GB",""),
            TaskRow(task("review-document","Brand guidelines.pdf",TaskState.QUEUED,12582912),"12 MB",""),
            TaskRow(task("review-complete","Portfolio.pdf",TaskState.COMPLETED,18874368).copy(bytesDone=18874368),"18 MB",""))
    }
    private fun capture(name:String) {
        compose.waitForIdle()
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("mkdir -p /sdcard/Download/ratatoskr-ui-review").use { android.os.ParcelFileDescriptor.AutoCloseInputStream(it).use { s->s.readBytes() } }
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("screencap -p /sdcard/Download/ratatoskr-ui-review/$name.png").use { android.os.ParcelFileDescriptor.AutoCloseInputStream(it).use { s->s.readBytes() } }
    }
    @Test fun approvedDesignDarkLightAndIntake() {
        val permission=when {
            android.os.Build.VERSION.SDK_INT>=33 -> android.Manifest.permission.POST_NOTIFICATIONS
            android.os.Build.VERSION.SDK_INT<29 -> android.Manifest.permission.WRITE_EXTERNAL_STORAGE
            else -> null
        }
        if(permission!=null)InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("pm grant ${context.packageName} $permission").use { android.os.ParcelFileDescriptor.AutoCloseInputStream(it).use { stream->stream.readBytes() } }
        val prefs=MobilePreferences(context)
        prefs.onboarded=true;prefs.watchClipboard=false;prefs.autoUpdateCheck=false;prefs.language="fa";prefs.brand="ember-forge"
        try {
            for(mode in listOf("dark","light")) {
                prefs.mode=mode
                ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                    var addLabel="";var downloadLabel="";var openLabel=""
                    compose.onNodeWithText("Ratatoskr").assertIsDisplayed()
                    compose.waitUntil(10000) { runCatching { compose.onAllNodesWithText("دانلودها",substring=true).fetchSemanticsNodes().isNotEmpty() }.getOrDefault(false) }
                    compose.waitForIdle()
                    scenario.onActivity { activity ->
                        addLabel=activity.getString(R.string.add_links);downloadLabel=activity.getString(R.string.download_action);openLabel=activity.getString(R.string.open_file)
                        val rows=sample()
                        activity.setContentView(ComposeView(activity).apply {
                            setContent { RatatoskrTheme(activity) {
                                var intake by remember { mutableStateOf(false) }
                                Surface(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.systemBars),color=androidx.compose.material3.MaterialTheme.colorScheme.background) {
                                    AbHome(AbHomeState(rows=rows),actions,onAdd={intake=true},onQuery={},onHistory={},onMenu={},onCategory={},categories=emptyList(),onSort={},onClipboard={},onSelection={})
                                    if(intake)AbEnterUrl("https://example.com/design.zip",false,onClose={intake=false},onPaste={""},onDownload={_,_->true})
                                }
                            } }
                        },android.view.ViewGroup.LayoutParams(-1,-1))
                    }
                    compose.waitUntil(10000) { runCatching { compose.onAllNodesWithText("Design course.mp4").fetchSemanticsNodes().isNotEmpty() }.getOrDefault(false) }
                    compose.onNodeWithText("Design course.mp4").assertIsDisplayed()
                    compose.onNodeWithText("Portfolio.pdf").assertIsDisplayed()
                    compose.onNodeWithContentDescription(openLabel).assertIsDisplayed()
                    compose.waitUntil(5000) { compose.onAllNodesWithTag("download-preview",useUnmergedTree=true).fetchSemanticsNodes().isNotEmpty() }
                    capture("approved-home-$mode-fa")
                    if(mode=="dark") {
                        compose.onNodeWithContentDescription(addLabel).performClick()
                        compose.onNodeWithText(downloadLabel).assertIsDisplayed()
                        capture("approved-intake-dark-fa")
                    }
                }
            }
        } finally { prefs.language="";prefs.mode="light" }
    }
}
