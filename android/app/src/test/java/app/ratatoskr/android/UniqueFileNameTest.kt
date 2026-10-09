package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Test

class UniqueFileNameTest {
    @Test fun firstFileHasNoSuffixAndCopiesIncrement() {
        val existing = mutableSetOf<String>()
        repeat(3) { index ->
            val name = UniqueFileName.next("سجاد.mkv") { it in existing }
            assertEquals(if (index == 0) "سجاد.mkv" else "سجاد ($index).mkv", name)
            existing.add(name)
        }
        assertEquals("سجاد (3).mkv", UniqueFileName.next("سجاد (1).mkv") { it in existing })
    }
    @Test fun keepsTheExtensionAndPicksTheFirstFreeNumber() {
        val existing = setOf("episode.mkv", "episode (1).mkv", "episode (3).mkv")
        assertEquals("episode (2).mkv", UniqueFileName.next("episode.mkv") { it in existing })
    }

    @Test fun doesNotTreatAHiddenFileDotAsAnExtension() {
        assertEquals(".config (1)", UniqueFileName.next(".config") { it == ".config" })
    }
}
