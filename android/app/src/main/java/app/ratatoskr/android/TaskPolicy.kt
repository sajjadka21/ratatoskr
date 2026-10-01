package app.ratatoskr.android

enum class TaskState { QUEUED, PROBING, DOWNLOADING, MERGING, SAVING, PAUSED, WAITING_NETWORK, NEEDS_SELECTION, FAILED, COMPLETED, CANCELLED }
enum class NetworkPolicy { ANY, WIFI_ONLY, UNMETERED }
data class NetworkSnapshot(val connected: Boolean, val wifi: Boolean, val metered: Boolean, val roaming: Boolean = false)

object TaskPolicy {
    val inFlight = setOf(TaskState.PROBING, TaskState.DOWNLOADING, TaskState.MERGING, TaskState.SAVING)
    fun recover(state: TaskState): TaskState = if (state in inFlight) TaskState.PAUSED else state
    fun mayRun(policy: NetworkPolicy, network: NetworkSnapshot, allowRoaming: Boolean = false): Boolean =
        network.connected && (allowRoaming || !network.roaming) && when (policy) {
            NetworkPolicy.ANY -> true
            NetworkPolicy.WIFI_ONLY -> network.wifi
            NetworkPolicy.UNMETERED -> !network.metered
        }
}

object MediaOptions {
    fun format(height: Int?, audioOnly: Boolean): String =
        if (audioOnly) "bestaudio[ext=m4a]/bestaudio/best" else LinkUtils.videoFormat(height)
    fun requiresAudioExtraction(audioOnly: Boolean): Boolean = audioOnly
    fun guardedFormat(height: Int?, audioOnly: Boolean): String = format(height, audioOnly)
        .split('/').joinToString("/") { fallback -> fallback.split('+').joinToString("+") {
            it + "[protocol~='^(https?|m3u8_native|http_dash_segments)$']"
        } }
}
