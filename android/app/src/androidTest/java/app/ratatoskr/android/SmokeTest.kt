package app.ratatoskr.android

import android.content.Context
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.Spinner
import android.widget.TextView
import androidx.test.core.app.ActivityScenario
import androidx.test.espresso.Espresso.onData
import androidx.test.espresso.Espresso.onView
import androidx.test.espresso.action.ViewActions.click
import androidx.test.espresso.action.ViewActions.closeSoftKeyboard
import androidx.test.espresso.action.ViewActions.typeText
import androidx.test.espresso.action.ViewActions.scrollTo
import androidx.test.espresso.assertion.ViewAssertions.matches
import androidx.test.espresso.contrib.AccessibilityChecks
import androidx.test.espresso.matcher.RootMatchers.isPlatformPopup
import androidx.test.espresso.matcher.ViewMatchers.isAssignableFrom
import androidx.test.espresso.matcher.ViewMatchers.isDisplayed
import androidx.test.espresso.matcher.ViewMatchers.withContentDescription
import androidx.test.espresso.matcher.ViewMatchers.withText
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Test
import org.junit.Rule
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import org.junit.Assert.assertEquals
import org.hamcrest.Description
import org.hamcrest.Matcher
import org.hamcrest.Matchers.anything
import org.hamcrest.TypeSafeMatcher

/** Opens the real screens on an emulator. Every click and typed key also runs the accessibility checks
 * (labels, touch-target size, contrast) over the whole screen, so a regression there fails the build. */
class SmokeTest {
    @get:Rule val compose = createEmptyComposeRule()
    companion object {
        @JvmStatic @BeforeClass fun accessibility() {
            AccessibilityChecks.enable().setRunChecksFromRootView(true)
        }
    }

    private val context: Context get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Before fun quietApp() {
        // No first-run dialog, no network look-ups and no clipboard banner while the test drives the screen.
        MobilePreferences(context).apply { onboarded = true; autoUpdateCheck = false; watchClipboard = false; mode = "light"; brand = "ember-forge"; language = "" }
        val permission = when {
            android.os.Build.VERSION.SDK_INT >= 33 -> android.Manifest.permission.POST_NOTIFICATIONS
            android.os.Build.VERSION.SDK_INT < 29 -> android.Manifest.permission.WRITE_EXTERNAL_STORAGE
            else -> null
        }
        if (permission != null) {
            InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("pm grant ${context.packageName} $permission").use {
                android.os.ParcelFileDescriptor.AutoCloseInputStream(it).use { stream -> stream.readBytes() }
            }
        }
    }

    @Test fun homeScreenShowsTheAbToolbarAndTheAddButton() {
        ActivityScenario.launch(MainActivity::class.java).use {
            compose.onNodeWithText("Ratatoskr").assertIsDisplayed()
            it.onActivity { activity ->
                val viewport = activity.findViewById<ViewGroup>(android.R.id.content)
                assertEquals(viewport.height, viewport.getChildAt(0).height)
            }
            compose.onNodeWithContentDescription(context.getString(R.string.add_links)).assertIsDisplayed()
            compose.onNodeWithText(context.getString(R.string.empty_jobs)).assertIsDisplayed()
            screenshot("home-empty")
        }
    }

    @Test fun addingLinksCountsFilesAndVideosAsYouType() {
        ActivityScenario.launch(MainActivity::class.java).use {
            compose.onNodeWithContentDescription(context.getString(R.string.add_links)).performClick()
            compose.onNode(hasSetTextAction()).performTextInput("https://example.org/p[01-03].jpg https://youtu.be/abc")
            compose.onNodeWithText(context.getString(R.string.links_summary, 4, 3, 1)).assertIsDisplayed()
            onView(androidx.test.espresso.matcher.ViewMatchers.isRoot()).perform(closeSoftKeyboard())
            screenshot("ab-add-link-filled")
        }
    }

    @Test fun addLinkHasOneDownloadActionAndTheAbMenuOpensSettings() {
        ActivityScenario.launch(MainActivity::class.java).use {
            compose.onNodeWithContentDescription(context.getString(R.string.menu)).performClick()
            compose.onNodeWithText(context.getString(R.string.settings)).performClick()
            onView(withText(R.string.section_network)).check(matches(isDisplayed()))
            androidx.test.espresso.Espresso.pressBack()
            compose.onNodeWithContentDescription(context.getString(R.string.add_links)).performClick()
            compose.onNodeWithText(context.getString(R.string.download_action)).assertIsDisplayed()
            compose.onNodeWithText(context.getString(R.string.file_download)).assertDoesNotExist()
            screenshot("add-link-single-action")
        }
    }

    @Test fun settingsOpensFromTheMenu() {
        ActivityScenario.launch(MainActivity::class.java).use {
            compose.onNodeWithContentDescription(context.getString(R.string.menu)).performClick()
            compose.onNodeWithText(context.getString(R.string.settings)).performClick()
            onView(withText(R.string.section_network)).check(matches(isDisplayed()))
        }
    }

    /** Exercise actual dropdown clicks and framework-driven recreation. Espresso
     * waits for the newly resumed screen; needing Back/reentry leaves this test
     * unable to open and dismiss the next dialog. */
    @Test fun changingAppearanceKeepsSettingsUsableWithoutLeavingTheScreen() {
        MobilePreferences(context).apply { mode = "light"; brand = "ember-forge"; language = "" }
        ActivityScenario.launch(SettingsActivity::class.java).use {
            selectSetting(R.string.appearance, 2)
            screenshot("settings-dark")
            assertEquals("dark", MobilePreferences(context).mode)
            openAndDismissPlugins()
            selectSetting(R.string.appearance, 1)
            screenshot("settings-light")
            assertEquals("light", MobilePreferences(context).mode)
            openAndDismissPlugins()
            selectSetting(R.string.app_name, 3)
            assertEquals("frost-byte", MobilePreferences(context).brand)
            screenshot("settings-frost-byte")
            for ((position, brand) in listOf(0 to "midnight-arcane", 1 to "ember-forge", 2 to "forest-rune")) {
                selectSetting(R.string.app_name, position)
                assertEquals(brand, MobilePreferences(context).brand)
                openAndDismissPlugins()
                screenshot("settings-$brand")
            }
            openAndDismissPlugins()
            selectSetting(R.string.language, 1)
            assertEquals("fa", MobilePreferences(context).language)
            screenshot("settings-fa")
            openAndDismissPlugins()
            selectSetting(R.string.language, 2)
            assertEquals("en", MobilePreferences(context).language)
            openAndDismissPlugins()
            selectSetting(R.string.language, 0)
            assertEquals("", MobilePreferences(context).language)
            openAndDismissPlugins()
        }
    }

    @Test fun abListShowsDownloadFailureRetryAndSurvivesRecreation() {
        val prefs = MobilePreferences(context)
        val store = TaskStore.get(context)
        val queued = store.enqueue("https://example.org/ratatoskr-desktop.zip", null, false, "Ratatoskr Desktop.zip", "file")
        store.state(queued.id, TaskState.FAILED, "not_a_file")
        val completed = store.enqueue("https://example.org/guide.pdf", null, false, "Getting started.pdf", "file")
        store.state(completed.id, TaskState.COMPLETED)
        val archive = store.enqueue("https://example.org/studio.zip", null, false, "Android Studio.zip", "file")
        store.state(archive.id, TaskState.PAUSED)
        store.update(archive.id, android.content.ContentValues().apply { put("progress", 64); put("bytes_done", 671088640L); put("total_bytes", 1048576000L) })
        val document = store.enqueue("https://example.org/design.pdf", null, false, "Design guidelines.pdf", "file")
        store.state(document.id, TaskState.PAUSED)
        store.update(document.id, android.content.ContentValues().apply { put("progress", 25); put("bytes_done", 2097152L); put("total_bytes", 8388608L) })
        try {
            prefs.language = "en"; prefs.mode = "dark"
            ActivityScenario.launch(MainActivity::class.java).use { scenario ->
                compose.onNodeWithText("Ratatoskr Desktop.zip").assertIsDisplayed()
                compose.onNodeWithText(context.getString(R.string.error_not_file)).assertIsDisplayed()
                compose.onNodeWithContentDescription(context.getString(R.string.resume)).assertIsDisplayed()
                screenshot("ab-home-downloads-dark")
                compose.onNodeWithContentDescription(context.getString(R.string.search_history)).performClick()
                compose.onNode(hasSetTextAction()).performTextInput("Desktop")
                scenario.recreate()
                compose.onNodeWithText("Desktop").assertIsDisplayed()
                screenshot("ab-search-restored")
                compose.onNodeWithText("Ratatoskr Desktop.zip").assertIsDisplayed()
            }
            prefs.mode = "light"; prefs.language = "fa"
            ActivityScenario.launch(MainActivity::class.java).use {
                compose.onNodeWithText("Ratatoskr Desktop.zip").assertIsDisplayed()
                compose.onNodeWithText("فعال").assertIsDisplayed()
                it.onActivity { activity -> assertEquals(View.LAYOUT_DIRECTION_RTL, activity.findViewById<View>(android.R.id.content).layoutDirection) }
                screenshot("ab-home-downloads-light-fa")
            }
        } finally { listOf(queued, completed, archive, document).forEach { store.remove(it.id) }; prefs.language = "" }
    }

    private fun screenshot(name: String) {
        require(name.matches(Regex("[a-z-]+")))
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        for (command in listOf("mkdir -p /sdcard/Download/ratatoskr-ui-review", "screencap -p /sdcard/Download/ratatoskr-ui-review/$name.png")) {
            automation.executeShellCommand(command).use {
                android.os.ParcelFileDescriptor.AutoCloseInputStream(it).use { stream -> stream.readBytes() }
            }
        }
    }

    private fun selectSetting(title: Int, position: Int) {
        onView(spinnerFollowingLabel(title)).perform(scrollTo(), click())
        onData(anything()).atPosition(position).perform(click())
    }

    private fun openAndDismissPlugins() {
        onView(withText(R.string.open_plugins)).perform(scrollTo(), click())
        onView(withText(R.string.plugins_hint)).check(matches(isDisplayed()))
        onView(withText(android.R.string.ok)).perform(click())
    }

    private fun spinnerFollowingLabel(title: Int): Matcher<View> = object : TypeSafeMatcher<View>() {
        override fun describeTo(description: Description) { description.appendText("spinner after settings label $title") }
        override fun matchesSafely(view: View): Boolean {
            if (view !is Spinner) return false
            val parent = view.parent as? ViewGroup ?: return false
            val index = parent.indexOfChild(view)
            val label = if (index > 0) parent.getChildAt(index - 1) as? TextView else null
            return label?.text?.toString() == view.context.getString(title)
        }
    }

    @Test fun theBrowserOpensFromTheMenu() {
        ActivityScenario.launch(MainActivity::class.java).use {
            compose.onNodeWithContentDescription(context.getString(R.string.menu)).performClick()
            compose.onNodeWithText(context.getString(R.string.browser)).performClick()
            onView(withText(R.string.download_page)).check(matches(isDisplayed()))
        }
    }
}
