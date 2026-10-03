package app.ratatoskr.android

import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.util.UUID

/** Exercise the installed production intake, not a mock of its submit callback. */
class IntakeUiTest {
    @get:Rule val compose = createEmptyComposeRule()
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private fun prepare() {
        MobilePreferences(context).apply { onboarded=true; autoUpdateCheck=false; watchClipboard=false; language="en"; mode="dark" }
        val permission = when {
            android.os.Build.VERSION.SDK_INT >= 33 -> android.Manifest.permission.POST_NOTIFICATIONS
            android.os.Build.VERSION.SDK_INT < 29 -> android.Manifest.permission.WRITE_EXTERNAL_STORAGE
            else -> null
        }
        if (permission != null) InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("pm grant ${context.packageName} $permission").close()
    }
    private fun capture(name: String) {
        compose.waitForIdle()
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        for (command in listOf("mkdir -p /sdcard/Download/ratatoskr-ui-review", "screencap -p /sdcard/Download/ratatoskr-ui-review/$name.png"))
            automation.executeShellCommand(command).use { android.os.ParcelFileDescriptor.AutoCloseInputStream(it).use { stream -> stream.readBytes() } }
    }
    @Test fun pasteCanSaveWithoutProbeAndNamedQueueSurvivesRecreation() {
        prepare()
        val url = "https://example.com/series-${UUID.randomUUID()}.mp4"
        val group = "Series ${UUID.randomUUID().toString().take(6)}"
        ActivityScenario.launch<MainActivity>(Intent(context, MainActivity::class.java).putExtra(Intent.EXTRA_TEXT, url)).use { scenario ->
            compose.onNodeWithText("Queue").performScrollTo().performClick()
            compose.onNodeWithText("Group / series name").performTextInput(group)
            compose.onNodeWithText("Add to queue").performClick()
            compose.waitUntil(10000) { TaskStore.get(context).list().any { it.url == url } }
            val task = TaskStore.get(context).list().first { it.url == url }
            try {
                assertEquals(TaskState.SAVED, task.state); assertEquals(group, task.groupName)
                assertFalse(MobileRuntime.busy(task.id))
                compose.onNodeWithText("$group (1)").assertIsDisplayed()
                capture("named-queue-local-review")
                scenario.recreate()
                compose.onNodeWithText("$group (1)").assertIsDisplayed()
                assertEquals(TaskState.SAVED, TaskStore.get(context).get(task.id)!!.state)
            } finally { TaskStore.get(context).remove(task.id) }
        }
    }
    @Test fun schedulingOffersBothCalendarsAndNeverStartsWhileChoosing() {
        prepare()
        val url = "https://example.com/scheduled-${UUID.randomUUID()}.zip"
        ActivityScenario.launch<MainActivity>(Intent(context, MainActivity::class.java).putExtra(Intent.EXTRA_TEXT, url)).use {
            compose.onNodeWithText(context.getString(R.string.more_options)).performScrollTo().performClick()
            compose.onNodeWithText("Choose date and time").performScrollTo().performClick()
            androidx.test.espresso.Espresso.onView(androidx.test.espresso.matcher.ViewMatchers.withText("Persian (Jalali)"))
                .check(androidx.test.espresso.assertion.ViewAssertions.matches(androidx.test.espresso.matcher.ViewMatchers.isDisplayed()))
            androidx.test.espresso.Espresso.onView(androidx.test.espresso.matcher.ViewMatchers.withText("Gregorian"))
                .check(androidx.test.espresso.assertion.ViewAssertions.matches(androidx.test.espresso.matcher.ViewMatchers.isDisplayed()))
            capture("scheduling-calendar-local-review")
            assertFalse(TaskStore.get(context).list().any { it.url == url })
        }
    }
}
