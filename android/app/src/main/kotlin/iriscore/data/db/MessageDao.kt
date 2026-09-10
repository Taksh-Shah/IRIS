package iriscore.data.db

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import kotlinx.coroutines.flow.Flow

@Dao
interface MessageDao {

    /** Insert a message; silently ignore duplicate uids (idempotent on replay). */
    @Insert(onConflict = OnConflictStrategy.IGNORE)
    suspend fun insert(message: MessageEntity)

    /**
     * Most-recent [limit] rows ordered oldest-first for display.
     * Emits a new list on every change — the repository maps this to
     * [iriscore.ui.state.MeshUiState.messages].
     */
    @Query("SELECT * FROM messages ORDER BY timestampMs ASC LIMIT :limit")
    fun observeRecent(limit: Int = 500): Flow<List<MessageEntity>>

    /** Update a message's delivery status after an ACK or failure. */
    @Query("UPDATE messages SET deliveryStatus = :status WHERE uid = :uid")
    suspend fun updateStatus(uid: Long, status: String)

    /** WP12: look up one message for retry/copy/info actions. */
    @Query("SELECT * FROM messages WHERE uid = :uid")
    suspend fun getById(uid: Long): MessageEntity?

    /** WP12: local-only deletion — never touches the sender's own copy. */
    @Query("DELETE FROM messages WHERE uid = :uid")
    suspend fun delete(uid: Long)

    /**
     * Highest persisted `uid`, or `null` if the table is empty. Used to reseed
     * [iriscore.ui.state.InboxUiMessage]'s in-memory uid counter on startup —
     * see that class's `seedUidCounter` for why this matters.
     */
    @Query("SELECT MAX(uid) FROM messages")
    suspend fun maxUid(): Long?
}
