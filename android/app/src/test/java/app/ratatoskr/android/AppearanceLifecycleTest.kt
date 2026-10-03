package app.ratatoskr.android

import android.content.Context
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.Spinner
import android.widget.ScrollView
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
import java.util.UUID
import java.util.concurrent.TimeUnit

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
    private lateinit var store: TaskStore
    private lateinit var databaseName: String
    private var previousStore: Any? = null
    private val instanceField = TaskStore::class.java.getDeclaredField("instance").apply { isAccessible = true }

    @Before fun preparePreferences() {
        val app = RuntimeEnvironment.getApplication()
        databaseName = "appearance-test-${UUID.randomUUID()}.db"
        store = TaskStore(app, databaseName)
        previousStore = instanceField.get(null)
        instanceField.set(null, store)
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
        instanceField.set(null, previousStore)
        store.close()
        RuntimeEnvironment.getApplication().deleteDatabase(databaseName)
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
        // Journal/view state belongs to the activity adapter; Compose rendering is
        // exercised by SmokeTest on three actual emulator API levels.
        fun field(name: String) = MainActivity::class.java.getDeclaredField(name).apply { isAccessible = true }
        field("history").set(activity, true)
        field("query").set(activity, "kept history search")
        prefs.mode = "dark"
        controller.recreate().visible(); idle()
        val restored = controller.get()
        assertTrue(restored.dark)
        assertEquals("kept history search", field("query").get(restored))
        assertEquals(true, field("history").get(restored))
    }

    @Test fun changingBrandRetainsTheSettingsScrollPosition() {
        val controller = settings()
        fun scroll(): ScrollView = views(content(controller.get())).filterIsInstance<ScrollView>().first()
        fun layout() {
            val root = content(controller.get())
            root.measure(View.MeasureSpec.makeMeasureSpec(1080, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(1600, View.MeasureSpec.EXACTLY))
            root.layout(0, 0, 1080, 1600)
            idle()
        }
        layout()
        scroll().scrollTo(0, 400)
        val originalOffset = scroll().scrollY
        assertTrue("Fixture must start below the top of the settings page", originalOffset > 0)
        spinner(controller.get(), R.string.app_name).setSelection(3)
        idle(); layout()
        assertEquals("frost-byte", prefs.brand)
        assertEquals("Changing appearance must keep the current settings position", originalOffset, scroll().scrollY)
    }

    @Test fun failedDirectTaskStaysOnActiveWithItsExplanationAndRetry() {
        val task = store.enqueue("https://example.org/fixture.zip", null, false, "Failed direct fixture", "file")
        val controller = Robolectric.buildActivity(MainActivity::class.java)
        controllers.add(controller); controller.setup().visible(); idle()
        val activity = controller.get()
        fun rows(): List<TaskRow> {
            val method = MainActivity::class.java.getDeclaredMethod("getHomeState").apply { isAccessible = true }
            return (method.invoke(activity) as AbHomeState).rows
        }
        assertEquals(task.id, rows().single().task.id)
        store.state(task.id, TaskState.FAILED, "not_a_file")
        shadowOf(Looper.getMainLooper()).idleFor(800, TimeUnit.MILLISECONDS)
        assertEquals("Failure stays on the active list", task.id, rows().single().task.id)
        assertEquals(TaskState.FAILED, rows().single().task.state)
        assertEquals("not_a_file", rows().single().task.error)
    }
}
