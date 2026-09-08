package iriscore.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisRadius
import iriscore.designsystem.IrisSpacing
import iriscore.designsystem.IrisType

/**
 * Text primitive for the whole shell.
 *
 * Built on [BasicText] rather than Material's `Text` on purpose: Material would
 * fold in `MaterialTheme` colours and typography, and IRIS's palette is the
 * authority here. Every style already carries its own colour, so nothing needs
 * to inherit from a theme that does not exist.
 */
@Composable
fun Text(
    text: String,
    style: TextStyle,
    modifier: Modifier = Modifier,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Clip,
) {
    BasicText(
        text = text,
        modifier = modifier,
        style = style,
        maxLines = maxLines,
        overflow = overflow,
    )
}

/** Semantic tone for a status chip. Tones map to state, never to decoration. */
enum class StatusTone { Neutral, Active, Warning, Critical }

/**
 * A small state marker: P0, QUEUED, link state.
 *
 * Chips are the one place accent colour appears in the message stream, and only
 * ever to mark state the user must be able to spot at a glance.
 *
 * WP11: [semanticDescription] carries the plain-language meaning ("Delivered
 * to recipient device") for TalkBack, separately from the compact visible
 * [label] ("SENT") — the chip stays terse on screen but is never cryptic to
 * a screen reader. Defaults to [label] when not given (chips with an
 * already-plain label, e.g. "P0", don't need a separate description).
 */
@Composable
fun IrisStatusChip(
    label: String,
    tone: StatusTone = StatusTone.Neutral,
    modifier: Modifier = Modifier,
    semanticDescription: String = label,
) {
    val color = when (tone) {
        StatusTone.Neutral -> IrisColors.TextTertiary
        StatusTone.Active -> IrisColors.AccentSuccess
        StatusTone.Warning -> IrisColors.AccentWarning
        StatusTone.Critical -> IrisColors.AccentCritical
    }

    Text(
        text = label,
        style = IrisType.Label.copy(color = color),
        modifier = modifier
            .background(color.copy(alpha = 0.10f), IrisRadius.SM)
            .border(1.dp, color.copy(alpha = 0.22f), IrisRadius.SM)
            .padding(horizontal = IrisSpacing.SM, vertical = IrisSpacing.XXS)
            .semantics { contentDescription = semanticDescription },
    )
}
