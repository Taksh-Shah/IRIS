package iriscore.data

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import org.json.JSONObject
import java.io.File
import javax.inject.Inject
import javax.inject.Singleton

/**
 * HV-56: durable contact book — `nodeIdHex -> user-assigned name`.
 *
 * Stored as JSON in `noBackupFilesDir/iris_contacts.json`, beside the local
 * trust database so aliases cannot be restored onto a different identity.
 * [KnownPeersStore]. Contacts persist across restarts and are loaded once
 * by the ViewModel at init. Each `/name <peerId> <alias>` write is reflected
 * immediately in the ViewModel's [contactsFlow] so the UI updates without a
 * restart.
 *
 * Deliberately not Room: the contact book is a small, flat map with no
 * relational queries; a keyed JSON file avoids the migration overhead and
 * an extra Gradle dependency for this use case.
 */
@Singleton
class ContactStore private constructor(
    private val file: File,
    legacy: File?,
) {
    @Inject constructor(
        @ApplicationContext context: Context,
    ) : this(File(context.noBackupFilesDir, FILE_NAME), File(context.filesDir, FILE_NAME))

    internal constructor(file: File) : this(file, null)

    init {
        if (!file.exists() && legacy?.exists() == true) {
            runCatching {
                AtomicTextFile.write(file, legacy.readText())
                legacy.delete()
            }
        }
    }

    @Synchronized
    fun all(): Map<String, String> {
        if (!file.exists()) return emptyMap()
        val o = runCatching { JSONObject(file.readText()) }.getOrElse { return emptyMap() }
        return buildMap {
            o.keys().forEach { id ->
                runCatching {
                    val alias = AliasPolicy.normalize(o.getString(id)) ?: return@runCatching
                    put(id.lowercase(), alias)
                }
            }
        }
    }

    /**
     * @return `true` if saved; `false` if `name` is already bound to a
     * *different* peer (case-insensitive) — a duplicate alias is refused
     * rather than silently accepted, because contact resolution
     * (`MeshViewModel`'s `/to <name>` lookup) picks whichever entry it finds
     * first: a second peer quietly claiming an existing alias would make
     * `/to <name>` non-deterministically address the wrong peer. Renaming
     * the *same* peer under a new name, or re-saving its own existing name,
     * is always allowed.
     */
    @Synchronized
    fun save(peerIdHex: String, name: String): Boolean {
        val id = peerIdHex.lowercase()
        val trimmed = AliasPolicy.normalize(name) ?: return false
        val o = if (file.exists()) {
            runCatching { JSONObject(file.readText()) }.getOrDefault(JSONObject())
        } else JSONObject()
        val collision = o.keys().asSequence().any { existingId ->
            existingId != id && runCatching {
                AliasPolicy.comparisonKey(o.getString(existingId)) == AliasPolicy.comparisonKey(trimmed)
            }.getOrDefault(false)
        }
        if (collision) return false
        o.put(id, trimmed)
        AtomicTextFile.write(file, o.toString())
        return true
    }

    @Synchronized
    fun isAliasAvailable(peerIdHex: String, name: String): Boolean {
        val normalized = AliasPolicy.normalize(name) ?: return false
        val id = peerIdHex.lowercase()
        return all().none { (existingId, existingName) ->
            existingId != id &&
                AliasPolicy.comparisonKey(existingName) == AliasPolicy.comparisonKey(normalized)
        }
    }

    @Synchronized
    fun resolve(peerIdHex: String): String? = all()[peerIdHex.lowercase()]

    @Synchronized
    fun remove(peerIdHex: String) {
        if (!file.exists()) return
        val o = runCatching { JSONObject(file.readText()) }.getOrElse { return }
        o.remove(peerIdHex.lowercase())
        AtomicTextFile.write(file, o.toString())
    }

    private companion object {
        const val FILE_NAME = "iris_contacts.json"
    }
}
