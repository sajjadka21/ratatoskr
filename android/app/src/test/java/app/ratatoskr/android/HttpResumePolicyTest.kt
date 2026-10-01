package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** A resumed HTTP response must establish identity and byte continuity before append. */
class HttpResumePolicyTest {
    private val original = HttpValidators(etag = "\"version-one\"")
    private val saved = ResumeRequest(offset = 100, total = 1000, validators = original)

    @Test fun strongEntityTagIsPreferredForIfRange() {
        assertEquals("\"version-one\"", HttpResumePolicy.ifRange(HttpValidators(
            etag = "\"version-one\"", lastModified = "Wed, 21 Oct 2015 07:28:00 GMT")))
    }

    @Test fun aWeakTagCannotAuthorizeAppendingOnItsOwn() {
        assertNull(HttpResumePolicy.ifRange(HttpValidators(etag = "W/\"version-one\"")))
        assertNull(HttpResumePolicy.ifRange(HttpValidators(etag = "unquoted-invalid-tag")))
        assertNull(HttpResumePolicy.ifRange(HttpValidators()))
    }

    @Test fun lastModifiedCanBeUsedWhenNoStrongEntityTagExists() {
        val modified = "Wed, 21 Oct 2015 07:28:00 GMT"
        assertEquals(modified, HttpResumePolicy.ifRange(HttpValidators(lastModified = modified)))
        assertEquals(modified, HttpResumePolicy.ifRange(HttpValidators(
            etag = "W/\"version-one\"", lastModified = modified)))
    }

    @Test fun malformedOrInjectedValidatorsNeverBecomeRequestHeaders() {
        assertNull(HttpResumePolicy.ifRange(HttpValidators(lastModified = "not-a-date")))
        assertNull(HttpResumePolicy.ifRange(HttpValidators(etag = "\"ok\"\r\nInjected: true")))
        assertNull(HttpResumePolicy.ifRange(HttpValidators(lastModified = "Wed, 21 Oct 2015 07:28:00 GMT\r\nX: y")))
    }

    @Test fun matchingPartialResponseContinuesAtTheExactStoredOffset() {
        assertEquals(ResumeDecision.APPEND, HttpResumePolicy.evaluate(saved,
            HttpResponseMetadata(206, "bytes 100-999/1000", 900, original)))
    }

    @Test fun aServerMayReturnAShorterValidRangeWhilePreservingContinuity() {
        assertEquals(ResumeDecision.APPEND, HttpResumePolicy.evaluate(saved,
            HttpResponseMetadata(206, "bytes 100-499/1000", 400, original)))
    }

    @Test fun unknownResponseLengthCanUseTheValidatedContentRangeLength() {
        assertEquals(ResumeDecision.APPEND, HttpResumePolicy.evaluate(saved,
            HttpResponseMetadata(206, "bytes 100-999/1000", null, original)))
    }

    @Test fun anIgnoredRangeMustRestartInsteadOfAppendingTheWholeFile() {
        assertEquals(ResumeDecision.RESTART, HttpResumePolicy.evaluate(saved,
            HttpResponseMetadata(200, contentLength = 1000, validators = original)))
    }

    @Test fun aFreshFullResponseStartsFromZero() {
        assertEquals(ResumeDecision.RESTART, HttpResumePolicy.evaluate(ResumeRequest(0),
            HttpResponseMetadata(200, contentLength = 1000)))
    }

    @Test fun anUnexpectedRangeStartCannotBeAppended() {
        listOf("bytes 0-999/1000", "bytes 101-999/1000").forEach { range ->
            assertEquals(range, ResumeDecision.REJECT, HttpResumePolicy.evaluate(saved,
                HttpResponseMetadata(206, range, validators = original)))
        }
    }

    @Test fun invalidRangeBoundsAndUnknownTotalsAreRefused() {
        listOf("bytes 100-99/1000", "bytes 100-1000/1000", "bytes 100-999/0",
            "bytes 100-999/*", "bytes -1-999/1000", "bytes 100-999/not-a-number")
            .forEach { range ->
                assertEquals(range, ResumeDecision.REJECT, HttpResumePolicy.evaluate(saved,
                    HttpResponseMetadata(206, range, validators = original)))
            }
    }

    @Test fun absentOrMalformedContentRangeCannotAuthorizeAppend() {
        listOf<String?>(null, "", "100-999/1000", "items 100-999/1000", "bytes */1000")
            .forEach { range ->
                assertEquals(ResumeDecision.REJECT, HttpResumePolicy.evaluate(saved,
                    HttpResponseMetadata(206, range, validators = original)))
            }
    }

    @Test fun contentLengthMustAgreeWithTheAdvertisedRange() {
        listOf<Long>(899, 901, -1).forEach { length ->
            assertEquals(ResumeDecision.REJECT, HttpResumePolicy.evaluate(saved,
                HttpResponseMetadata(206, "bytes 100-999/1000", length, original)))
        }
    }

    @Test fun changedEntityTagRequiresACompleteRestart() {
        assertEquals(ResumeDecision.RESTART, HttpResumePolicy.evaluate(saved,
            HttpResponseMetadata(206, "bytes 100-999/1000", 900, HttpValidators(etag = "\"version-two\""))))
    }

    @Test fun missingOrDowngradedEntityTagCannotBlindlyReuseAnExistingPartialFile() {
        listOf(HttpValidators(), HttpValidators(etag = "W/\"version-one\""))
            .forEach { current ->
                assertEquals(ResumeDecision.RESTART, HttpResumePolicy.evaluate(saved,
                    HttpResponseMetadata(206, "bytes 100-999/1000", 900, current)))
            }
    }

    @Test fun matchingWeakTagsWithoutAnotherValidatorAreNotEnough() {
        val weak = HttpValidators(etag = "W/\"version-one\"")
        assertEquals(ResumeDecision.RESTART, HttpResumePolicy.evaluate(ResumeRequest(100, 1000, weak),
            HttpResponseMetadata(206, "bytes 100-999/1000", 900, weak)))
        assertEquals(ResumeDecision.RESTART, HttpResumePolicy.evaluate(ResumeRequest(100, 1000),
            HttpResponseMetadata(206, "bytes 100-999/1000", 900)))
    }

    @Test fun matchingLastModifiedAllowsResumeButChangedDateRestarts() {
        val savedDate = HttpValidators(lastModified = "Wed, 21 Oct 2015 07:28:00 GMT")
        val request = ResumeRequest(100, 1000, savedDate)
        assertEquals(ResumeDecision.APPEND, HttpResumePolicy.evaluate(request,
            HttpResponseMetadata(206, "bytes 100-999/1000", 900, savedDate)))
        assertEquals(ResumeDecision.RESTART, HttpResumePolicy.evaluate(request,
            HttpResponseMetadata(206, "bytes 100-999/1000", 900,
                HttpValidators(lastModified = "Thu, 22 Oct 2015 07:28:00 GMT"))))
    }

    @Test fun changedTotalSizeRequiresRestartEvenIfTheTagWasReused() {
        assertEquals(ResumeDecision.RESTART, HttpResumePolicy.evaluate(saved,
            HttpResponseMetadata(206, "bytes 100-1999/2000", 1900, original)))
    }

    @Test fun a416ResponseIsCompleteOnlyWhenIdentityAndStoredSizeMatch() {
        assertEquals(ResumeDecision.COMPLETE, HttpResumePolicy.evaluate(ResumeRequest(1000, 1000, original),
            HttpResponseMetadata(416, "bytes */1000", validators = original)))
    }

    @Test fun a416WithDifferentSizeOrIdentityMustNotReportCompletion() {
        assertEquals(ResumeDecision.RESTART, HttpResumePolicy.evaluate(ResumeRequest(1100, 1100, original),
            HttpResponseMetadata(416, "bytes */1000", validators = original)))
        assertEquals(ResumeDecision.RESTART, HttpResumePolicy.evaluate(ResumeRequest(1000, 1000, original),
            HttpResponseMetadata(416, "bytes */1000", validators = HttpValidators(etag = "\"version-two\""))))
        assertEquals(ResumeDecision.RESTART, HttpResumePolicy.evaluate(ResumeRequest(1000, 1000),
            HttpResponseMetadata(416, "bytes */1000")))
    }

    @Test fun a416WithoutTheRemoteSizeIsNotEvidenceOfCompletion() {
        assertEquals(ResumeDecision.REJECT, HttpResumePolicy.evaluate(ResumeRequest(1000, 1000, original),
            HttpResponseMetadata(416, validators = original)))
    }

    @Test fun errorResponsesCannotBeWrittenAsDownloadedContent() {
        listOf(301, 403, 404, 429, 500).forEach { status ->
            assertEquals(ResumeDecision.REJECT, HttpResumePolicy.evaluate(saved,
                HttpResponseMetadata(status, contentLength = 100, validators = original)))
        }
    }

    @Test fun invalidStoredOffsetsAndSizesAreRejected() {
        listOf(ResumeRequest(-1, 1000, original), ResumeRequest(100, -1, original))
            .forEach { request ->
                assertEquals(ResumeDecision.REJECT, HttpResumePolicy.evaluate(request,
                    HttpResponseMetadata(206, "bytes 100-999/1000", 900, original)))
            }
    }

    @Test fun offsetsAndTotalsAboveFourGiBAreNotTruncatedToIntegers() {
        val offset = 4_294_967_296L
        val total = 8_589_934_592L
        assertEquals(ResumeDecision.APPEND, HttpResumePolicy.evaluate(ResumeRequest(offset, total, original),
            HttpResponseMetadata(206, "bytes $offset-${total - 1}/$total", total - offset, original)))
        assertTrue(HttpResumePolicy.transferFinished(total, total))
        assertFalse(HttpResumePolicy.transferFinished(total, total - 1))
    }

    @Test fun truncatedAndOverrunBodiesNeverBecomeCompletedDownloads() {
        assertTrue(HttpResumePolicy.transferFinished(900, 900))
        assertFalse(HttpResumePolicy.transferFinished(900, 899))
        assertFalse(HttpResumePolicy.transferFinished(900, 901))
        assertFalse(HttpResumePolicy.transferFinished(900, -1))
        assertFalse(HttpResumePolicy.transferFinished(-1, 0))
        assertTrue(HttpResumePolicy.transferFinished(0, 0))
        assertTrue(HttpResumePolicy.transferFinished(null, 100))
    }
}
