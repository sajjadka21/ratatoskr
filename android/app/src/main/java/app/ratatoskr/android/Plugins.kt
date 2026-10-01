package app.ratatoskr.android

import org.json.JSONObject
import java.net.URI

/** One rule of a plugin. Same file format and meaning as the desktop app (docs/PLUGINS.md). */
sealed class PluginRule {
    data class RewriteUrl(val pattern: String, val replace: String) : PluginRule()
    data class Rename(val pattern: String, val replace: String) : PluginRule()
}

data class Plugin(val id: String, val name: String, val version: String, val rules: List<PluginRule>, val ruleCount: Int)

class PluginException(message: String) : Exception(message)

/** Plugins are data, never code: they can only rewrite links and file names, and every result is checked. */
object Plugins {
    const val MAX_BYTES = 64 * 1024
    private const val MAX_RULES = 50
    private const val MAX_WILDCARDS = 4
    private const val MAX_PATTERN = 512
    private const val MAX_INPUT = 4096

    fun parse(text: String): Plugin {
        if (text.toByteArray(Charsets.UTF_8).size > MAX_BYTES) throw PluginException("too_large")
        val json = try { JSONObject(text) } catch (error: Exception) { throw PluginException("not_json") }
        if (json.optInt("schema") != 1) throw PluginException("unsupported_schema")
        val id = json.optString("id")
        if (!Regex("[a-z0-9-]{1,40}").matches(id)) throw PluginException("bad_id")
        val rows = json.optJSONArray("rules")
        if ((rows?.length() ?: 0) > MAX_RULES) throw PluginException("too_many_rules")
        val rules = (0 until (rows?.length() ?: 0)).mapNotNull { index ->
            val rule = rows!!.optJSONObject(index) ?: throw PluginException("bad_rule")
            val type = rule.optString("type")
            when (type) {
                "rewrite_url", "rename" -> {
                    val pattern = rule.optString("match"); val replace = rule.optString("replace")
                    if (pattern.isEmpty() || pattern.length > MAX_PATTERN || replace.length > MAX_PATTERN) throw PluginException("bad_rule")
                    val wildcards = pattern.count { it == '*' }
                    if (wildcards > MAX_WILDCARDS) throw PluginException("bad_rule")
                    if (Regex("\\{(\\d+)}").findAll(replace).any { it.groupValues[1].toInt().let { slot -> slot == 0 || slot > wildcards } }) throw PluginException("bad_rule")
                    if (type == "rewrite_url") PluginRule.RewriteUrl(pattern, replace) else PluginRule.Rename(pattern, replace)
                }
                // Header rules are a desktop feature; the file is still valid here and they are ignored.
                "referer", "user_agent" -> null
                else -> throw PluginException("bad_rule")
            }
        }
        return Plugin(id, json.optString("name").trim().ifEmpty { id }.take(80), json.optString("version").trim().take(20), rules, rows?.length() ?: 0)
    }

    /** What each `*` caught, or null when the text does not match. */
    fun glob(pattern: String, text: String): List<String>? {
        if (text.length > MAX_INPUT) return null
        val parts = pattern.split('*')
        if (parts.size == 1) return if (pattern == text) emptyList() else null
        if (!text.startsWith(parts.first())) return null
        var rest = text.substring(parts.first().length)
        val caught = mutableListOf<String>()
        for (middle in parts.subList(1, parts.size - 1)) {
            val at = rest.indexOf(middle)
            if (at < 0) return null
            caught.add(rest.substring(0, at)); rest = rest.substring(at + middle.length)
        }
        if (!rest.endsWith(parts.last())) return null
        caught.add(rest.substring(0, rest.length - parts.last().length))
        return caught
    }

    fun expand(replace: String, caught: List<String>): String =
        Regex("\\{(\\d+)}").replace(replace) { match -> caught.getOrNull(match.groupValues[1].toInt() - 1) ?: match.value }

    fun rewriteUrl(plugins: List<Plugin>, url: String): String {
        for (rule in plugins.flatMap { it.rules }) {
            if (rule !is PluginRule.RewriteUrl) continue
            val caught = glob(rule.pattern, url) ?: continue
            val result = expand(rule.replace, caught)
            if (isHttp(result)) return result
        }
        return url
    }

    fun rename(plugins: List<Plugin>, name: String): String {
        for (rule in plugins.flatMap { it.rules }) {
            if (rule !is PluginRule.Rename) continue
            val caught = glob(rule.pattern, name) ?: continue
            val result = expand(rule.replace, caught)
            if (result.isNotBlank() && result.none { it == '/' || it == '\\' || it == '\u0000' }) return result
        }
        return name
    }

    private fun isHttp(value: String) = runCatching { URI(value).let { it.scheme?.lowercase() in setOf("http", "https") && !it.host.isNullOrEmpty() } }.getOrDefault(false)
}
