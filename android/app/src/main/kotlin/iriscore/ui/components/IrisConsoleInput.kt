package iriscore.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import iriscore.command.InputMode
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisRadius
import iriscore.designsystem.IrisSizing
import iriscore.designsystem.IrisSpacing
import iriscore.designsystem.IrisType
import iriscore.designsystem.glass.GlassProminence
import iriscore.designsystem.glass.IrisGlassSurface

/**
 * The console input — message mode looks like a chat input; command/context
 * modes expose the sigil and monospace style so they read as terminal tokens.
 */
@Composable
fun IrisConsoleInput(
    value: String,
    mode: InputMode,
    onValueChange: (String) -> Unit,
    onSubmit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val isMessage = mode is InputMode.Message

    // Sigil is only shown for non-message modes — a plain chat input needs no prompt glyph.
    val sigil = when (mode) {
        is InputMode.Command -> "/"
        is InputMode.Context -> "@"
        InputMode.Message -> null
    }
    val sigilColor = when (mode) {
        is InputMode.Command -> IrisColors.AccentPrimary
        is InputMode.Context -> IrisColors.AccentSuccess
        InputMode.Message -> IrisColors.TextTertiary
    }
    val placeholder = when (mode) {
        InputMode.Message -> "Type a message…"
        is InputMode.Command -> "command"
        is InputMode.Context -> "peer id or name"
    }

    val canSend = value.isNotBlank()

    IrisGlassSurface(
        modifier = modifier.fillMaxWidth(),
        shape = IrisRadius.XL,
        prominence = GlassProminence.Prominent,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = IrisSizing.InputHeight)
                .padding(start = IrisSpacing.LG, end = IrisSpacing.XS, top = IrisSpacing.SM, bottom = IrisSpacing.SM),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (sigil != null) {
                Text(text = sigil, style = IrisType.Command.copy(color = sigilColor))
                Spacer(Modifier.width(IrisSpacing.SM))
            }

            Box(Modifier.weight(1f)) {
                if (value.isEmpty()) {
                    Text(
                        text = placeholder,
                        style = if (isMessage) {
                            IrisType.Body.copy(color = IrisColors.TextQuaternary)
                        } else {
                            IrisType.Command.copy(color = IrisColors.TextQuaternary)
                        },
                    )
                }
                BasicTextField(
                    value = value,
                    onValueChange = onValueChange,
                    textStyle = if (isMessage) IrisType.Body else IrisType.Command,
                    cursorBrush = SolidColor(IrisColors.AccentPrimary),
                    singleLine = false,
                    maxLines = 5,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                    keyboardActions = KeyboardActions(onSend = { onSubmit() }),
                    modifier = Modifier.fillMaxWidth(),
                )
            }

            // Send button: filled accent circle when there is text (WhatsApp / Messenger pattern).
            Spacer(Modifier.width(IrisSpacing.XS))
            Box(
                modifier = Modifier
                    .size(40.dp)
                    .clip(CircleShape)
                    .background(
                        if (canSend) IrisColors.AccentPrimary else IrisColors.SurfaceRaised,
                    )
                    .semantics { contentDescription = "Send" },
                contentAlignment = Alignment.Center,
            ) {
                IconButton(
                    onClick = onSubmit,
                    enabled = canSend,
                    modifier = Modifier.size(40.dp),
                ) {
                    Icon(
                        imageVector = Icons.AutoMirrored.Filled.Send,
                        contentDescription = null,
                        tint = if (canSend) IrisColors.BackgroundPrimary else IrisColors.TextQuaternary,
                        modifier = Modifier.size(18.dp),
                    )
                }
            }
        }
    }
}
