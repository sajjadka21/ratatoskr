package app.ratatoskr.android

import android.content.Intent
import android.text.SpannableString
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
class ShareIntakeTest {
    @Test fun instagramSharedCaptionOpensNormalIntakeWithoutProbeOrAutomaticDownload() {
        val context = RuntimeEnvironment.getApplication()
        val reel = "https://www.instagram.com/reel/Dd_Rx17K9K3/"
        val intent = Intent(context, ShareActivity::class.java).setAction(Intent.ACTION_SEND).setType("text/plain")
            .putExtra(Intent.EXTRA_TEXT, SpannableString("Watch this reel: $reel"))
        val controller = Robolectric.buildActivity(ShareActivity::class.java, intent).create()
        try {
            val activity = controller.get()
            val next = shadowOf(activity).nextStartedActivity
            assertNotNull(next)
            assertEquals(MainActivity::class.java.name, next.component!!.className)
            assertEquals(reel, next.getStringExtra(Intent.EXTRA_TEXT))
            assertTrue(next.flags and Intent.FLAG_ACTIVITY_CLEAR_TOP != 0)
            assertTrue(next.flags and Intent.FLAG_ACTIVITY_SINGLE_TOP != 0)
            assertTrue(activity.isFinishing)
        } finally { controller.destroy() }
    }
}
