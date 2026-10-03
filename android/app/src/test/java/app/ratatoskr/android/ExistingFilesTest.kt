package app.ratatoskr.android
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.io.File
import java.util.UUID

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28])
class ExistingFilesTest {
    @Test @Config(sdk = [29]) fun scopedStorageIsReportedAsLimitedRatherThanProvingAbsence() {
        val result = ExistingFiles.inspect(RuntimeEnvironment.getApplication(), listOf("https://example.com/video.mp4"))
        assertTrue(result.limited)
    }
    @Test fun alsoChecksTheDefaultPicturesDestinationWithoutChangingTheImage() {
        val context = RuntimeEnvironment.getApplication()
        val root = File(android.os.Environment.getExternalStoragePublicDirectory(android.os.Environment.DIRECTORY_PICTURES), "Ratatoskr")
        root.mkdirs()
        val name = "Photo${UUID.randomUUID().toString().replace("-", "")}"
        val file = File(root, "$name.jpg")
        try {
            file.writeText("unchanged-image")
            assertTrue(file.name in ExistingFiles.inspect(context, listOf("https://example.com/$name.jpg")).matches)
            assertEquals("unchanged-image", file.readText())
        } finally { file.delete() }
    }
    @Test fun findsAnActualFileOutsideAppHistoryWithoutChangingIt() {
        val context = RuntimeEnvironment.getApplication()
        val root = android.os.Environment.getExternalStoragePublicDirectory(android.os.Environment.DIRECTORY_DOWNLOADS)
        root.mkdirs()
        val unique = "Fixture${UUID.randomUUID().toString().replace("-", "")}.S04E01"
        val file = File(root, "$unique.site.mkv")
        try {
            file.writeText("untouched")
            val result = ExistingFiles.inspect(context, listOf("https://example.com/$unique.mkv"))
            assertTrue(file.name in result.matches)
            assertEquals("untouched", file.readText())
            assertFalse(file.name in ExistingFiles.inspect(context, listOf("https://example.com/${unique.replace("E01", "E02")}.mkv")).matches)
        } finally { file.delete() }
    }
}
