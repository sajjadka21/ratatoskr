package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.io.ByteArrayInputStream

class ChecksumsTest {
    private val abc = "abc".toByteArray()

    @Test fun knownVectors() {
        assertEquals("900150983cd24fb0d6963f7d28e17f72", Checksums.compute(ByteArrayInputStream(abc), "MD5"))
        assertEquals("a9993e364706816aba3e25717850c26c9cd0d89d", Checksums.compute(ByteArrayInputStream(abc), "SHA-1"))
        assertEquals("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", Checksums.compute(ByteArrayInputStream(abc), "SHA-256"))
    }

    @Test fun theAlgorithmFollowsTheLength() {
        assertEquals("MD5", Checksums.algorithm("900150983cd24fb0d6963f7d28e17f72"))
        assertEquals("SHA-1", Checksums.algorithm("a9993e364706816aba3e25717850c26c9cd0d89d"))
        assertEquals("SHA-256", Checksums.algorithm("BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"))
        assertNull(Checksums.algorithm("abc"))
        assertNull(Checksums.algorithm("g".repeat(32)))
    }

    @Test fun pastedTextIsCleanedUp() {
        assertEquals("abcdef", Checksums.normalize("  ABCDEF  file.zip\n"))
        assertEquals("abcdef", Checksums.normalize("sha256:ABCDEF"))
        assertEquals("", Checksums.normalize("   "))
    }

    @Test fun readingCanBeInterrupted() {
        var calls = 0
        try { Checksums.compute(ByteArrayInputStream(ByteArray(300_000)), "MD5") { if (++calls > 2) throw TransferFailure("interrupted") } } catch (e: TransferFailure) { assertEquals("interrupted", e.code) }
        assertEquals(3, calls)
    }
}
