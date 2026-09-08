package iriscore.data

import java.text.Normalizer
import java.util.Locale

/** Canonical validation for local contact labels. Aliases are never trust anchors. */
object AliasPolicy {
    const val MAX_CODE_POINTS = 32
    private val peerIdPattern = Regex("^[0-9a-fA-F]{64}$")

    fun normalize(raw: String): String? {
        val value = Normalizer.normalize(raw.trim(), Normalizer.Form.NFC)
        val count = value.codePointCount(0, value.length)
        if (count !in 1..MAX_CODE_POINTS) return null
        if (value.any { Character.isISOControl(it) || it == '/' || it == '@' }) return null
        if (peerIdPattern.matches(value)) return null
        return value
    }

    fun comparisonKey(value: String): String =
        Normalizer.normalize(value, Normalizer.Form.NFC).lowercase(Locale.ROOT)
}
