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
import org.junit.Assert.assertEquals
import org.hamcrest.Description
import org.hamcrest.Matcher
import org.hamcrest.Matchers.anything
import org.hamcrest.TypeSafeMatcher

/** Opens the real screens on an emulator. Every click and typed key also runs the accessibility checks
 * (labels, touch-target size, contrast) over the whole screen, so a regression there fails the build. */
class SmokeTest {
    companion object {
        @JvmStatic @BeforeClass fun accessibility() {
            AccessibilityChecks.enable().setRunChecksFromRootView(true)
        }
    }

    private val context: Context get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Before fun quietApp() {
        // No first-run dialog, no network look-ups and no clipboard banner while the test drives the screen.
        MobilePreferences(context).apply { onboarded = true; autoUpdateCheck = false; watchClipboard = false }
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

    @Test fun homeScreenShowsTheBrandTheTabsAndTheAddButton() {
        ActivityScenario.launch(MainActivity::class.java).use {
            onView(withText("Ratatoskr")).check(matches(isDisplayed()))
            onView(withText(R.string.add_links)).check(matches(isDisplayed()))
            onView(withText(R.string.empty_jobs)).check(matches(isDisplayed()))
            screenshot("home-empty")
        }
    }

    @Test fun addingLinksCountsFilesAndVideosAsYouType() {
        ActivityScenario.launch(MainActivity::class.java).use {
            onView(withText(R.string.add_links)).perform(click())
            onView(isAssignableFrom(EditText::class.java)).perform(typeText("https://example.org/p[01-03].jpg https://youtu.be/abc"), closeSoftKeyboard())
            onView(withText(context.getString(R.string.links_summary, 4, 3, 1))).check(matches(isDisplayed()))
        }
    }

    @Test fun settingsOpensFromTheMenu() {
        ActivityScenario.launch(MainActivity::class.java).use {
            onView(withContentDescription(R.string.menu)).perform(click())
            onView(withText(R.string.settings)).inRoot(isPlatformPopup()).perform(click())
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

    private fun screenshot(name: String) {
        require(name.matches(Regex("[a-z-]+")))
        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
        automation.executeShellCommand("mkdir -p /sdcard/Download/ratatoskr-ui-review && screencap -p /sdcard/Download/ratatoskr-ui-review/$name.png").use {
            android.os.ParcelFileDescriptor.AutoCloseInputStream(it).use { stream -> stream.readBytes() }
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
            onView(withContentDescription(R.string.menu)).perform(click())
            onView(withText(R.string.browser)).inRoot(isPlatformPopup()).perform(click())
            onView(withText(R.string.download_page)).check(matches(isDisplayed()))
        }
    }
}
