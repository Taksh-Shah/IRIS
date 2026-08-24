package iriscore.designsystem

import androidx.compose.runtime.Immutable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.sp

/**
 * Two typographic voices, used for meaning rather than flavour.
 *
 *  - [Sans] carries human communication: names, message bodies, prose.
 *  - [Mono] carries machine surfaces: commands, node ids, status lines,
 *    timestamps, shortcuts, telemetry.
 *
 * The split is the CLI identity. Setting the whole interface in monospace would
 * make IRIS a terminal skin; setting none of it in monospace would erase the
 * distinction between what a person said and what the system did.
 *
 * The families resolve to the platform text stack (Roboto / Roboto Mono).
 * Bundling Inter and JetBrains Mono would sharpen the intended look, but that
 * is an asset-licensing decision rather than a code one, and every style here
 * is defined so that swapping [Sans] and [Mono] is a one-line change.
 */
@Immutable
object IrisType {

    val Sans: FontFamily = FontFamily.SansSerif
    val Mono: FontFamily = FontFamily.Monospace

    // -- human voice -------------------------------------------------------

    /** Message body. Generous line height — this is read, not scanned. */
    val Body = TextStyle(
        fontFamily = Sans,
        fontSize = 15.sp,
        lineHeight = 22.sp,
        fontWeight = FontWeight.Normal,
        letterSpacing = 0.sp,
        color = IrisColors.TextPrimary,
    )

    /** Sender name above a message group. */
    val Sender = TextStyle(
        fontFamily = Sans,
        fontSize = 13.sp,
        lineHeight = 18.sp,
        fontWeight = FontWeight.Medium,
        letterSpacing = 0.1.sp,
        color = IrisColors.TextSecondary,
    )

    /** Screen and section titles. Tight tracking reads as considered. */
    val Title = TextStyle(
        fontFamily = Sans,
        fontSize = 17.sp,
        lineHeight = 22.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = (-0.2).sp,
        color = IrisColors.TextPrimary,
    )

    val Secondary = TextStyle(
        fontFamily = Sans,
        fontSize = 13.sp,
        lineHeight = 18.sp,
        color = IrisColors.TextSecondary,
    )

    // -- machine voice -----------------------------------------------------

    /** What the user types into the console. */
    val Command = TextStyle(
        fontFamily = Mono,
        fontSize = 14.sp,
        lineHeight = 20.sp,
        letterSpacing = 0.sp,
        color = IrisColors.TextPrimary,
    )

    /** Command names in the palette. */
    val CommandName = TextStyle(
        fontFamily = Mono,
        fontSize = 13.sp,
        lineHeight = 18.sp,
        fontWeight = FontWeight.Medium,
        color = IrisColors.TextPrimary,
    )

    /** System event bodies — the `> key   value` lines. */
    val SystemLine = TextStyle(
        fontFamily = Mono,
        fontSize = 12.sp,
        lineHeight = 18.sp,
        color = IrisColors.TextSecondary,
    )

    /**
     * Section labels: THREAD.SYNC, STATUS, IRIS. Wide tracking is what makes an
     * all-caps mono label read as a system heading instead of shouting.
     */
    val Label = TextStyle(
        fontFamily = Mono,
        fontSize = 10.sp,
        lineHeight = 14.sp,
        fontWeight = FontWeight.Medium,
        letterSpacing = 1.4.sp,
        color = IrisColors.TextTertiary,
    )

    /** Timestamps, node ids, counters. */
    val Meta = TextStyle(
        fontFamily = Mono,
        fontSize = 11.sp,
        lineHeight = 15.sp,
        letterSpacing = 0.2.sp,
        color = IrisColors.TextTertiary,
    )

    /** Keyboard hints in the palette footer. */
    val Shortcut = TextStyle(
        fontFamily = Mono,
        fontSize = 10.sp,
        lineHeight = 14.sp,
        letterSpacing = 0.4.sp,
        color = IrisColors.TextQuaternary,
        textAlign = TextAlign.End,
    )
}
