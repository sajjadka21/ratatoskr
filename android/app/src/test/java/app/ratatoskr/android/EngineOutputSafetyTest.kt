package app.ratatoskr.android

import android.net.Uri
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
class EngineOutputSafetyTest {
    @Test fun onlyPublicMediaStoreDownloadAndImageRowsCanBeDeleted() {
        assertTrue(Engine.isMediaRow(Uri.parse("content://media/external_primary/downloads/42")))
        assertTrue(Engine.isMediaRow(Uri.parse("content://media/external/images/media/42")))
        assertTrue(Engine.isMediaRow(Uri.parse("content://media/external/file/42")))
        assertFalse(Engine.isMediaRow(Uri.parse("content://media/external_primary/video/media/42")))
        assertFalse(Engine.isMediaRow(Uri.parse("content://com.android.providers.downloads.documents/document/42")))
        assertFalse(Engine.isMediaRow(Uri.parse("content://media/external_primary/downloads/42?query=1")))
    }
}

