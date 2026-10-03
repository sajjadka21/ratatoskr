package app.ratatoskr.android

/** Process-local ownership complements SQLite's durable journal. Activities,
 * foreground services and scheduled slices must never execute the same job twice. */
object MobileRuntime {
    private val initialized = mutableSetOf<String>()
    private val owners = mutableMapOf<String, TransferControl>()
    private val groups = mutableMapOf<String, String>()
    private val resumes = mutableSetOf<String>()
    @Synchronized fun initialize(store: TaskStore) {
        if (initialized.add(store.databaseName)) store.recover()
    }
    @Synchronized fun claim(id: String, control: TransferControl, limit: Int = 3, groupName: String = ""): Boolean {
        if (id in owners || owners.size >= limit.coerceIn(1, 3) || (groupName.isNotEmpty() && groupName in groups.values)) return false
        owners[id] = control; groups[id] = groupName; return true
    }
    @Synchronized fun busy(id: String) = id in owners
    @Synchronized fun stop(id: String) { owners[id]?.stop() }
    @Synchronized fun requestResume(id: String) { resumes.add(id) }
    @Synchronized fun clearResume(id: String) { resumes.remove(id) }
    @Synchronized fun release(id: String): Boolean { owners.remove(id); groups.remove(id); return resumes.remove(id) }
}
