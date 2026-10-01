package app.ratatoskr.android

import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter

data class HttpValidators(val etag: String? = null, val lastModified: String? = null)
data class ResumeRequest(val offset: Long, val total: Long? = null, val validators: HttpValidators = HttpValidators())
data class HttpResponseMetadata(val status: Int, val contentRange: String? = null, val contentLength: Long? = null, val validators: HttpValidators = HttpValidators())
enum class ResumeDecision { APPEND, RESTART, COMPLETE, REJECT }

object HttpResumePolicy {
    fun ifRange(validators: HttpValidators): String? {
        val tag = validators.etag
        if (tag != null && Regex("\"[^\"\\r\\n]*\"").matches(tag)) return tag
        val date = validators.lastModified ?: return null
        if ('\r' in date || '\n' in date) return null
        return runCatching { ZonedDateTime.parse(date, DateTimeFormatter.RFC_1123_DATE_TIME); date }.getOrNull()
    }
    fun evaluate(request: ResumeRequest, response: HttpResponseMetadata): ResumeDecision {
        if (request.offset < 0 || (request.total != null && request.total < 0)) return ResumeDecision.REJECT
        if (response.status == 200) return ResumeDecision.RESTART
        val identity = ifRange(request.validators)?.let { it == ifRange(response.validators) } == true
        if (response.status == 416) {
            val size = Regex("bytes \\*/(\\d+)").matchEntire(response.contentRange.orEmpty())?.groupValues?.get(1)?.toLongOrNull() ?: return ResumeDecision.REJECT
            return if (identity && request.offset == size && request.total == size) ResumeDecision.COMPLETE else ResumeDecision.RESTART
        }
        if (response.status != 206) return ResumeDecision.REJECT
        val range = Regex("bytes (\\d+)-(\\d+)/(\\d+)").matchEntire(response.contentRange.orEmpty()) ?: return ResumeDecision.REJECT
        val values = range.groupValues.drop(1).map { it.toLongOrNull() ?: return ResumeDecision.REJECT }
        val (start, end, total) = values
        if (start != request.offset || end < start || total <= end ||
            (response.contentLength != null && response.contentLength != end - start + 1)) return ResumeDecision.REJECT
        if (!identity || (request.total != null && request.total != total)) return ResumeDecision.RESTART
        return ResumeDecision.APPEND
    }
    fun transferFinished(expectedBytes: Long?, receivedBytes: Long): Boolean = receivedBytes >= 0 &&
        (expectedBytes == null || (expectedBytes >= 0 && expectedBytes == receivedBytes))
}
