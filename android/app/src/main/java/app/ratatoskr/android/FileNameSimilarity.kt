package app.ratatoskr.android

/** A filename hint only; episode numbers and file types must remain distinct. */
object FileNameSimilarity {
    private val ignored = setOf("web", "dl", "webdl", "webrip", "bluray", "bdrip", "h264", "h265", "x264", "x265", "hevc", "avc", "aac", "proper", "repack")
    private fun words(name: String) = name.substringBeforeLast('.', name).lowercase(java.util.Locale.ROOT)
        .replace(Regex("\\s*\\(\\d+\\)$"), "").split(Regex("[^\\p{L}\\p{N}]+"))
        .filter { it.isNotEmpty() && it !in ignored && !it.matches(Regex("\\d{3,4}p")) }.take(64)
    fun similar(a: String, b: String): Boolean {
        val ext = a.substringAfterLast('.', "").lowercase(java.util.Locale.ROOT)
        if (ext.isEmpty() || ext != b.substringAfterLast('.', "").lowercase(java.util.Locale.ROOT)) return false
        val left = words(a); val right = words(b)
        if (left.isEmpty() || right.isEmpty()) return false
        fun numbers(words: List<String>) = words.flatMap { Regex("\\d+").findAll(it).map { n -> n.value }.toList() }
        if (numbers(left) != numbers(right)) return false
        val row = IntArray(right.size + 1)
        for (word in left) {
            var previous = 0
            for ((j, other) in right.withIndex()) {
                val saved = row[j + 1]
                row[j + 1] = if (word == other) previous + 1 else maxOf(row[j], row[j + 1])
                previous = saved
            }
        }
        val smaller = minOf(left.size, right.size)
        return row.last() * 5 >= maxOf(left.size, right.size) * 4 || (smaller >= 2 && row.last() == smaller && maxOf(left.size, right.size) <= smaller + 2)
    }
}
