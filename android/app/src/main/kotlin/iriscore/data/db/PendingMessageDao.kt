package iriscore.data.db

import androidx.room.Dao
import androidx.room.Delete
import androidx.room.Insert
import androidx.room.Query
import kotlinx.coroutines.flow.Flow

@Dao
interface PendingMessageDao {

    /** Enqueue a message for later delivery; returns the auto-generated row id. */
    @Insert
    suspend fun insert(message: PendingMessageEntity): Long

    /** All pending messages in FIFO order for the drain loop. */
    @Query("SELECT * FROM pending_messages ORDER BY enqueuedAtMs ASC")
    suspend fun getAll(): List<PendingMessageEntity>

    /** Remove one successfully-delivered row. */
    @Delete
    suspend fun delete(message: PendingMessageEntity)

    /** Drop all pending sends for a peer the operator has forgotten. */
    @Query("DELETE FROM pending_messages WHERE recipientHex = :recipientHex COLLATE NOCASE")
    suspend fun removeRecipient(recipientHex: String)

    /** Live count for the relay-queued badge in [iriscore.ui.state.MeshUiState]. */
    @Query("SELECT COUNT(*) FROM pending_messages")
    fun observeCount(): Flow<Int>
}
