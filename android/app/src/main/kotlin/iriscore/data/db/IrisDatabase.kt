package iriscore.data.db

import androidx.room.Database
import androidx.room.RoomDatabase

/**
 * Durable IRIS store — messages and pending sends survive process death and
 * device reboot.
 *
 * Schema notes:
 *  - exportSchema = false: schema JSON would be generated into the source tree
 *    and must then be committed and validated in CI. Deferred until migration
 *    tooling is in place (WP2 follow-up).
 *  - No SQLCipher: plaintext at-rest. Messages are encrypted end-to-end at the
 *    transport layer; at-rest encryption is a planned hardening step.
 */
@Database(
    entities = [MessageEntity::class, PendingMessageEntity::class],
    version = 1,
    exportSchema = false,
)
abstract class IrisDatabase : RoomDatabase() {
    abstract fun messageDao(): MessageDao
    abstract fun pendingMessageDao(): PendingMessageDao
}
