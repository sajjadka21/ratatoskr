package app.ratatoskr.android

/** Each transfer checks both user consent and network policy between operations. */
class TransferControl(private val allowed: () -> Boolean) {
    @Volatile private var stopped = false
    fun stop() { stopped = true }
    fun check() {
        if (stopped || Thread.currentThread().isInterrupted) throw TransferFailure("interrupted")
        if (!allowed()) throw TransferFailure("waiting_network")
    }
}
