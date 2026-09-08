package iriscore.ui.screens

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
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
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.app.ActivityCompat
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import iriscore.command.CommandRegistry
import iriscore.command.ConsoleInputParser
import iriscore.command.InputMode
import iriscore.designsystem.IrisColors
import iriscore.designsystem.IrisSizing
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
import iriscore.ui.components.TrustedPeersDialog
import iriscore.ui.state.ConsoleEntry
import iriscore.ui.state.MeshStatus
import iriscore.ui.state.MeshUiState
import iriscore.ui.state.TransportStatus
import iriscore.util.MeshPermissions
import kotlinx.coroutines.launch

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
    // HV-59: per-transport chip data from the 5-s ViewModel poll.
    val transportStates by viewModel.transportStates.collectAsStateWithLifecycle()
    // HV-56: contact book for name resolution throughout the screen.
    val contacts by viewModel.contacts.collectAsStateWithLifecycle()
    // HV-60: retry counter drives escalated help text in RetryNotice.
    val reconnectAttempts by viewModel.reconnectAttempts.collectAsStateWithLifecycle()
    val pairing by viewModel.pairing.collectAsStateWithLifecycle()
    val myPairingCode by viewModel.myPairingCode.collectAsStateWithLifecycle()

    var input by rememberSaveable { mutableStateOf("") }
    var selectedCommand by remember { mutableStateOf(0) }
    var peersOpen by rememberSaveable { mutableStateOf(false) }

    if (peersOpen) {
        TrustedPeersDialog(
            contacts = contacts,
            pairing = pairing,
            myPairingCode = myPairingCode,
            onDismiss = {
                peersOpen = false
                viewModel.resetPairing()
            },
            onLoadMyCode = viewModel::loadMyPairingCode,
            onBeginPairing = viewModel::beginPairing,
            onConfirmPairing = viewModel::confirmPairing,
            onResetPairing = viewModel::resetPairing,
            onSelect = viewModel::selectContact,
            onForget = viewModel::forgetContact,
        )
    }

    val mode = remember(input) { ConsoleInputParser.parse(input) }
    val matches = remember(mode) {
        (mode as? InputMode.Command)?.let { CommandRegistry.search(it.token) }.orEmpty()
    }
    val paletteVisible = mode is InputMode.Command && matches.isNotEmpty()

    // Keep the selection in range as the filtered set shrinks under the cursor.
    LaunchedEffect(matches) { selectedCommand = selectedCommand.coerceIn(0, maxOf(0, matches.size - 1)) }

    val context = LocalContext.current
    val activity = context as? Activity
    // HV-61: track whether we have ever issued the permission request so we can
    // distinguish "never asked" (shouldShowRationale == false, first launch) from
    // "permanently denied" (shouldShowRationale == false AFTER at least one ask).
    var hasRequestedPermissions by rememberSaveable { mutableStateOf(false) }
    var permissionsGranted by rememberSaveable { mutableStateOf(MeshPermissions.allGranted(context)) }
    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        // Re-read from the system: permissions already held before the request
        // are absent from the result map.
        permissionsGranted = MeshPermissions.allGranted(context)
    }

    // HV-61: "don't ask again" detection. After one denial, shouldShowRationale
    // flips to true (can ask again); after the user selects "Don't ask again" it
    // flips back to false. So permanently-denied = asked at least once AND every
    // missing permission now returns false from shouldShowRationale.
    val permanentlyDenied = hasRequestedPermissions && !permissionsGranted &&
        MeshPermissions.missing(context).none { perm ->
            activity != null && ActivityCompat.shouldShowRequestPermissionRationale(activity, perm)
        }

    // HV-30: a runtime grant revoked from Settings while the app is backgrounded
    // is otherwise never noticed — the composition's cached `permissionsGranted`
    // stays true and the UI shows RUNNING against dark radios. Re-read on every
    // resume; the core has already demoted the transport to Unavailable, so the
    // RetryNotice fires and this surfaces the PermissionNotice to re-grant.
    LifecycleResumeEffect(Unit) {
        permissionsGranted = MeshPermissions.allGranted(context)
        onPauseOrDispose { }
    }

    LaunchedEffect(permissionsGranted) {
        if (permissionsGranted && !iriscore.ui.MainActivity.suppressAutoStartMesh) {
            viewModel.ensureStarted()
        } else if (permissionsGranted) {
            // HV-21 bench: the Activity is up only to satisfy the OEM
            // "app has a foreground Activity" gate for WifiP2pManager
            // .discoverPeers(); the test's own engine owns the mesh.
        } else {
            MeshPermissions.missing(context)
                .takeIf { it.isNotEmpty() }
                ?.let {
                    hasRequestedPermissions = true
                    permissionLauncher.launch(it.toTypedArray())
                }
        }
    }

    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()

    // HV-63: only auto-scroll when the user is already near the bottom — within
    // 2 items of the last entry. If they've scrolled up to read history, let them:
    // new messages are counted and shown in the "N new ↓" pill instead.
    val atBottom by remember {
        derivedStateOf {
            val info = listState.layoutInfo
            val last = info.visibleItemsInfo.lastOrNull()?.index ?: return@derivedStateOf true
            last >= info.totalItemsCount - 3
        }
    }
    var unseenCount by remember { mutableIntStateOf(0) }

    // Scroll to tail when a new entry arrives — but only if already at the bottom.
    LaunchedEffect(entries.size) {
        if (entries.isNotEmpty()) {
            if (atBottom) {
                listState.animateScrollToItem(entries.lastIndex)
            } else {
                unseenCount++
            }
        }
    }
    // Reset the unseen badge whenever the user scrolls back to the bottom.
    LaunchedEffect(atBottom) {
        if (atBottom) unseenCount = 0
    }

    // HV-54: the composer floats over the transcript and grows upward as the
    // user types a multi-line message (up to 5 lines), but the list used to
    // reserve a fixed bottom padding sized for a one-line composer — so a
    // longer message covered the newest 1-3 messages with no way to scroll
    // them into view (the list believed it was already at the end). Measure
    // the actual floating column's height and feed it into the list's
    // bottom padding so the reserved space always matches what's really
    // floating above it.
    val density = LocalDensity.current
    var floatingHeight by remember { mutableStateOf(IrisSizing.InputHeight + IrisSpacing.XXL) }
    // Re-follow the tail as the composer grows/shrinks too, not just when a
    // new entry arrives — otherwise the last message can still slide back
    // under the composer as it expands without the list re-scrolling.
    LaunchedEffect(floatingHeight) {
        if (entries.isNotEmpty() && atBottom) listState.animateScrollToItem(entries.lastIndex)
    }

    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .background(IrisColors.BackgroundPrimary),
    ) {
        // Phase 6: the layout adapts by width rather than assuming a phone.
        // Past the tablet breakpoint the reading column is capped and centred
        // so message text does not run to 120+ characters a line, and the
        // gutters open up. Chrome still spans the full window.
        val wide = maxWidth >= IrisSizing.TabletBreakpoint
        val gutter = if (wide) IrisSizing.WideGutter else IrisSpacing.Gutter
        val contentWidth: Modifier =
            if (wide) {
                Modifier.widthIn(max = IrisSizing.ReadableContentWidth)
            } else {
                Modifier.fillMaxWidth()
            }

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
                contacts = contacts,
                permissionsGranted = permissionsGranted,
                transportStates = transportStates,
                onOpenPeers = { peersOpen = true },
                modifier = Modifier.windowInsetsPadding(WindowInsets.statusBars),
            )

            if (!permissionsGranted) {
                PermissionNotice(
                    permanentlyDenied = permanentlyDenied,
                    onGrant = {
                        if (permanentlyDenied) {
                            // HV-61: permanently denied — take the user to Settings.
                            context.startActivity(
                                Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                                    data = Uri.fromParts("package", context.packageName, null)
                                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                                },
                            )
                        } else {
                            MeshPermissions.missing(context)
                                .takeIf { it.isNotEmpty() }
                                ?.let {
                                    hasRequestedPermissions = true
                                    permissionLauncher.launch(it.toTypedArray())
                                }
                        }
                    },
                )
            }
            if (state.status == MeshStatus.UNAVAILABLE ||
                (state.status == MeshStatus.STARTING && reconnectAttempts > 0)
            ) {
                RetryNotice(
                    isRetrying = state.status == MeshStatus.STARTING,
                    reconnectAttempts = reconnectAttempts,
                    onRetry = { viewModel.reconnectMesh() },
                )
            }

            LazyColumn(
                state = listState,
                modifier = Modifier
                    .weight(1f)
                    .align(Alignment.CenterHorizontally)
                    .then(contentWidth),
                // The composer floats OVER this list, so the transcript has to
                // reserve room for it. Without this the newest entry — the one
                // the user most wants to read — sits underneath the input and
                // cannot be scrolled into view, because the list believes it is
                // already at the end.
                contentPadding = PaddingValues(
                    start = gutter,
                    end = gutter,
                    bottom = floatingHeight + IrisSpacing.SM,
                ),
            ) {
                items(count = entries.size, key = { entries[it].uid }) { index ->
                    ConsoleRow(
                        entries = entries,
                        index = index,
                        contacts = contacts,
                        onReply = { senderId -> viewModel.replyTo(senderId) },
                    )
                }
            }
        }

        // Input and palette float above the transcript, pinned to the bottom.
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .then(contentWidth)
                // One union, not two chained calls: the IME and navigation-bar
                // insets overlap, and applying them separately makes the
                // composer ride too high above the keyboard. `union` takes the
                // larger of the two, which is what "clear both" actually means.
                .windowInsetsPadding(WindowInsets.ime.union(WindowInsets.navigationBars))
                .padding(horizontal = IrisSpacing.MD, vertical = IrisSpacing.MD)
                .onGloballyPositioned { coords ->
                    val measured = with(density) { coords.size.height.toDp() }
                    if (measured.value > 0f) floatingHeight = measured
                },
        ) {
            // HV-63: when the user has scrolled up and new messages arrive, show
            // a pill instead of force-scrolling them back to the bottom.
            if (unseenCount > 0) {
                Row(
                    Modifier
                        .align(Alignment.CenterHorizontally)
                        .clickable {
                            scope.launch {
                                if (entries.isNotEmpty()) {
                                    listState.animateScrollToItem(entries.lastIndex)
                                }
                                unseenCount = 0
                            }
                        }
                        .background(IrisColors.AccentPrimary.copy(alpha = 0.92f), iriscore.designsystem.IrisRadius.Full)
                        .padding(horizontal = IrisSpacing.MD, vertical = IrisSpacing.XS),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(IrisSpacing.XS),
                ) {
                    Text(
                        text = "$unseenCount new  ↓",
                        style = IrisType.Label.copy(color = IrisColors.BackgroundPrimary),
                    )
                }
                Spacer(Modifier.height(IrisSpacing.SM))
            }

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
private fun ConsoleRow(
    entries: List<ConsoleEntry>,
    index: Int,
    contacts: Map<String, String>,
    onReply: (String) -> Unit,
) {
    when (val entry = entries[index]) {
        is ConsoleEntry.Message -> {
            val previous = entries.getOrNull(index - 1)
            val grouped = previous is ConsoleEntry.Message &&
                previous.message.senderId == entry.message.senderId
            IrisMessage(
                message = entry.message,
                grouped = grouped,
                onReply = onReply,
                contactName = contacts[entry.message.senderId],
            )
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
    contacts: Map<String, String>,
    permissionsGranted: Boolean,
    transportStates: List<TransportStatus>,
    onOpenPeers: () -> Unit,
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

        Text(
            text = "PEERS",
            style = IrisType.Label.copy(color = IrisColors.AccentPrimary),
            modifier = Modifier.clickable(onClick = onOpenPeers).padding(IrisSpacing.XS),
        )

        if (recipient != null) {
            Text(
                text = "→ ${contacts[recipient] ?: (recipient.take(8) + "…")}",
                style = IrisType.Meta,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (state.relayQueued > 0) {
            IrisStatusChip(label = "Q${state.relayQueued}", tone = StatusTone.Warning)
        }
        // HV-59: per-transport chips (BLE ●, WD ○, etc.) when running and the
        // 5-s poll has populated the list. Chips are compact — just label + dot.
        if (state.status == MeshStatus.RUNNING && transportStates.isNotEmpty()) {
            transportStates.forEach { t ->
                IrisStatusChip(
                    label = when (t.state) {
                        "Connected" -> "${t.label} ●"
                        "Available" -> "${t.label} ◐"
                        "Degraded" -> "${t.label} △"
                        else -> "${t.label} ○"
                    },
                    tone = when (t.state) {
                        "Connected", "Available" -> StatusTone.Active
                        else -> StatusTone.Warning
                    },
                )
            }
        }
        // HV-61: override the global status chip with "PERM" when permissions are
        // missing so the user immediately sees why the mesh is not carrying traffic
        // instead of a misleading IDLE/RUNNING chip.
        IrisStatusChip(
            label = when {
                !permissionsGranted -> "PERM"
                state.status == MeshStatus.RUNNING -> "LINK"
                state.status == MeshStatus.STARTING -> "INIT"
                state.status == MeshStatus.IDLE -> "IDLE"
                else -> "DOWN"
            },
            tone = when {
                !permissionsGranted -> StatusTone.Critical
                state.status == MeshStatus.RUNNING -> StatusTone.Active
                state.status == MeshStatus.STARTING -> StatusTone.Warning
                state.status == MeshStatus.IDLE -> StatusTone.Neutral
                else -> StatusTone.Critical
            },
        )
    }
}

@Composable
private fun PermissionNotice(permanentlyDenied: Boolean, onGrant: () -> Unit) {
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
                // HV-61: when permanently denied, explain that the system dialog
                // will no longer appear and the user must open Settings manually.
                text = if (permanentlyDenied) {
                    "Nearby-device access permanently denied. Open Settings to re-enable."
                } else {
                    "Nearby-device access is required; without it the mesh carries no traffic."
                },
                style = IrisType.Secondary,
            )
        }
        Spacer(Modifier.width(IrisSpacing.MD))
        Text(
            text = if (permanentlyDenied) "SETTINGS" else "GRANT",
            style = IrisType.Label.copy(color = IrisColors.AccentPrimary),
        )
    }
}

@Composable
private fun RetryNotice(isRetrying: Boolean, reconnectAttempts: Int, onRetry: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = IrisSpacing.Gutter, vertical = IrisSpacing.SM)
            .background(IrisColors.SurfacePrimary, iriscore.designsystem.IrisRadius.MD)
            .clickable(enabled = !isRetrying, onClick = onRetry)
            .padding(IrisSpacing.MD),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                text = if (isRetrying) "RECONNECTING…" else "MESH UNAVAILABLE",
                style = IrisType.Label.copy(
                    color = if (isRetrying) IrisColors.AccentWarning else IrisColors.AccentCritical,
                ),
            )
            Spacer(Modifier.height(IrisSpacing.XXS))
            Text(
                text = when {
                    isRetrying -> "Connecting to mesh — this may take a moment."
                    reconnectAttempts >= 2 -> "Still failing. Try toggling Bluetooth/Wi-Fi or restarting the app."
                    else -> "Tap to retry mesh connection."
                },
                style = IrisType.Secondary,
            )
        }
        if (!isRetrying) {
            Spacer(Modifier.width(IrisSpacing.MD))
            Text(text = "RETRY", style = IrisType.Label.copy(color = IrisColors.AccentPrimary))
        }
    }
}
