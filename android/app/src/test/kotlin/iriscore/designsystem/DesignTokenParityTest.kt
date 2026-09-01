package iriscore.designsystem

import androidx.compose.ui.graphics.Color
import iriscore.command.CommandRegistry
import java.io.File
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * Guards the Android/desktop design-system boundary.
 *
 * The two platforms render with different toolkits, so the tokens and the
 * command surface are mirrored rather than shared at build time. Mirrored
 * values drift silently, which is exactly how a "single design system" quietly
 * becomes two. These tests parse the desktop sources and fail the Android build
 * when the two disagree.
 *
 * They also assert the WCAG contrast floor, so nobody can darken a text tier
 * back below the readable threshold without the build objecting.
 */
class DesignTokenParityTest {

    private val repoRoot: File by lazy {
        // Test working directory is android/app.
        generateSequence(File("").absoluteFile) { it.parentFile }
            .first { File(it, "crates/iris-desktop/ui").isDirectory }
    }

    private val tokensCss: String by lazy {
        File(repoRoot, "crates/iris-desktop/ui/tokens.css").readText()
    }

    private val commandsJs: String by lazy {
        File(repoRoot, "crates/iris-desktop/ui/commands.js").readText()
    }

    private fun cssVar(name: String): String {
        val match = Regex("--$name:\\s*(#[0-9a-fA-F]{6})").find(tokensCss)
            ?: error("token --$name not found in tokens.css")
        return match.groupValues[1].lowercase()
    }

    /**
     * Hex from the float channels rather than `toArgb()`: this is a pure-JVM
     * unit test and `android.graphics.Color` is a returns-default stub here.
     */
    private fun Color.hex(): String = "#%02x%02x%02x".format(
        Math.round(red * 255f),
        Math.round(green * 255f),
        Math.round(blue * 255f),
    )

    // -- colour parity -----------------------------------------------------

    @Test
    fun `colour tokens match the desktop mirror`() {
        val pairs = listOf(
            "iris-bg-primary" to IrisColors.BackgroundPrimary,
            "iris-bg-secondary" to IrisColors.BackgroundSecondary,
            "iris-surface-primary" to IrisColors.SurfacePrimary,
            "iris-surface-secondary" to IrisColors.SurfaceSecondary,
            "iris-surface-raised" to IrisColors.SurfaceRaised,
            "iris-text-primary" to IrisColors.TextPrimary,
            "iris-text-secondary" to IrisColors.TextSecondary,
            "iris-text-tertiary" to IrisColors.TextTertiary,
            "iris-text-quaternary" to IrisColors.TextQuaternary,
            "iris-accent-primary" to IrisColors.AccentPrimary,
            "iris-accent-success" to IrisColors.AccentSuccess,
            "iris-accent-warning" to IrisColors.AccentWarning,
            "iris-accent-critical" to IrisColors.AccentCritical,
        )
        pairs.forEach { (token, color) ->
            assertEquals(
                cssVar(token),
                color.hex(),
                "design token --$token has drifted from IrisColors",
            )
        }
    }

    // -- accessibility -----------------------------------------------------

    private fun channel(c: Double): Double =
        if (c > 0.04045) Math.pow((c + 0.055) / 1.055, 2.4) else c / 12.92

    private fun luminance(hex: String): Double {
        val v = hex.removePrefix("#").toInt(16)
        val r = channel(((v shr 16) and 0xFF) / 255.0)
        val g = channel(((v shr 8) and 0xFF) / 255.0)
        val b = channel((v and 0xFF) / 255.0)
        return 0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    private fun contrast(fg: String, bg: String): Double {
        val a = luminance(fg)
        val b = luminance(bg)
        return (maxOf(a, b) + 0.05) / (minOf(a, b) + 0.05)
    }

    @Test
    fun `every text tier clears WCAG AA on both backgrounds`() {
        val backgrounds = listOf(
            IrisColors.BackgroundPrimary.hex(),
            IrisColors.SurfacePrimary.hex(),
        )
        val tiers = mapOf(
            "TextPrimary" to IrisColors.TextPrimary,
            "TextSecondary" to IrisColors.TextSecondary,
            "TextTertiary" to IrisColors.TextTertiary,
            "TextQuaternary" to IrisColors.TextQuaternary,
        )
        tiers.forEach { (name, color) ->
            backgrounds.forEach { bg ->
                val ratio = contrast(color.hex(), bg)
                assertTrue(
                    ratio >= 4.5,
                    "$name on $bg is ${"%.2f".format(ratio)}:1, below the 4.5:1 AA floor",
                )
            }
        }
    }

    // -- command-surface parity --------------------------------------------

    @Test
    fun `command set matches the desktop mirror`() {
        // A user who learns /sos on the phone must find it on the desktop.
        val desktopNames = Regex("name:\\s*\"([a-z0-9]+)\"")
            .findAll(commandsJs)
            .map { it.groupValues[1] }
            .toSet()
        val androidNames = CommandRegistry.commands.map { it.name }.toSet()
        assertEquals(
            androidNames,
            desktopNames,
            "the two platforms advertise different commands",
        )
    }

    @Test
    fun `priority constants match the desktop mirror`() {
        fun jsConst(name: String): Int =
            Regex("const $name = (\\d+)").find(commandsJs)
                ?.groupValues?.get(1)?.toInt()
                ?: error("$name not found in commands.js")

        assertEquals(
            iriscore.command.CommandExecutor.PRIORITY_SOS.toInt(),
            jsConst("IRIS_PRIORITY_SOS"),
        )
        assertEquals(
            iriscore.command.CommandExecutor.PRIORITY_NORMAL.toInt(),
            jsConst("IRIS_PRIORITY_NORMAL"),
        )
        assertEquals(
            iriscore.command.CommandExecutor.PEER_ID_HEX_LENGTH,
            jsConst("IRIS_PEER_ID_HEX_LENGTH"),
        )
    }
}
