package iriscore.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
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
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import iriscore.command.InputMode
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisRadius
import iriscore.designsystem.IrisSizing
import iriscore.designsystem.IrisSpacing
import iriscore.designsystem.IrisType
import iriscore.designsystem.glass.GlassProminence
import iriscore.designsystem.glass.IrisGlassSurface

/**
 * The console input — the single place a user types, whatever they mean.
 *
 * The leading sigil is the whole affordance: it shows `>` for a message, and
 * switches to `/` or `@` as the mode changes under the cursor. The user never
 * picks a mode; typing is the mode switch. Rendered on glass because this is
 * the one persistently floating surface in the app.
 */
@Composable
fun IrisConsoleInput(
    value: String,
    mode: InputMode,
    onValueChange: (String) -> Unit,
    onSubmit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val sigil = when (mode) {
        is InputMode.Command -> "/"
        is InputMode.Context -> "@"
        InputMode.Message -> ">"
    }
    // Only the machine modes are tinted; an ordinary message keeps the neutral
    // prompt so writing to a person never looks like operating a terminal.
    val sigilColor = when (mode) {
        is InputMode.Command -> IrisColors.AccentPrimary
        is InputMode.Context -> IrisColors.AccentSuccess
        InputMode.Message -> IrisColors.TextTertiary
    }

    IrisGlassSurface(
        modifier = modifier.fillMaxWidth(),
        shape = IrisRadius.XL,
        prominence = GlassProminence.Prominent,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = IrisSizing.InputHeight)
                .padding(horizontal = IrisSpacing.LG, vertical = IrisSpacing.MD),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text = sigil, style = IrisType.Command.copy(color = sigilColor))
            Spacer(Modifier.width(IrisSpacing.MD))

            Box(Modifier.weight(1f)) {
                if (value.isEmpty()) {
                    Text(
                        text = "Message, /command, or @peer",
                        style = IrisType.Command.copy(color = IrisColors.TextQuaternary),
                    )
                }
                BasicTextField(
                    value = value,
                    onValueChange = onValueChange,
                    textStyle = if (mode is InputMode.Message) {
                        // Human text is set in the human face; commands and
                        // references stay monospace so they line up as tokens.
                        IrisType.Body
                    } else {
                        IrisType.Command
                    },
                    cursorBrush = SolidColor(IrisColors.AccentPrimary),
                    singleLine = false,
                    maxLines = 5,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                    keyboardActions = KeyboardActions(onSend = { onSubmit() }),
                    modifier = Modifier.fillMaxWidth(),
                )
            }

            // HV-55: the IME "Send" action key is the only way to send, but
            // many keyboards (Gboard in some languages, third-party
            // keyboards, hardware keyboards, some RTL layouts) render that
            // key as a newline/"done" instead and onSend never fires, with
            // no visible affordance to send and no accessible route for
            // TalkBack / motor-impaired users. An explicit button is the
            // fallback that always works; the IME action stays as a shortcut.
            Spacer(Modifier.width(IrisSpacing.SM))
            IconButton(
                onClick = onSubmit,
                enabled = value.isNotBlank(),
                modifier = Modifier
                    .size(IrisSizing.InputHeight)
                    .semantics { contentDescription = "Send" },
            ) {
                Icon(
                    imageVector = Icons.AutoMirrored.Filled.Send,
                    contentDescription = null,
                    tint = if (value.isNotBlank()) IrisColors.AccentPrimary else IrisColors.TextQuaternary,
                )
            }
        }
    }
}
