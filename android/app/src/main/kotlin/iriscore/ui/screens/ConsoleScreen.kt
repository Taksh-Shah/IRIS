package iriscore.ui.screens

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import iriscore.command.CommandRegistry
import iriscore.command.ConsoleInputParser
import iriscore.command.InputMode
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisSpacing
import iriscore.designsystem.IrisType
import iriscore.designsystem.glass.ProvideGlassTier
import iriscore.designsystem.glass.glassBackdrop
import iriscore.ui.MeshViewModel
import iriscore.ui.components.IrisCommandPalette
import iriscore.ui.components.IrisConsoleInput
import iriscore.ui.components.IrisMessage
import iriscore.ui.components.IrisStatusChip
import iriscore.ui.components.IrisSystemEvent
import iriscore.ui.components.StatusTone
import iriscore.ui.components.Text
import iriscore.ui.state.ConsoleEntry
import iriscore.ui.state.MeshStatus
import iriscore.ui.state.MeshUiState
import iriscore.util.MeshPermissions

/**
 * The IRIS console — the single screen of the mobile shell.
 *
 * Structure is deliberately flat: a quiet status line, the transcript, and one
 * floating input. Commands, system output and human messages share the stream,
 * so running a command never feels like leaving the conversation.
 */
@Composable
fun ConsoleScreen(viewModel: MeshViewModel = hiltViewModel()) = ProvideGlassTier {
    val state by viewModel.uiState.collectAsStateWithLifecycle()
    val entries by viewModel.console.collectAsStateWithLifecycle()
    val recipient by viewModel.recipient.collectAsStateWithLifecycle()

    var input by rememberSaveable { mutableStateOf("") }
    var selectedCommand by remember { mutableStateOf(0) }

    val mode = remember(input) { ConsoleInputParser.parse(input) }
    val matches = remember(mode) {
        (mode as? InputMode.Command)?.let { CommandRegistry.search(it.token) }.orEmpty()
    }
    val paletteVisible = mode is InputMode.Command && matches.isNotEmpty()

    // Keep the selection in range as the filtered set shrinks under the cursor.
    LaunchedEffect(matches) { selectedCommand = selectedCommand.coerceIn(0, maxOf(0, matches.size - 1)) }

    val context = LocalContext.current
    var permissionsGranted by rememberSaveable { mutableStateOf(MeshPermissions.allGranted(context)) }
    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        // Re-read from the system: permissions already held before the request
        // are absent from the result map.
        permissionsGranted = MeshPermissions.allGranted(context)
    }

    LaunchedEffect(permissionsGranted) {
        if (permissionsGranted) {
            viewModel.ensureStarted()
        } else {
            MeshPermissions.missing(context)
                .takeIf { it.isNotEmpty() }
                ?.let { permissionLauncher.launch(it.toTypedArray()) }
        }
    }

    val listState = rememberLazyListState()
    // Follow the tail as the transcript grows.
    LaunchedEffect(entries.size) {
        if (entries.isNotEmpty()) listState.animateScrollToItem(entries.lastIndex)
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(IrisColors.BackgroundPrimary),
    ) {
        Column(
            Modifier
                .fillMaxSize()
                // The transcript recedes while the palette is open — this is the
                // backdrop half of the glass effect.
                .glassBackdrop(active = paletteVisible),
        ) {
            StatusLine(
                state = state,
                recipient = recipient,
                modifier = Modifier.windowInsetsPadding(WindowInsets.statusBars),
            )

            if (!permissionsGranted) {
                PermissionNotice(
                    onGrant = {
                        MeshPermissions.missing(context)
                            .takeIf { it.isNotEmpty() }
                            ?.let { permissionLauncher.launch(it.toTypedArray()) }
                    },
                )
            }

            LazyColumn(
                state = listState,
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .padding(horizontal = IrisSpacing.Gutter),
            ) {
                items(count = entries.size, key = { entries[it].uid }) { index ->
                    ConsoleRow(entries = entries, index = index)
                }
            }
        }

        // Input and palette float above the transcript, pinned to the bottom.
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .windowInsetsPadding(WindowInsets.ime)
                .windowInsetsPadding(WindowInsets.navigationBars)
                .padding(horizontal = IrisSpacing.MD, vertical = IrisSpacing.MD),
        ) {
            IrisCommandPalette(
                visible = paletteVisible,
                commands = matches,
                selectedIndex = selectedCommand,
                onSelect = { command ->
                    // Completing a command leaves the cursor ready for its
                    // argument rather than running a command that needs one.
                    input = if (command.argumentHint != null) "${command.invocation} " else command.invocation
                },
            )
            if (paletteVisible) Spacer(Modifier.height(IrisSpacing.SM))

            // Failures raised by the engine itself (rejected send, bad
            // recipient) surface here; command-level errors already appear in
            // the transcript as system events.
            state.lastError?.let { error ->
                Text(
                    text = error,
                    style = IrisType.SystemLine.copy(color = IrisColors.AccentCritical),
                    modifier = Modifier.padding(
                        start = IrisSpacing.SM,
                        end = IrisSpacing.SM,
                        bottom = IrisSpacing.SM,
                    ),
                )
            }

            IrisConsoleInput(
                value = input,
                mode = mode,
                onValueChange = { input = it },
                onSubmit = {
                    viewModel.submit(input)
                    input = ""
                },
            )
        }
    }
}

/**
 * One console row. Message grouping needs the previous entry, so the whole list
 * is passed rather than a single item.
 */
@Composable
private fun ConsoleRow(entries: List<ConsoleEntry>, index: Int) {
    when (val entry = entries[index]) {
        is ConsoleEntry.Message -> {
            val previous = entries.getOrNull(index - 1)
            val grouped = previous is ConsoleEntry.Message &&
                previous.message.senderId == entry.message.senderId
            IrisMessage(message = entry.message, grouped = grouped)
        }

        is ConsoleEntry.System -> IrisSystemEvent(
            title = entry.title,
            lines = entry.lines,
            status = entry.status,
        )

        is ConsoleEntry.Echo -> Text(
            text = entry.text,
            style = IrisType.Command.copy(color = IrisColors.TextTertiary),
            modifier = Modifier.padding(top = IrisSpacing.MD),
        )
    }
}

/**
 * The persistent system line: node identity, transport state, relay depth.
 *
 * Always present, never loud — this is the ambient state readout that makes
 * IRIS feel like an operating layer rather than a chat window.
 */
@Composable
private fun StatusLine(
    state: MeshUiState,
    recipient: String?,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = IrisSpacing.Gutter, vertical = IrisSpacing.MD),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(IrisSpacing.SM),
    ) {
        Text(text = "IRIS", style = IrisType.Label.copy(color = IrisColors.TextSecondary))
        Text(text = state.nodeIdShort, style = IrisType.Meta)

        Spacer(Modifier.weight(1f))

        if (recipient != null) {
            Text(
                text = "→ ${recipient.take(8)}…",
                style = IrisType.Meta,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (state.relayQueued > 0) {
            IrisStatusChip(label = "Q${state.relayQueued}", tone = StatusTone.Warning)
        }
        IrisStatusChip(
            label = when (state.status) {
                MeshStatus.RUNNING -> "LINK"
                MeshStatus.STARTING -> "INIT"
                MeshStatus.IDLE -> "IDLE"
                MeshStatus.UNAVAILABLE -> "DOWN"
            },
            tone = when (state.status) {
                MeshStatus.RUNNING -> StatusTone.Active
                MeshStatus.STARTING -> StatusTone.Warning
                MeshStatus.IDLE -> StatusTone.Neutral
                MeshStatus.UNAVAILABLE -> StatusTone.Critical
            },
        )
    }
}

@Composable
private fun PermissionNotice(onGrant: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = IrisSpacing.Gutter, vertical = IrisSpacing.SM)
            .background(IrisColors.SurfacePrimary, iriscore.designsystem.IrisRadius.MD)
            .clickable(onClick = onGrant)
            .padding(IrisSpacing.MD),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                text = "PERMISSIONS",
                style = IrisType.Label.copy(color = IrisColors.AccentWarning),
            )
            Spacer(Modifier.height(IrisSpacing.XXS))
            Text(
                text = "Nearby-device access is required; without it the mesh carries no traffic.",
                style = IrisType.Secondary,
            )
        }
        Spacer(Modifier.width(IrisSpacing.MD))
        Text(text = "GRANT", style = IrisType.Label.copy(color = IrisColors.AccentPrimary))
    }
}
