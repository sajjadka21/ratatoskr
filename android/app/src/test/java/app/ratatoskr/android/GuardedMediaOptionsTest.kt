package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Validate the actual selector grammar and protocol predicates used by yt-dlp. */
class GuardedMediaOptionsTest {
    private data class Component(val kind: String, val filters: String)
    private val atom = "(bestaudio|best|bv\\*|ba|b)((?:\\[[^\\]]+\\])*)"
    private val componentPattern = Regex(atom)
    private val selectorPattern = Regex("$atom(?:[+/]$atom)*")
    private val protocolPattern = Regex("\\[protocol~='([^']+)'\\]")
    private val allowed = listOf("http", "https", "m3u8_native", "http_dash_segments")

    private fun components(selector: String): List<Component> {
        assertTrue("Unexpected or unparsed selector syntax: $selector", selectorPattern.matches(selector))
        return componentPattern.findAll(selector).map {
            Component(it.groupValues[1], it.groupValues[2])
        }.toList().also { assertTrue("Selector must contain a format", it.isNotEmpty()) }
    }

    private fun protocols(component: Component): Regex {
        val predicates = protocolPattern.findAll(component.filters).toList()
        assertEquals("Each format must have exactly one mandatory protocol predicate: $component", 1, predicates.size)
        return Regex(predicates.single().groupValues[1])
    }

    @Test fun combinedVideoAndEachFallbackAllowSupportedNetworkProtocols() {
        val selector = MediaOptions.guardedFormat(null, false)
        val formats = components(selector)
        assertEquals(listOf("bv*", "ba", "b"), formats.map { it.kind })
        for (component in formats) {
            val predicate = protocols(component)
            for (protocol in allowed) {
                assertTrue("${component.kind} must accept $protocol", predicate.containsMatchIn(protocol))
            }
        }
    }

    @Test fun audioPreferenceAndAllAudioFallbacksRemainGuarded() {
        val selector = MediaOptions.guardedFormat(null, true)
        val formats = components(selector)
        assertEquals(listOf("bestaudio", "bestaudio", "best"), formats.map { it.kind })
        assertTrue("Prefer an existing m4a audio stream", formats.first().filters.contains("[ext=m4a]"))
        for (component in formats) {
            val predicate = protocols(component)
            for (protocol in allowed) assertTrue(predicate.containsMatchIn(protocol))
        }
        assertTrue("The muxed best fallback still requires extraction", MediaOptions.requiresAudioExtraction(true))
    }

    @Test fun protocolSearchCannotAdmitUnsupportedOrCompositeProtocolsThroughAnyBranch() {
        // yt-dlp applies regex search, so substring matches must also be rejected.
        val rejected = listOf("", "ftp", "file", "rtmp", "rtsp", "m3u8", "data", "ws",
            "unknown", "xhttps", "httpsx", "https+ffmpeg", "http_dash_segments+ffmpeg")
        for (audioOnly in listOf(false, true)) {
            for (height in listOf<Int?>(null, 540)) {
                val selector = MediaOptions.guardedFormat(height, audioOnly)
                for (component in components(selector)) {
                    val predicate = protocols(component)
                    for (protocol in rejected) {
                        assertFalse("$selector admitted $protocol via ${component.kind}", predicate.containsMatchIn(protocol))
                    }
                }
            }
        }
    }

    @Test fun eachVideoCapableFallbackHonorsTheRequestedHeight() {
        for (height in listOf(360, 480, 540, 720, 1080)) {
            val formats = components(MediaOptions.guardedFormat(height, false))
            assertTrue("A video selector must include a muxed fallback", formats.any { it.kind == "b" })
            for (component in formats) {
                protocols(component)
                if (component.kind in setOf("bv*", "b", "best")) {
                    assertTrue("${component.kind} exceeds the $height limit", component.filters.contains("[height<=$height]"))
                } else {
                    assertFalse("Audio must not be filtered by video height", component.filters.contains("[height"))
                }
            }
        }
    }

    @Test fun selectingAudioIgnoresVideoHeightWithoutLosingItsProtocolGuard() {
        for (height in listOf(480, 540, 1080)) {
            val formats = components(MediaOptions.guardedFormat(height, true))
            assertTrue(formats.first().filters.contains("[ext=m4a]"))
            for (component in formats) {
                protocols(component)
                assertFalse(component.filters.contains("[height"))
            }
        }
    }

    @Test fun automaticHeightPreservesUncappedSelectionWithMandatoryProtocolFilters() {
        for (height in listOf<Int?>(null, 0, -1)) {
            val formats = components(MediaOptions.guardedFormat(height, false))
            assertEquals(listOf("bv*", "ba", "b"), formats.map { it.kind })
            for (component in formats) {
                protocols(component)
                assertFalse(component.filters.contains("[height"))
            }
        }
    }
}
