package app.ratatoskr.android

import org.junit.Assert.*
import org.junit.Test

class TransferProgressTest {
    private val task = MobileTask("id", "https://example.com/a", "a", TaskState.DOWNLOADING)
    @Test fun mediaPercentIsVisibleWithoutContentLength() {
        assertEquals(.42f, TransferProgress.fraction(task.copy(progress = 42))!!, .001f)
    }
    @Test fun fileProgressUsesLiveBytesAndRemainsBounded() {
        assertEquals(.25f, TransferProgress.fraction(task.copy(bytesDone = 25, totalBytes = 100))!!, .001f)
        assertEquals(1f, TransferProgress.fraction(task.copy(bytesDone = 150, totalBytes = 100))!!, .001f)
        assertNull(TransferProgress.fraction(task))
        assertNull(TransferProgress.fraction(task.copy(state = TaskState.PROBING, progress = 20)))
    }
}
