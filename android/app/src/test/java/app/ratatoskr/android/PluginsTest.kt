package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [29])
class PluginsTest {
    private val sample = """{ "schema": 1, "id": "my-site", "name": "My site", "version": "1.0.0", "rules": [
        { "type": "rewrite_url", "match": "https://example.com/view/*", "replace": "https://cdn.example.com/files/{1}.zip" },
        { "type": "rename", "match": "*.mp4.part", "replace": "{1}.mp4" },
        { "type": "referer", "host": "cdn.example.com", "value": "https://example.com/" } ] }"""

    @Test fun parsesAndAppliesRules() {
        val plugin = Plugins.parse(sample)
        assertEquals("my-site", plugin.id)
        assertEquals(3, plugin.ruleCount)
        assertEquals("https://cdn.example.com/files/42.zip", Plugins.rewriteUrl(listOf(plugin), "https://example.com/view/42"))
        assertEquals("https://other.org/view/42", Plugins.rewriteUrl(listOf(plugin), "https://other.org/view/42"))
        assertEquals("clip.mp4", Plugins.rename(listOf(plugin), "clip.mp4.part"))
        assertEquals("clip.mkv", Plugins.rename(listOf(plugin), "clip.mkv"))
    }

    @Test fun aRewriteMayNotLeaveHttpAndARenameMayNotBecomeAPath() {
        val plugin = Plugins.parse("""{ "schema": 1, "id": "x", "rules": [
            { "type": "rewrite_url", "match": "https://a.org/*", "replace": "file:///{1}" },
            { "type": "rename", "match": "*", "replace": "../{1}" } ] }""")
        assertEquals("https://a.org/etc", Plugins.rewriteUrl(listOf(plugin), "https://a.org/etc"))
        assertEquals("a.bin", Plugins.rename(listOf(plugin), "a.bin"))
    }

    @Test fun badPluginsAreRefused() {
        fun fails(text: String) = assertThrows(PluginException::class.java) { Plugins.parse(text) }
        fails("nope")
        fails("""{"schema":2,"id":"x"}""")
        fails("""{"schema":1,"id":"Bad Id"}""")
        fails("""{"schema":1,"id":"x","rules":[{"type":"run","match":"a"}]}""")
        fails("""{"schema":1,"id":"x","rules":[{"type":"rename","match":"a*","replace":"{2}"}]}""")
        fails("""{"schema":1,"id":"x","rules":[{"type":"rename","match":"*****a","replace":"x"}]}""")
        fails("x".repeat(Plugins.MAX_BYTES + 1))
    }

    @Test fun globCatchesWhatEachStarMatched() {
        assertEquals(listOf("XX", "YY"), Plugins.glob("a*b*c", "aXXbYYc"))
        assertEquals(emptyList<String>(), Plugins.glob("exact", "exact"))
        assertNull(Plugins.glob("exact", "inexact"))
        assertNull(Plugins.glob("a*b", "a".repeat(5000)))
    }

    @Test fun theStoreKeepsListsTogglesAndRemoves() {
        val folder = java.nio.file.Files.createTempDirectory("plugins").toFile()
        try {
            val store = PluginStore(folder, null)
            store.import(sample)
            assertEquals(listOf("my-site"), store.all().map { it.id })
            assertTrue(store.enabled("my-site"))
            store.remove("../my-site")
            assertEquals(1, store.all().size)
            store.remove("my-site")
            assertEquals(0, store.all().size)
            assertThrows(PluginException::class.java) { store.import("""{"schema":1,"id":"../evil"}""") }
        } finally { folder.deleteRecursively() }
    }
}
