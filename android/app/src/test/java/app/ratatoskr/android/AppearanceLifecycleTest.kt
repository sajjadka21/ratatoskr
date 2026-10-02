package app.ratatoskr.android

import android.content.Context
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.Spinner
import android.widget.TextView
import androidx.appcompat.app.AppCompatDelegate
import androidx.core.os.LocaleListCompat
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ActivityController
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode
import org.robolectric.annotation.SQLiteMode
import org.robolectric.shadows.ShadowDialog

/** Real settings controls and lifecycle, without running downloads or update checks.
 * Duplicate selection delivery models layout/state restoration callbacks: these
 * must not start another recreation after the committed choice is restored. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
@LooperMode(LooperMode.Mode.PAUSED)
@SQLiteMode(SQLiteMode.Mode.NATIVE)
class AppearanceLifecycleTest {
    private val controllers = mutableListOf<ActivityController<out MobileActivity>>()
    private lateinit var prefs: MobilePreferences

    @Before fun preparePreferences() {
        val app = RuntimeEnvironment.getApplication()
        app.getSharedPreferences("download_preferences", Context.MODE_PRIVATE).edit().clear().commit()
        AppCompatDelegate.setDefaultNightMode(AppCompatDelegate.MODE_NIGHT_NO)
        AppCompatDelegate.setApplicationLocales(LocaleListCompat.getEmptyLocaleList())
        prefs = MobilePreferences(app).apply {
            mode = "light"; autoUpdateCheck = false; onboarded = true
        }
    }

    @After fun closeActivities() {
        controllers.asReversed().forEach { it.pause().stop().destroy() }
        controllers.clear()
        AppCompatDelegate.setDefaultNightMode(AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM)
        AppCompatDelegate.setApplicationLocales(LocaleListCompat.getEmptyLocaleList())
        RuntimeEnvironment.getApplication().getSharedPreferences("download_preferences", Context.MODE_PRIVATE).edit().clear().commit()
    }

    private fun idle() = shadowOf(Looper.getMainLooper()).idle()
    private fun views(root: View): Sequence<View> = sequence {
        yield(root)
        if (root is ViewGroup) for (index in 0 until root.childCount) yieldAll(views(root.getChildAt(index)))
    }
    private fun content(activity: MobileActivity) = activity.findViewById<View>(android.R.id.content)
    private fun spinner(activity: SettingsActivity, title: Int): Spinner {
        val label = views(content(activity)).filterIsInstance<TextView>().first { it.text.toString() == activity.getString(title) }
        val parent = label.parent as ViewGroup
        return parent.getChildAt(parent.indexOfChild(label) + 1) as Spinner
    }
    private fun settings(): ActivityController<SettingsActivity> = Robolectric.buildActivity(SettingsActivity::class.java).also {
        controllers.add(it); it.setup().visible(); idle()
    }
    private fun assertPluginsUsable(activity: SettingsActivity) {
        val button = views(content(activity)).filterIsInstance<TextView>().first { it.text.toString() == activity.getString(R.string.open_plugins) }
        assertTrue(button.performClick()); idle()
        val dialog = ShadowDialog.getLatestDialog()
        assertTrue("Settings can open a dialog after changing appearance", dialog.isShowing)
        dialog.dismiss(); idle()
        assertFalse(dialog.isShowing)
    }

    @Test fun restoringAlreadySavedAppearanceDoesNotRequestAnotherRecreation() {
        prefs.mode = "dark"; prefs.brand = "forest-rune"
        val controller = settings()
        controller.recreate().visible()
        val activity = controller.get()
        idle()
        assertTrue(activity.dark)
        assertEquals(2, spinner(activity, R.string.appearance).selectedItemPosition)
        assertEquals(2, spinner(activity, R.string.app_name).selectedItemPosition)
        assertSame("Initial spinner callbacks must not recreate the restored screen", activity, controller.get())
        assertPluginsUsable(activity)
    }

    @Test fun duplicateAppearanceCallbackDoesNotRecreateAnUnchangedScreen() {
        val controller = settings()
        val activity = controller.get()
        for (title in listOf(R.string.appearance, R.string.app_name, R.string.language)) {
            val control = spinner(activity, title)
            // The first selection was delivered during layout; repeat the same
            // position as Android can do when restoring a spinner hierarchy.
            control.onItemSelectedListener!!.onItemSelected(control, control.selectedView, control.selectedItemPosition, control.selectedItemId)
        }
        idle()
        assertSame("Repeated committed selections must not request recreation", activity, controller.get())
        assertPluginsUsable(controller.get())
    }

    @Test fun darkLightAndBrandChangesSaveAndLeaveSettingsInteractive() {
        val controller = settings()
        for ((mode, position) in listOf("dark" to 2, "light" to 1)) {
            spinner(controller.get(), R.string.appearance).setSelection(position); idle()
            assertEquals(mode, prefs.mode)
            controller.recreate().visible(); idle()
            assertEquals(mode == "dark", controller.get().dark)
            assertPluginsUsable(controller.get())
        }
        spinner(controller.get(), R.string.app_name).setSelection(3); idle()
        assertEquals("frost-byte", prefs.brand)
        controller.recreate().visible(); idle()
        assertEquals(3, spinner(controller.get(), R.string.app_name).selectedItemPosition)
        assertPluginsUsable(controller.get())
    }

    @Test fun explicitLanguagesAndReturnToSystemSaveAndLeaveSettingsInteractive() {
        val controller = settings()
        for ((language, position) in listOf("fa" to 1, "en" to 2, "" to 0)) {
            spinner(controller.get(), R.string.language).setSelection(position); idle()
            assertEquals(language, prefs.language)
            controller.recreate().visible(); idle()
            assertEquals(language, AppCompatDelegate.getApplicationLocales().toLanguageTags())
            assertEquals(position, spinner(controller.get(), R.string.language).selectedItemPosition)
            assertPluginsUsable(controller.get())
        }
    }

    @Test fun appearanceRecreationPreservesHomeHistoryAndSearch() {
        val controller = Robolectric.buildActivity(MainActivity::class.java)
        controllers.add(controller); controller.setup().visible(); idle()
        val activity = controller.get()
        val history = views(content(activity)).filterIsInstance<com.google.android.material.button.MaterialButton>().first {
            it.text.toString().startsWith(activity.getString(R.string.history))
        }
        assertTrue(history.performClick())
        views(content(activity)).filterIsInstance<EditText>().first().setText("kept history search")
        prefs.mode = "dark"
        controller.recreate().visible(); idle()
        val restored = controller.get()
        assertTrue(restored.dark)
        assertEquals("kept history search", views(content(restored)).filterIsInstance<EditText>().first().text.toString())
        assertTrue(views(content(restored)).filterIsInstance<com.google.android.material.button.MaterialButton>().first {
            it.text.toString().startsWith(restored.getString(R.string.history))
        }.isChecked)
    }
}
