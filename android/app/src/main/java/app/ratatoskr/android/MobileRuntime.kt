package app.ratatoskr.android

/** Process-local ownership complements SQLite's durable journal. Activities,
 * foreground services and scheduled slices must never execute the same job twice. */
object MobileRuntime {
    private val initialized = mutableSetOf<String>()
    private val owners = mutableMapOf<String, TransferControl>()
    private val resumes = mutableSetOf<String>()
    @Synchronized fun initialize(store: TaskStore) {
        if (initialized.add(store.databaseName)) store.recover()
    }
    @Synchronized fun claim(id: String, control: TransferControl): Boolean {
        if (id in owners) return false
        owners[id] = control; return true
    }
    @Synchronized fun busy(id: String) = id in owners
    @Synchronized fun stop(id: String) { owners[id]?.stop() }
    @Synchronized fun requestResume(id: String) { resumes.add(id) }
    @Synchronized fun clearResume(id: String) { resumes.remove(id) }
    @Synchronized fun release(id: String): Boolean { owners.remove(id); return resumes.remove(id) }
}
