package iriscore.data

import com.google.common.truth.Truth.assertThat
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File

class ContactStoreTest {
    @TempDir lateinit var directory: File

    @Test
    fun `aliases survive restart and collisions are case insensitive`() {
        val file = File(directory, "contacts.json")
        val first = "11".repeat(32)
        val second = "22".repeat(32)
        val store = ContactStore(file)

        assertThat(store.save(first, " Rahul ")).isTrue()
        assertThat(store.save(second, "RAHUL")).isFalse()
        assertThat(ContactStore(file).resolve(first)).isEqualTo("Rahul")
    }
}
