package app.ratatoskr.android

import android.content.Context
import android.widget.EditText
import androidx.test.core.app.ActivityScenario
import androidx.test.espresso.Espresso.onView
import androidx.test.espresso.action.ViewActions.click
import androidx.test.espresso.action.ViewActions.closeSoftKeyboard
import androidx.test.espresso.action.ViewActions.typeText
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
    }

    @Test fun homeScreenShowsTheBrandTheTabsAndTheAddButton() {
        ActivityScenario.launch(MainActivity::class.java).use {
            onView(withText("Ratatoskr")).check(matches(isDisplayed()))
            onView(withText(R.string.add_links)).check(matches(isDisplayed()))
            onView(withText(R.string.empty_jobs)).check(matches(isDisplayed()))
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

    @Test fun theBrowserOpensFromTheMenu() {
        ActivityScenario.launch(MainActivity::class.java).use {
            onView(withContentDescription(R.string.menu)).perform(click())
            onView(withText(R.string.browser)).inRoot(isPlatformPopup()).perform(click())
            onView(withText(R.string.download_page)).check(matches(isDisplayed()))
        }
    }
}
