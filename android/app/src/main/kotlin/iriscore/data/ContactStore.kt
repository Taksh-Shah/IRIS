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
 * Stored as JSON in `filesDir/iris_contacts.json`, the same pattern as
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
class ContactStore @Inject constructor(
    @ApplicationContext context: Context,
) {
    private val file = File(context.filesDir, FILE_NAME)

    @Synchronized
    fun all(): Map<String, String> {
        if (!file.exists()) return emptyMap()
        val o = runCatching { JSONObject(file.readText()) }.getOrElse { return emptyMap() }
        return buildMap { o.keys().forEach { put(it, o.getString(it)) } }
    }

    @Synchronized
    fun save(peerIdHex: String, name: String) {
        val o = if (file.exists()) {
            runCatching { JSONObject(file.readText()) }.getOrDefault(JSONObject())
        } else JSONObject()
        o.put(peerIdHex.lowercase(), name.trim())
        file.writeText(o.toString())
    }

    @Synchronized
    fun resolve(peerIdHex: String): String? = all()[peerIdHex.lowercase()]

    @Synchronized
    fun remove(peerIdHex: String) {
        if (!file.exists()) return
        val o = runCatching { JSONObject(file.readText()) }.getOrElse { return }
        o.remove(peerIdHex.lowercase())
        file.writeText(o.toString())
    }

    private companion object {
        const val FILE_NAME = "iris_contacts.json"
    }
}
