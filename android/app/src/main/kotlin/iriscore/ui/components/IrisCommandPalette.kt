package iriscore.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import iriscore.command.CommandGroup
import iriscore.command.IrisCommand
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisMotion
import iriscore.designsystem.IrisRadius
import iriscore.designsystem.IrisSizing
import iriscore.designsystem.IrisSpacing
import iriscore.designsystem.IrisType
import iriscore.designsystem.glass.GlassProminence
import iriscore.designsystem.glass.IrisGlassSurface

/**
 * The command surface.
 *
 * Opens the moment the input line starts with `/` — there is no button to find
 * and no mode to enter. Commands are grouped, ranked by the registry, and
 * selectable by tap (mobile) or by the arrow keys and Enter (desktop), so every
 * command has both a keyboard and a touch route to execution.
 */
@Composable
fun IrisCommandPalette(
    visible: Boolean,
    commands: List<IrisCommand>,
    selectedIndex: Int,
    onSelect: (IrisCommand) -> Unit,
    modifier: Modifier = Modifier,
) {
    AnimatedVisibility(
        visible = visible,
        // Scale from just under 1 with a fade: the panel settles into place
        // rather than popping. No translation — it is anchored to the input.
        enter = fadeIn(tween(IrisMotion.Normal, easing = IrisMotion.Standard)) +
            scaleIn(
                initialScale = IrisMotion.PanelEnterScale,
                animationSpec = tween(IrisMotion.Normal, easing = IrisMotion.Standard),
            ),
        exit = fadeOut(tween(IrisMotion.Fast, easing = IrisMotion.Exit)) +
            scaleOut(
                targetScale = IrisMotion.PanelEnterScale,
                animationSpec = tween(IrisMotion.Fast, easing = IrisMotion.Exit),
            ),
        modifier = modifier,
    ) {
        val listState = rememberLazyListState()

        // Keep the keyboard selection on screen as it moves past the fold.
        LaunchedEffect(selectedIndex) {
            if (selectedIndex >= 0) listState.animateScrollToItem(selectedIndex)
        }

        IrisGlassSurface(
            modifier = Modifier.fillMaxWidth(),
            shape = IrisRadius.LG,
            prominence = GlassProminence.Prominent,
        ) {
            Column(Modifier.padding(vertical = IrisSpacing.SM)) {
                if (commands.isEmpty()) {
                    Text(
                        text = "no matching command",
                        style = IrisType.SystemLine,
                        modifier = Modifier.padding(
                            horizontal = IrisSpacing.LG,
                            vertical = IrisSpacing.SM,
                        ),
                    )
                } else {
                    LazyColumn(
                        state = listState,
                        modifier = Modifier.heightIn(max = IrisSizing.PaletteMaxHeight),
                    ) {
                        var lastGroup: CommandGroup? = null
                        commands.forEachIndexed { index, command ->
                            val group = command.group
                            if (group != lastGroup) {
                                item(key = "group-${group.name}") {
                                    Text(
                                        text = group.label,
                                        style = IrisType.Label,
                                        modifier = Modifier.padding(
                                            start = IrisSpacing.LG,
                                            end = IrisSpacing.LG,
                                            top = IrisSpacing.SM,
                                            bottom = IrisSpacing.XS,
                                        ),
                                    )
                                }
                                lastGroup = group
                            }
                            item(key = command.name) {
                                CommandRow(
                                    command = command,
                                    selected = index == selectedIndex,
                                    onClick = { onSelect(command) },
                                )
                            }
                        }
                    }
                }

                PaletteFooter()
            }
        }
    }
}

@Composable
private fun CommandRow(
    command: IrisCommand,
    selected: Boolean,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(
                if (selected) IrisColors.GlassHighlight else IrisColors.BackgroundPrimary.copy(alpha = 0f),
            )
            .clickable(enabled = command.enabled, onClick = onClick)
            .padding(horizontal = IrisSpacing.LG, vertical = IrisSpacing.SM),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = command.invocation,
            style = IrisType.CommandName.copy(
                color = if (command.enabled) IrisColors.TextPrimary else IrisColors.TextQuaternary,
            ),
        )
        command.argumentHint?.let { hint ->
            Spacer(Modifier.width(IrisSpacing.XS))
            Text(text = hint, style = IrisType.Meta)
        }
        Spacer(Modifier.width(IrisSpacing.MD))
        Text(
            text = command.description,
            style = IrisType.Secondary.copy(color = IrisColors.TextTertiary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}

/** Keyboard affordances. Present on mobile too — harmless, and honest on a tablet keyboard. */
@Composable
private fun PaletteFooter() {
    Box(
        Modifier
            .fillMaxWidth()
            .padding(start = IrisSpacing.LG, end = IrisSpacing.LG, top = IrisSpacing.SM),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(IrisSpacing.MD)) {
            Text(text = "↑↓ navigate", style = IrisType.Shortcut)
            Text(text = "↵ run", style = IrisType.Shortcut)
            Text(text = "esc dismiss", style = IrisType.Shortcut)
        }
    }
}
