package app.ratatoskr.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Regression contracts for persisted jobs, mobile data consent, and media output. */
class RecoveryPolicyTest {
    @Test fun interruptedWorkBecomesPausedInsteadOfBeingLostOrReportedComplete() {
        listOf(TaskState.PROBING, TaskState.DOWNLOADING, TaskState.MERGING, TaskState.SAVING)
            .forEach { interrupted ->
                assertEquals("Recovery of $interrupted", TaskState.PAUSED, TaskPolicy.recover(interrupted))
            }
    }

    @Test fun recoveryPreservesExplicitUserPauseAndPendingDecisions() {
        listOf(TaskState.PAUSED, TaskState.QUEUED, TaskState.WAITING_NETWORK, TaskState.NEEDS_SELECTION)
            .forEach { stored -> assertEquals("Recovery of $stored", stored, TaskPolicy.recover(stored)) }
    }

    @Test fun recoveryNeverRetriesFailedOrCancelledWorkWithoutUserConsent() {
        listOf(TaskState.FAILED, TaskState.CANCELLED, TaskState.COMPLETED)
            .forEach { terminal -> assertEquals("Recovery of $terminal", terminal, TaskPolicy.recover(terminal)) }
    }

    @Test fun aSecondRecoveryDoesNotChangeTheFirstRecoveredState() {
        TaskState.values().forEach { original ->
            val recovered = TaskPolicy.recover(original)
            assertEquals("Repeated recovery of $original", recovered, TaskPolicy.recover(recovered))
        }
    }

    @Test fun noNetworkBlocksEveryDownloadPolicyEvenWithStaleTransportFlags() {
        NetworkPolicy.values().forEach { policy ->
            listOf(false, true).forEach { wifi ->
                listOf(false, true).forEach { metered ->
                    assertFalse("Disconnected $policy, wifi=$wifi, metered=$metered",
                        TaskPolicy.mayRun(policy, NetworkSnapshot(false, wifi, metered)))
                }
            }
        }
    }

    @Test fun wifiOnlyNeverFallsBackToCellularEvenIfCellularIsUnmetered() {
        assertFalse(TaskPolicy.mayRun(NetworkPolicy.WIFI_ONLY, NetworkSnapshot(true, false, true)))
        assertFalse(TaskPolicy.mayRun(NetworkPolicy.WIFI_ONLY, NetworkSnapshot(true, false, false)))
    }

    @Test fun wifiOnlyAndUnmeteredAreDifferentChoicesForAChargedHotspot() {
        val chargedHotspot = NetworkSnapshot(connected = true, wifi = true, metered = true)
        assertTrue(TaskPolicy.mayRun(NetworkPolicy.WIFI_ONLY, chargedHotspot))
        assertFalse(TaskPolicy.mayRun(NetworkPolicy.UNMETERED, chargedHotspot))
    }

    @Test fun unmeteredPolicyAcceptsUnmeteredConnectionsRegardlessOfTransport() {
        assertTrue(TaskPolicy.mayRun(NetworkPolicy.UNMETERED, NetworkSnapshot(true, true, false)))
        assertTrue(TaskPolicy.mayRun(NetworkPolicy.UNMETERED, NetworkSnapshot(true, false, false)))
        assertFalse(TaskPolicy.mayRun(NetworkPolicy.UNMETERED, NetworkSnapshot(true, false, true)))
    }

    @Test fun anyNetworkPolicyAllowsMeteredMobileDataOnlyWhenConnected() {
        assertTrue(TaskPolicy.mayRun(NetworkPolicy.ANY, NetworkSnapshot(true, false, true)))
        assertTrue(TaskPolicy.mayRun(NetworkPolicy.ANY, NetworkSnapshot(true, true, false)))
    }

    @Test fun aWifiToMobileHandoverRevokesWifiOnlyPermission() {
        assertTrue(TaskPolicy.mayRun(NetworkPolicy.WIFI_ONLY, NetworkSnapshot(true, true, false)))
        assertFalse(TaskPolicy.mayRun(NetworkPolicy.WIFI_ONLY, NetworkSnapshot(true, false, true)))
        assertTrue(TaskPolicy.mayRun(NetworkPolicy.WIFI_ONLY, NetworkSnapshot(true, true, false)))
    }

    @Test fun offeredQualityLabelsKeepTheActual540PixelSourceHeight() {
        assertEquals(listOf(540), LinkUtils.offeredHeights(listOf(540)))
        assertEquals(listOf(1080, 720, 540, 360),
            LinkUtils.offeredHeights(listOf(360, 540, 720, 1080, 540, null, 0, -1)))
    }

    @Test fun selectedVideoQualityCannotSilentlyUseAnUnboundedBestFallback() {
        listOf(360, 480, 540, 720).forEach { height ->
            assertBoundedVideoFormat(LinkUtils.videoFormat(height), height)
            assertBoundedVideoFormat(MediaOptions.format(height, audioOnly = false), height)
        }
    }

    @Test fun audioOnlyAlwaysRequestsExtractionEvenIfTheSourceHasOnlyMuxedVideo() {
        listOf<Int?>(null, 480, 1080).forEach { height ->
            val selected = MediaOptions.format(height, audioOnly = true)
            assertTrue("Prefer an audio stream when it exists: $selected", selected.startsWith("bestaudio"))
            assertFalse("Do not explicitly request a video/audio merge: $selected", selected.contains("+ba"))
            assertTrue("Muxed fallback still requires extraction", MediaOptions.requiresAudioExtraction(true))
        }
    }

    @Test fun videoDownloadsDoNotExtractAwayTheirVideoStream() {
        assertFalse(MediaOptions.requiresAudioExtraction(false))
        assertTrue(MediaOptions.format(null, audioOnly = false).isNotBlank())
    }

    @Test fun roamingIsRefusedByDefaultForEveryNetworkPolicy() {
        val roamingNetwork = NetworkSnapshot(true, wifi = true, metered = false, roaming = true)
        NetworkPolicy.values().forEach { policy ->
            assertFalse("Roaming needs explicit consent for $policy", TaskPolicy.mayRun(policy, roamingNetwork))
            assertTrue("Explicit roaming consent for $policy",
                TaskPolicy.mayRun(policy, roamingNetwork, allowRoaming = true))
        }
    }

    @Test fun roamingConsentDoesNotOverrideWifiOnlyOrOfflineRestrictions() {
        assertFalse(TaskPolicy.mayRun(NetworkPolicy.WIFI_ONLY,
            NetworkSnapshot(true, wifi = false, metered = false, roaming = true), allowRoaming = true))
        assertFalse(TaskPolicy.mayRun(NetworkPolicy.ANY,
            NetworkSnapshot(false, wifi = true, metered = false, roaming = true), allowRoaming = true))
    }

    @Test fun instagramTrackingAndMobileHostVariantsHaveOneCanonicalIdentity() {
        val variants = listOf(
            "https://www.instagram.com/reel/Ab_C-123/?igsh=first",
            "https://m.instagram.com/reel/Ab_C-123?utm_source=share&igsh=second",
            "http://instagram.com/reels/Ab_C-123/?utm_medium=copy_link#comment",
        )
        assertEquals(setOf("https://www.instagram.com/reel/Ab_C-123/"),
            variants.map { LinkUtils.canonicalUrl(it) }.toSet())
    }

    @Test fun differentInstagramPostIdsAreNeverDeduplicatedTogether() {
        assertFalse(LinkUtils.canonicalUrl("https://instagram.com/reel/one/?igsh=x") ==
            LinkUtils.canonicalUrl("https://instagram.com/reel/two/?igsh=x"))
    }

    @Test fun userInfoCredentialsCannotEnterThePersistentDownloadQueue() {
        listOf(
            "https://user:password@example.com/file.zip",
            "https://user@example.com/file.zip",
            "https://user%3Apassword@example.com/file.zip",
        ).forEach { address ->
            assertFalse("Credential-bearing URL must be refused", LinkUtils.isPublicHttpUrl(address))
            try {
                LinkUtils.canonicalUrl(address)
                throw AssertionError("Canonical URL accepted credentials")
            } catch (expected: IllegalArgumentException) {
                // Canonicalization must not accidentally persist a rejected address.
            }
        }
    }

    @Test fun encodedCredentialQueryKeysAreRefusedBeforeTheyCanBeStored() {
        listOf("access%5Ftoken", "%61uthorization", "pass%77ord", "session%69d", "%63ookie")
            .forEach { key ->
                assertFalse("Credential query key must be decoded before screening: $key",
                    LinkUtils.isPublicHttpUrl("https://example.com/file?$key=private"))
            }
    }

    @Test fun oneShareCanContainSeveralDifferentUrlsWithoutTrailingPunctuation() {
        assertEquals(listOf("https://example.com/first.mp4", "https://example.com/second.mp4"),
            LinkUtils.extractUrls("First (https://example.com/first.mp4), second https://example.com/second.mp4!"))
        assertEquals(emptyList<String>(), LinkUtils.extractUrls(null))
    }

    @Test fun shareIntakeCapsDistinctUrlsAtFiftyWithoutLettingDuplicatesUseTheLimit() {
        val urls = (1..75).map { "https://example.com/video/$it.mp4" }
        val text = List(10) { urls.first() }.joinToString(" ") + " " + urls.joinToString("\n")
        assertEquals(urls.take(50), LinkUtils.extractUrls(text))
    }

    @Test fun genericSignedDownloadQueriesSurviveCanonicalizationExactly() {
        val signed = "https://files.example.com/archive.zip?X-Amz-Signature=abcdef%2B0123&Expires=123456&key=a%2Fb+space"
        assertEquals(signed, LinkUtils.canonicalUrl(signed))
        assertEquals(signed, LinkUtils.canonicalUrl("$signed#presentation-only-fragment"))
    }

    @Test fun instagramLookingThirdPartyHostsDoNotLoseTheirSignedQuery() {
        val address = "https://instagram.com.example.org/reel/Ab_C-123/?signature=keep%2Bme"
        assertEquals(address, LinkUtils.canonicalUrl(address))
    }

    @Test fun sharedGenericSignedQueryCanEndInValidPunctuation() {
        listOf("!", ".", ";", ":", "?").forEach { suffix ->
            val signed = "https://files.example.org/archive.zip?signature=keep$suffix"
            assertEquals("Do not trim a valid query value", signed, LinkUtils.extractUrl("$signed "))
            assertEquals("Batch intake also keeps the signature", listOf(signed), LinkUtils.extractUrls("$signed\n"))
        }
    }

    @Test fun instagramPostAndReelPathsDeduplicateByContentWithoutChangingTheirUrls() {
        val post = "https://www.instagram.com/p/Ab_C-123/?igsh=post"
        val reel = "https://m.instagram.com/reel/Ab_C-123/?igsh=reel"
        assertEquals(LinkUtils.contentIdentity(post), LinkUtils.contentIdentity(reel))
        assertEquals("https://www.instagram.com/p/Ab_C-123/", LinkUtils.canonicalUrl(post))
        assertEquals("https://www.instagram.com/reel/Ab_C-123/", LinkUtils.canonicalUrl(reel))
        assertFalse(LinkUtils.contentIdentity(post) ==
            LinkUtils.contentIdentity("https://instagram.com/reel/different/"))
    }

    @Test fun nonInstagramIdentityKeepsDifferentSignedQueriesDistinct() {
        val first = "https://files.example.org/archive.zip?signature=one"
        val second = "https://files.example.org/archive.zip?signature=two"
        assertFalse(LinkUtils.contentIdentity(first) == LinkUtils.contentIdentity(second))
        assertEquals(LinkUtils.contentIdentity(first), LinkUtils.contentIdentity("$first#display-fragment"))
    }

    private fun assertBoundedVideoFormat(format: String, height: Int) {
        assertTrue("No empty quality selector", format.isNotBlank())
        // Every fallback remains capped; a trailing '/b' used to override the user's choice.
        format.split('/').forEach { alternative ->
            assertTrue("Unbounded fallback for ${height}p: $alternative", alternative.contains("height<=$height"))
        }
    }
}
