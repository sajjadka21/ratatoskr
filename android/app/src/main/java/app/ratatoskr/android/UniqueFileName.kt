package app.ratatoskr.android

/** Produces `name (1).ext`, `name (2).ext`, … without losing the extension. */
internal object UniqueFileName {
    fun next(requested: String, exists: (String) -> Boolean): String {
        if (!exists(requested)) return requested
        val dot = requested.lastIndexOf('.').takeIf { it > 0 }
        val stem = (if (dot == null) requested else requested.substring(0, dot)).replace(Regex(" \\([1-9][0-9]*\\)$"), "")
        val extension = if (dot == null) "" else requested.substring(dot)
        for (index in 1..9999) {
            val candidate = "$stem ($index)$extension"
            if (!exists(candidate)) return candidate
        }
        error("unable to allocate a unique filename")
    }
}
