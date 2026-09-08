package iriscore.data

import com.google.common.truth.Truth.assertThat
import org.junit.jupiter.api.Test

class AliasPolicyTest {
    @Test
    fun `normalizes safe unicode names`() {
        assertThat(AliasPolicy.normalize("  Ra\u0301hul  ")).isEqualTo("Ráhul")
        assertThat(AliasPolicy.comparisonKey("RAHUL")).isEqualTo("rahul")
    }

    @Test
    fun `rejects reserved dangerous and peer id shaped names`() {
        assertThat(AliasPolicy.normalize("Rahul/peer")).isNull()
        assertThat(AliasPolicy.normalize("Rahul\npeer")).isNull()
        assertThat(AliasPolicy.normalize("a".repeat(64))).isNull()
        assertThat(AliasPolicy.normalize("🙂".repeat(33))).isNull()
    }
}
